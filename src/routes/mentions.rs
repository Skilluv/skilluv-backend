//! SKI-286 — mention inbox.
//!
//! Endpoints:
//!   GET  /api/users/me/mentions            (auth)
//!   POST /api/users/me/mentions/{id}/read  (auth)
//!   POST /api/users/me/mentions/read-all   (auth)
//!
//! Everything is scoped to the caller — a mention is addressed to exactly
//! one person, so there is no target parameter to get wrong. Visibility of
//! the underlying content is enforced in `services::mentions`, not here.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::mentions;

const DEFAULT_PER_PAGE: i64 = 20;
const MAX_PER_PAGE: i64 = 100;

pub fn mention_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/mentions", get(list_mine))
        .route("/users/me/mentions/read-all", post(read_all))
        .route("/users/me/mentions/{id}/read", post(read_one))
}

fn meta() -> serde_json::Value {
    json!({
        "request_id": Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListMentionsQuery {
    #[serde(default)]
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[serde(default)]
    #[param(minimum = 1, maximum = 100)]
    pub per_page: Option<i64>,
    /// Restrict to mentions the caller has not opened yet.
    #[serde(default)]
    pub unread_only: bool,
}

/// Paginated mention inbox.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MentionListResponse {
    pub data: Vec<mentions::Mention>,
    pub pagination: crate::api_response::Pagination,
    pub meta: crate::api_response::MetaInfo,
}

/// Result of marking one mention read.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MentionRead {
    pub id: Uuid,
    /// When it was first opened. Unchanged on a repeated call — marking read
    /// is idempotent.
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MentionReadResponse {
    pub data: MentionRead,
    pub meta: crate::api_response::MetaInfo,
}

/// Number of mentions transitioned from unread to read.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MentionsMarked {
    pub marked: u64,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MentionsMarkedResponse {
    pub data: MentionsMarked,
    pub meta: crate::api_response::MetaInfo,
}

/// Mentions addressed to the caller, newest first.
///
/// SKI-293 — this route replaces `GET /api/social/mentions/me`, which was
/// documented with an empty schema while this one carried the real contract.
#[utoipa::path(
    get,
    path = "/api/users/me/mentions",
    operation_id = "mentionsListMine",
    tag = "mentions",
    params(ListMentionsQuery),
    responses(
        (status = 200, description = "Mention inbox (paginated)", body = MentionListResponse),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListMentionsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let offset = (page - 1) * per_page;

    let items =
        mentions::list_for_user(&state.db, auth.user_id, q.unread_only, per_page, offset).await?;
    let total = mentions::count_for_user(&state.db, auth.user_id, q.unread_only).await?;
    // Ceiling division without `div_ceil`, which is still unstable for
    // signed integers on this toolchain.
    let total_pages = if total == 0 {
        0
    } else {
        (total + per_page - 1) / per_page
    };

    Ok(Json(json!({
        "data": items,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
        "meta": meta(),
    })))
}

/// Mark one mention as read. Idempotent: a second call keeps the first
/// timestamp.
#[utoipa::path(
    post,
    path = "/api/users/me/mentions/{id}/read",
    tag = "mentions",
    params(("id" = Uuid, Path, description = "Mention UUID")),
    responses(
        (status = 200, description = "Marked read", body = MentionReadResponse),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such mention for this caller", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_one(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let read_at = mentions::mark_read(&state.db, auth.user_id, id).await?;
    Ok(Json(json!({
        "data": { "id": id, "read_at": read_at },
        "meta": meta(),
    })))
}

/// Mark every unread mention as read.
#[utoipa::path(
    post,
    path = "/api/users/me/mentions/read-all",
    tag = "mentions",
    responses(
        (status = 200, description = "Count of mentions transitioned to read", body = MentionsMarkedResponse),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_all(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let marked = mentions::mark_all_read(&state.db, auth.user_id).await?;
    Ok(Json(json!({
        "data": { "marked": marked },
        "meta": meta(),
    })))
}
