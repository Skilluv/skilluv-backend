//! P26.4 — Routes HTTP du sas compagnonnage débutant.
//!
//! Endpoints :
//!   GET  /api/beginner/verifications/questions/{template_id}  — apprenti tire N questions
//!   POST /api/beginner/verifications                          — apprenti soumet answers
//!   GET  /api/beginner/verifications/mine                     — apprenti voit sa progression
//!   GET  /api/beginner/verifications/queue                    — compagnon lit la file (cap-gated)
//!   POST /api/beginner/verifications/{id}/verdict             — compagnon rend verdict (cap-gated)

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::middleware::capabilities::require_capability;
use crate::services::apprentice_verification;

pub fn apprentice_verification_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/beginner/verifications/questions/{template_id}",
            get(pick_questions),
        )
        .route("/beginner/verifications", post(submit))
        .route("/beginner/verifications/mine", get(mine))
        .route("/beginner/verifications/queue", get(queue))
        .route("/beginner/verifications/{id}/verdict", post(record_verdict))
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// Apprenti — tire N questions
// ═══════════════════════════════════════════════════════════════════

/// The questions drawn for one template. Drawn per request, so two
/// people verifying the same template do not see the same set.
#[utoipa::path(
    get, path = "/api/beginner/verifications/questions/{template_id}", tag = "challenges",
    params(("template_id" = uuid::Uuid, Path, description = "The challenge template")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such template", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pick_questions(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(template_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let qs = apprentice_verification::pick_questions(&state.db, template_id).await?;
    Ok(Json(wrap(json!({ "questions": qs }))))
}

// ═══════════════════════════════════════════════════════════════════
// Apprenti — soumet answers
// ═══════════════════════════════════════════════════════════════════

/// Submit answers for a compagnon to look at.
#[utoipa::path(
    post, path = "/api/beginner/verifications", tag = "challenges",
    request_body = crate::services::apprentice_verification::SubmitPayload,
    responses(
        (status = 200, description = "Submitted for a compagnon to look at"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<apprentice_verification::SubmitPayload>,
) -> Result<Json<Value>, AppError> {
    let row =
        apprentice_verification::submit_verification(&state.db, auth.user_id, payload).await?;
    Ok(Json(wrap(json!({ "verification": row }))))
}

// ═══════════════════════════════════════════════════════════════════
// Apprenti — voit sa progression
// ═══════════════════════════════════════════════════════════════════

/// The verifications the caller asked for, whatever state they reached.
#[utoipa::path(
    get, path = "/api/beginner/verifications/mine", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn mine(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Value>, AppError> {
    let progress = apprentice_verification::get_progress(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({ "progress": progress }))))
}

// ═══════════════════════════════════════════════════════════════════
// Compagnon — file d'attente
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::IntoParams)]
struct QueueParams {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Verification requests waiting on a compagnon. Verifiers only.
#[utoipa::path(
    get, path = "/api/beginner/verifications/queue", tag = "moderation",
    params(QueueParams),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an apprentice verifier", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<QueueParams>,
) -> Result<Json<Value>, AppError> {
    require_capability(&state.db, auth.user_id, "apprentice_verifier").await?;
    let limit = params.limit.clamp(1, 100);
    let offset = params.offset.max(0);
    let rows = apprentice_verification::list_pending(&state.db, limit, offset).await?;
    Ok(Json(wrap(json!({ "pending": rows }))))
}

// ═══════════════════════════════════════════════════════════════════
// Compagnon — rend un verdict
// ═══════════════════════════════════════════════════════════════════

/// Record a compagnon's verdict on a verification request.
#[utoipa::path(
    post, path = "/api/beginner/verifications/{id}/verdict", tag = "moderation",
    params(("id" = uuid::Uuid, Path, description = "The verification request")),
    request_body = crate::services::apprentice_verification::VerdictPayload,
    responses(
        (status = 200, description = "The verdict was recorded"),
        (status = 403, description = "Not an apprentice verifier", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such verification request", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_verdict(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<apprentice_verification::VerdictPayload>,
) -> Result<Json<Value>, AppError> {
    require_capability(&state.db, auth.user_id, "apprentice_verifier").await?;
    let updated =
        apprentice_verification::record_verdict(&state.db, id, auth.user_id, payload).await?;
    Ok(Json(wrap(json!({ "verification": updated }))))
}
