//! Certifications issued elsewhere: declare, review, list.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::credentials;

pub fn credential_routes() -> Router<AppState> {
    Router::new().route(
        "/users/me/credentials",
        get(my_credentials).post(declare_credential),
    )
}

pub fn admin_credential_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/credentials/pending", get(pending))
        .route("/admin/credentials/{id}/verify", post(verify))
        .route("/admin/credentials/{id}/refuse", post(refuse))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// Record a certification somebody else issued.
///
/// It arrives claimed and stays claimed until a reviewer opens the issuer's
/// page. The person adding it is the person it belongs to, which is exactly
/// why their word is not the check.
#[utoipa::path(
    post, path = "/api/users/me/credentials", tag = "profile",
    request_body = credentials::CredentialInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown issuer or level, or no public link", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<credentials::CredentialInput>,
) -> Result<Json<Value>, AppError> {
    let credential = credentials::declare(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "credential": credential }))))
}

#[utoipa::path(
    get, path = "/api/users/me/credentials", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_credentials(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let credentials = credentials::for_user(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "credentials": credentials }))))
}

#[utoipa::path(
    get, path = "/api/admin/credentials/pending", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn pending(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let pending = credentials::awaiting_review(&state.db, 100).await?;
    Ok(Json(build_response(json!({ "credentials": pending }))))
}

#[derive(Deserialize, ToSchema)]
pub struct ReviewBody {
    /// What was opened, and what it said.
    pub note: String,
}

#[utoipa::path(
    post, path = "/api/admin/credentials/{id}/verify", tag = "admin",
    params(("id" = Uuid, Path, description = "Credential id")),
    request_body = ReviewBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A note too short to be a record of a check", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Nothing waiting under that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn verify(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    credentials::verify(&state.db, id, auth.user_id, &body.note).await?;
    Ok(Json(build_response(json!({ "verified": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/credentials/{id}/refuse", tag = "admin",
    params(("id" = Uuid, Path, description = "Credential id")),
    request_body = ReviewBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A refusal without a reason", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Nothing waiting under that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn refuse(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let owner = credentials::refuse(&state.db, id, &body.note).await?;

    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(owner),
        "credential.refused",
    )
    .arg("reason", body.note.trim().to_string())
    .execute()
    .await;

    Ok(Json(build_response(json!({ "refused": true }))))
}
