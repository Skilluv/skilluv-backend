//! SKI-73 (P26 v2 B-02) — GitHub webhook receiver for reverse sync.
//!
//! Route: `POST /webhooks/github` (mounted OUTSIDE `/api`, same rationale
//! as the internal-tracker receiver — see `routes/linear_webhook.rs`).
//!
//! Env vars (soft-required — 503 if any is missing):
//!   GITHUB_WEBHOOK_SECRET      HMAC-SHA256 secret configured in GH webhook
//!   LINEAR_API_KEY             upstream API key with issue-update rights
//!   LINEAR_DONE_STATE_ID       workflow state UUID to move closed tickets to
//!
//! Only `X-GitHub-Event: issues` is processed. Everything else 202s so
//! GitHub does not disable the webhook (repeated non-2xx replies would).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;
use crate::errors::AppError;
use crate::services::ci_sync::{
    self, CheckRunEvent, CiWebhookDecision, MergeBonusDecision, PullRequestEvent,
};
use crate::services::linear_sync::{self, GithubIssueEvent, ReverseSyncDecision};

pub fn github_webhook_routes() -> Router<AppState> {
    Router::new().route("/webhooks/github", post(receive))
}

fn required_secret() -> Result<String, AppError> {
    std::env::var("GITHUB_WEBHOOK_SECRET")
        .map_err(|_| AppError::ServiceUnavailable("GITHUB_WEBHOOK_SECRET not configured".into()))
}

fn required_linear_env() -> Result<(String, String), AppError> {
    let api_key = std::env::var("LINEAR_API_KEY")
        .map_err(|_| AppError::ServiceUnavailable("LINEAR_API_KEY not configured".into()))?;
    let done_state = std::env::var("LINEAR_DONE_STATE_ID")
        .map_err(|_| AppError::ServiceUnavailable("LINEAR_DONE_STATE_ID not configured".into()))?;
    Ok((api_key, done_state))
}

async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
    let secret = required_secret()?;

    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            tracing::warn!("github webhook: missing X-Hub-Signature-256 header");
            AppError::Unauthorized
        })?;
    linear_sync::verify_github_signature(&secret, &body, signature)?;

    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match event_type {
        "issues" => {
            // Only the issues path needs the Linear write credentials.
            let (api_key, done_state) = required_linear_env()?;
            let event: GithubIssueEvent = serde_json::from_slice(&body).map_err(|e| {
                AppError::Validation(format!("github webhook payload parse failed: {e}"))
            })?;
            let decision =
                linear_sync::handle_github_issue_event(&state.db, &api_key, &done_state, event)
                    .await?;
            let moved = matches!(decision, ReverseSyncDecision::MovedToDone { .. });
            metrics::counter!("skilluv_github_reverse_sync_moved_total")
                .increment(u64::from(moved));
            Ok((
                StatusCode::ACCEPTED,
                Json(json!({"decision": format!("{decision:?}") })),
            ))
        }
        "check_run" => {
            // SKI-87 — advance submitted → ci_green on successful CI. LINEAR_*
            // secrets are not used on this path; only the shared HMAC secret.
            let event: CheckRunEvent = serde_json::from_slice(&body).map_err(|e| {
                AppError::Validation(format!("check_run payload parse failed: {e}"))
            })?;
            let decision = ci_sync::handle_check_run_event(&state.db, event).await?;
            if let CiWebhookDecision::Advanced { slice_count } = &decision {
                metrics::counter!("skilluv_ci_webhook_advanced_total")
                    .increment(*slice_count as u64);
            }
            Ok((
                StatusCode::ACCEPTED,
                Json(json!({"decision": format!("{decision:?}") })),
            ))
        }
        "pull_request" => {
            // SKI-89 — validated → merged bonus when the upstream maintainer
            // merges the PR. Silent no-op for slices that never reached
            // `validated` (upstream can merge without Skilluv validating).
            let event: PullRequestEvent = serde_json::from_slice(&body).map_err(|e| {
                AppError::Validation(format!("pull_request payload parse failed: {e}"))
            })?;
            let decision = ci_sync::handle_pull_request_event(&state.db, event).await?;
            if matches!(decision, MergeBonusDecision::Awarded { .. }) {
                metrics::counter!("skilluv_merge_bonus_awarded_total").increment(1);
            }
            Ok((
                StatusCode::ACCEPTED,
                Json(json!({"decision": format!("{decision:?}") })),
            ))
        }
        _ => {
            // Politely 202 anything else — GitHub disables endpoints that
            // return non-2xx repeatedly. `ping` (delivery test) lands here too.
            Ok((StatusCode::ACCEPTED, Json(json!({"decision": "Skipped"}))))
        }
    }
}
