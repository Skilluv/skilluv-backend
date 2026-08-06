//! P26 v2 SKI-81 / SKI-82 — validator candidacy + admin invitation routes.
//!
//! Two route groups:
//!   `validator_application_routes()`       — user-facing (apply, accept
//!                                             invitation, withdraw)
//!   `admin_validator_application_routes()` — admin-only (invite, approve
//!                                             candidacy, reject)
//! The admin group is mounted through `admin_gate` in `lib.rs`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::validator_applications::{self, ApplyInput, InviteInput};

pub fn validator_application_routes() -> Router<AppState> {
    Router::new()
        .route("/me/apply-as-validator", post(apply))
        .route("/me/validator-applications", get(my_applications))
        .route(
            "/validator-applications/{id}/accept",
            post(accept_invitation),
        )
        .route("/validator-applications/{id}/withdraw", post(withdraw))
}

pub fn admin_validator_application_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/validators/invite", post(admin_invite))
        .route(
            "/admin/validator-applications/{id}/approve",
            post(admin_approve),
        )
        .route(
            "/admin/validator-applications/{id}/reject",
            post(admin_reject),
        )
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

// ─── User routes ─────────────────────────────────────────────────

/// SKI-81 — user self-nominates for a validator domain.
pub async fn apply(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ApplyInput>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::apply(&state.db, auth.user_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "application": app }))),
    ))
}

pub async fn my_applications(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<validator_applications::ValidatorApplication> = sqlx::query_as(
        "SELECT * FROM validator_applications WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "applications": rows }))))
}

/// SKI-82 — invitee accepts the pending invitation.
pub async fn accept_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::accept(&state.db, id, auth.user_id, None).await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let app =
        validator_applications::resolve(&state.db, id, auth.user_id, "withdrawn", None, false)
            .await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

// ─── Admin routes ────────────────────────────────────────────────

/// SKI-82 — admin invites a user (bypasses stats).
pub async fn admin_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<InviteInput>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::invite(&state.db, auth.user_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "application": app }))),
    ))
}

pub async fn admin_approve(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let app =
        validator_applications::accept(&state.db, id, auth.user_id, Some(auth.user_id)).await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectBody {
    pub reason: String,
}

pub async fn admin_reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::resolve(
        &state.db,
        id,
        auth.user_id,
        "rejected",
        Some(body.reason),
        true,
    )
    .await?;
    Ok(Json(wrap(json!({ "application": app }))))
}
