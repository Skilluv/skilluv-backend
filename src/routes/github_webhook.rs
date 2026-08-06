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
use crate::services::linear_sync::{self, GithubIssueEvent, ReverseSyncDecision};

pub fn github_webhook_routes() -> Router<AppState> {
    Router::new().route("/webhooks/github", post(receive))
}

fn required_env() -> Result<(String, String, String), AppError> {
    let secret = std::env::var("GITHUB_WEBHOOK_SECRET")
        .map_err(|_| AppError::ServiceUnavailable("GITHUB_WEBHOOK_SECRET not configured".into()))?;
    let api_key = std::env::var("LINEAR_API_KEY")
        .map_err(|_| AppError::ServiceUnavailable("LINEAR_API_KEY not configured".into()))?;
    let done_state = std::env::var("LINEAR_DONE_STATE_ID")
        .map_err(|_| AppError::ServiceUnavailable("LINEAR_DONE_STATE_ID not configured".into()))?;
    Ok((secret, api_key, done_state))
}

async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
    let (secret, api_key, done_state) = required_env()?;

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

    if event_type != "issues" {
        // Politely 202 anything else — GitHub disables endpoints that
        // return non-2xx repeatedly. `ping` (delivery test) lands here too.
        return Ok((StatusCode::ACCEPTED, Json(json!({"decision": "Skipped"}))));
    }

    let event: GithubIssueEvent = serde_json::from_slice(&body)
        .map_err(|e| AppError::Validation(format!("github webhook payload parse failed: {e}")))?;

    let decision =
        linear_sync::handle_github_issue_event(&state.db, &api_key, &done_state, event).await?;

    let moved = matches!(decision, ReverseSyncDecision::MovedToDone { .. });
    metrics::counter!("skilluv_github_reverse_sync_moved_total").increment(u64::from(moved));

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"decision": format!("{decision:?}") })),
    ))
}
