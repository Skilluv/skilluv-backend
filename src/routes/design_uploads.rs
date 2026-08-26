//! Handing in a file too large to send through the API.
//!
//! Five endpoints, and none of them carries a byte of the file. `init` hands
//! out presigned PUT URLs, the client uploads straight to the object store,
//! and `complete` asks the store to assemble what arrived.
//!
//! That is not an optimisation. Five gigabytes through an axum handler holds a
//! connection and a buffer for as long as somebody's connection takes, and it
//! does so for every concurrent upload — one designer on a rural line would
//! degrade the API for everybody.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUserComplete;
use crate::services::design_uploads;

/// How long a download link lives by default. Long enough to open a file in
/// another application, short enough that a link pasted in a chat is stale by
/// the time it is forwarded.
const DEFAULT_DOWNLOAD_TTL: u32 = 60 * 60;
const MAX_DOWNLOAD_TTL: u32 = 24 * 60 * 60;

pub fn design_upload_routes() -> Router<AppState> {
    Router::new()
        .route("/design/uploads", post(init))
        .route("/design/uploads/{id}/parts", get(parts))
        .route("/design/uploads/{id}/complete", post(complete))
        .route("/design/uploads/{id}/preview-url", post(preview_url))
        .route("/design/uploads/{id}/download-url", get(download_url))
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

/// Open an upload and get the URLs to push the parts to.
///
/// Requires a completed profile rather than merely a session: an upload
/// reserves storage that somebody pays for, and an unverified address is the
/// cheapest thing in the world to make.
#[utoipa::path(
    post, path = "/api/design/uploads", tag = "design",
    request_body = design_uploads::InitInput,
    responses(
        (status = 201, description = "session opened, with one presigned URL per part"),
        (status = 400, description = "unknown subtype, or larger than the subtype allows",
         body = crate::api_response::ErrorResponse),
        (status = 403, description = "that slice is not yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn init(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Json(body): Json<design_uploads::InitInput>,
) -> Result<impl IntoResponse, AppError> {
    let started = design_uploads::init(&state.db, &state.storage, auth.user_id, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "upload": started }))),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PartsQuery {
    /// First part wanted, 1-based.
    pub from: i32,
    /// Last part wanted, inclusive.
    pub to: i32,
}

/// Fresh URLs for a range of parts.
///
/// This is both "my URLs expired" and "I am resuming after a crash". The
/// client asks for the parts it has no ETag for; nothing here needs to know
/// which those are, which is what makes the upload resumable without any
/// bookkeeping on this side.
#[utoipa::path(
    get, path = "/api/design/uploads/{id}/parts", tag = "design",
    params(("id" = Uuid, Path, description = "upload session id"), PartsQuery),
    responses(
        (status = 200, description = "presigned URLs for the requested parts"),
        (status = 404, description = "no such upload", body = crate::api_response::ErrorResponse),
        (status = 409, description = "the upload is finished or expired", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn parts(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Path(id): Path<Uuid>,
    Query(q): Query<PartsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let parts =
        design_uploads::part_urls(&state.db, &state.storage, auth.user_id, id, q.from, q.to)
            .await?;
    Ok(Json(wrap(json!({ "parts": parts }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteBody {
    /// Every part, with the ETag the object store returned for it.
    pub parts: Vec<design_uploads::CompletedPart>,
}

/// Assemble the parts.
#[utoipa::path(
    post, path = "/api/design/uploads/{id}/complete",
    operation_id = "designUploadsComplete",
    tag = "design",
    params(("id" = Uuid, Path, description = "upload session id")),
    request_body = CompleteBody,
    responses(
        (status = 200, description = "assembled, with the size the store actually holds"),
        (status = 400, description = "wrong number of parts, or larger than declared",
         body = crate::api_response::ErrorResponse),
        (status = 409, description = "the upload is finished or expired", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn complete(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Path(id): Path<Uuid>,
    Json(body): Json<CompleteBody>,
) -> Result<impl IntoResponse, AppError> {
    let session =
        design_uploads::complete(&state.db, &state.storage, auth.user_id, id, body.parts).await?;
    Ok(Json(wrap(json!({ "upload": session }))))
}

/// A URL to PUT the preview to.
///
/// Required for the subtypes a browser cannot open — a scene file, a project
/// file, an audio master. Nothing here renders one: the person who made the
/// file picks the frame that represents it, which is a better frame than any
/// heuristic would find and costs no render farm.
#[utoipa::path(
    post, path = "/api/design/uploads/{id}/preview-url", tag = "design",
    params(("id" = Uuid, Path, description = "upload session id")),
    responses(
        (status = 200, description = "a presigned PUT for the preview"),
        (status = 404, description = "no such upload", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn preview_url(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let url =
        design_uploads::preview_upload_url(&state.db, &state.storage, auth.user_id, id).await?;
    Ok(Json(wrap(json!({ "url": url }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct DownloadQuery {
    /// How long the link should live, in seconds. Capped at a day.
    #[param(minimum = 60, maximum = 86400)]
    pub ttl_seconds: Option<u32>,
}

/// A link to the file, for a limited time.
#[utoipa::path(
    get, path = "/api/design/uploads/{id}/download-url", tag = "design",
    params(("id" = Uuid, Path, description = "upload session id"), DownloadQuery),
    responses(
        (status = 200, description = "a presigned GET"),
        (status = 409, description = "the upload is not finished", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn download_url(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Path(id): Path<Uuid>,
    Query(q): Query<DownloadQuery>,
) -> Result<impl IntoResponse, AppError> {
    let ttl = q
        .ttl_seconds
        .unwrap_or(DEFAULT_DOWNLOAD_TTL)
        .clamp(60, MAX_DOWNLOAD_TTL);
    let url =
        design_uploads::download_url(&state.db, &state.storage, auth.user_id, id, ttl).await?;
    Ok(Json(wrap(json!({ "url": url, "expires_in": ttl }))))
}
