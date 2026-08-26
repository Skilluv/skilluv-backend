//! SKI-72 (P26 v2 B-01) — inbound webhook receiver for the internal-tracker
//! sync bot. See `src/services/linear_sync.rs` for the naming policy.
//!
//! Route: `POST /webhooks/linear` (mounted OUTSIDE `/api` so external
//! webhook senders do not accidentally hit our API rate-limits, and so
//! signature verification is not conflated with JWT auth).
//!
//! Env vars (soft-required — the endpoint returns 503 if either is missing):
//!   LINEAR_WEBHOOK_SECRET      shared HMAC-SHA256 secret
//!   SKILLUV_BOT_GITHUB_TOKEN   PAT / GitHub App token with `issues:write`
//!                              on the four Skilluv repos.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;
use crate::errors::AppError;
use crate::services::linear_sync::{self, InboundEvent, SyncDecision};

pub fn linear_webhook_routes() -> Router<AppState> {
    Router::new().route("/webhooks/linear", post(receive))
}

/// Reads env at request time (not at boot) so operators can rotate secrets
/// without a restart. Returns `Err` when either variable is missing so the
/// caller responds 503 instead of processing an unverified payload.
fn required_env() -> Result<(String, String), AppError> {
    let secret = std::env::var("LINEAR_WEBHOOK_SECRET")
        .map_err(|_| AppError::ServiceUnavailable("LINEAR_WEBHOOK_SECRET not configured".into()))?;
    let token = std::env::var("SKILLUV_BOT_GITHUB_TOKEN").map_err(|_| {
        AppError::ServiceUnavailable("SKILLUV_BOT_GITHUB_TOKEN not configured".into())
    })?;
    Ok((secret, token))
}

/// Linear webhook. HMAC-signed; an unsigned or mis-signed body is refused
/// before anything is read from it.
#[utoipa::path(
    // Mounted at the root, not under `/api` — see the module note above. The
    // document claimed `/api/webhooks/linear`, which 404s.
    post, path = "/webhooks/linear",
    operation_id = "linearWebhookReceive",
    tag = "webhooks",
    request_body(content = String, description = "The raw Linear payload the signature covers"),
    responses(
        (status = 202, description = "Accepted for processing"),
        (status = 401, description = "The HMAC signature did not verify", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
    let (secret, token) = required_env()?;

    let signature = headers
        .get("linear-signature")
        .or_else(|| headers.get("x-hub-signature-256"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            tracing::warn!("linear webhook: missing signature header");
            AppError::Unauthorized
        })?;
    linear_sync::verify_signature(&secret, &body, signature)?;

    let event: InboundEvent = serde_json::from_slice(&body)
        .map_err(|e| AppError::Validation(format!("webhook payload parse failed: {e}")))?;

    let decision = linear_sync::handle_event(&state.db, &token, event).await?;

    let (created, updated, ignored) = match &decision {
        SyncDecision::Created { .. } => (1u32, 0u32, 0u32),
        SyncDecision::Updated { .. } => (0, 1, 0),
        _ => (0, 0, 1),
    };
    metrics::counter!("skilluv_linear_sync_created_total").increment(created as u64);
    metrics::counter!("skilluv_linear_sync_updated_total").increment(updated as u64);
    metrics::counter!("skilluv_linear_sync_ignored_total").increment(ignored as u64);

    let body_json = json!({
        "decision": format!("{decision:?}"),
    });
    Ok((StatusCode::ACCEPTED, Json(body_json)))
}
