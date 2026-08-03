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
        .route(
            "/beginner/verifications/{id}/verdict",
            post(record_verdict),
        )
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

async fn pick_questions(
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

async fn submit(
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

async fn mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let progress = apprentice_verification::get_progress(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({ "progress": progress }))))
}

// ═══════════════════════════════════════════════════════════════════
// Compagnon — file d'attente
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct QueueParams {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    20
}

async fn queue(
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

async fn record_verdict(
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
