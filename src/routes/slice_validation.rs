//! P26 v2 Phase D — routes for the validator workflow.
//!
//! All endpoints require an authenticated user with the
//! `challenge_validator:{domain}` capability matching the slice's
//! primary_domain (enforced at the service layer). See SKI-80 and
//! `middleware::capabilities::require_challenge_validator_for`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::slice_validation;

pub fn slice_validation_routes() -> Router<AppState> {
    Router::new()
        .route("/slices/{id}/validation/pickup", post(pickup))
        .route("/slices/{id}/validation/approve", post(approve))
        .route("/slices/{id}/validation/reject", post(reject))
        .route("/me/validation/queue", get(my_queue))
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// SKI-83 — POST /api/slices/{id}/validation/pickup
#[utoipa::path(
    post, path = "/api/slices/{id}/validation/pickup", tag = "slices",
    params(("id" = uuid::Uuid, Path, description = "The slice to pick up for validation")),
    responses(
        (status = 200, description = "The slice is now yours to validate"),
        (status = 403, description = "Not eligible to validate this slice", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Somebody else picked it up first", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pickup(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let slice = slice_validation::pickup(&state.db, id, auth.user_id).await?;
    Ok((StatusCode::OK, Json(wrap(json!({ "slice": slice })))))
}

/// SKI-84 — POST /api/slices/{id}/validation/approve
#[utoipa::path(
    post, path = "/api/slices/{id}/validation/approve", tag = "slices",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, description = "Validated"),
        (status = 403, description = "The slice is not yours to validate", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn approve(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let slice =
        slice_validation::approve(&state.db, &state.config.jwt_secret, id, auth.user_id).await?;
    Ok((StatusCode::OK, Json(wrap(json!({ "slice": slice })))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SliceValidationRejectBody)]
pub struct RejectBody {
    /// Feedback shown to the challenger. Required, ≤ 2000 chars.
    pub reason: String,
    /// What kind of blocker this is: `ci_failing`, `tests_missing`,
    /// `docs_missing`, `review_comments`, `scope_mismatch`, `out_of_depth`.
    ///
    /// Required alongside the prose. The prose says what is wrong with this
    /// submission; the category says what kind of thing to do about it, and
    /// is what lets an operator see that a project rejects everything for
    /// missing tests.
    pub blocking_reason: String,
}

/// SKI-85 — POST /api/slices/{id}/validation/reject
#[utoipa::path(
    post, path = "/api/slices/{id}/validation/reject",
    operation_id = "sliceValidationReject",
    tag = "slices",
    params(("id" = uuid::Uuid, Path)),
    request_body = RejectBody,
    responses(
        (status = 200, description = "Sent back, with what has to change"),
        (status = 403, description = "The slice is not yours to validate", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<impl IntoResponse, AppError> {
    let slice = slice_validation::reject(
        &state.db,
        id,
        auth.user_id,
        &body.reason,
        &body.blocking_reason,
    )
    .await?;
    Ok((StatusCode::OK, Json(wrap(json!({ "slice": slice })))))
}

/// SKI-86 — GET /api/me/validation/queue
#[utoipa::path(
    get, path = "/api/me/validation/queue", tag = "slices",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let slices = slice_validation::my_queue(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({ "slices": slices }))))
}
