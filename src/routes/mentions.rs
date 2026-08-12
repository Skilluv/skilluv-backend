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

#[derive(Debug, Deserialize)]
pub struct ListMentionsQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
    #[serde(default)]
    pub unread_only: bool,
}

async fn list_mine(
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

async fn read_one(
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

async fn read_all(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let marked = mentions::mark_all_read(&state.db, auth.user_id).await?;
    Ok(Json(json!({
        "data": { "marked": marked },
        "meta": meta(),
    })))
}
