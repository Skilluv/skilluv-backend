//! `POST /api/webhooks/payments/{provider}` — one endpoint, every provider.
//!
//! The provider comes from the path rather than from the body: the body is
//! the part an attacker controls, and picking a verifier from it would let
//! them choose which secret their forgery is checked against.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;
use crate::errors::AppError;
use crate::services::payment_webhook_sources;
use crate::services::payment_webhooks::{self, Outcome};

pub fn routes() -> Router<AppState> {
    Router::new().route("/webhooks/payments/{provider}", post(receive))
}

/// Headers providers sign with, in the order we look for them.
///
/// One list rather than one per source: a provider that changes its header
/// name — which happens — should not need a code change to keep working.
const SIGNATURE_HEADERS: [&str; 5] = [
    "stripe-signature",
    "x-fedapay-signature",
    "x-signature",
    "x-callback-signature",
    "signature",
];

#[utoipa::path(
    post,
    path = "/api/webhooks/payments/{provider}",
    tag = "payments",
    params(("provider" = String, Path, description = "Provider name, e.g. stripe, fedapay, mtn")),
    // The body is whatever the provider sends and is verified byte for byte
    // against their signature, so it is taken raw rather than deserialised.
    request_body(content = String, description = "The provider's raw event payload"),
    responses(
        (status = 200, description = "Event accepted", body = serde_json::Value),
        (status = 401, description = "Signature did not verify"),
        (status = 404, description = "No such provider configured on this deployment"),
    ),
)]
pub async fn receive(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let sources = payment_webhook_sources::sources_from_env();
    let Some(source) = sources.iter().find(|s| s.name() == provider) else {
        // 404 rather than 200: a provider we hold no secret for is one whose
        // events we cannot verify, and acknowledging them would tell it to
        // stop retrying something we never received.
        tracing::warn!(
            provider = %provider,
            "payment webhook for a provider this deployment has no secret for"
        );
        return Err(AppError::NotFound(format!("payment provider {provider}")));
    };

    let signature = SIGNATURE_HEADERS
        .iter()
        .find_map(|name| headers.get(*name))
        .and_then(|v| v.to_str().ok());

    let body = std::str::from_utf8(&body)
        .map_err(|_| AppError::Validation("webhook body is not valid UTF-8".into()))?;

    let outcome = payment_webhooks::receive(&state.db, source.as_ref(), body, signature).await?;

    // 200 on a duplicate and on an event we chose not to act on: both mean
    // "we have this, stop retrying". Only a signature failure and a genuine
    // processing error are worth a retry, and those return an error status.
    Ok((
        StatusCode::OK,
        Json(json!({
            "received": true,
            "outcome": match &outcome {
                Outcome::Applied(kind) => kind.as_str(),
                Outcome::Duplicate => "duplicate",
                Outcome::Ignored => "ignored",
            },
        })),
    ))
}
