use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::{ApiResponse, SimpleMessage};
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Notification;
use crate::services::notify;

pub fn notification_routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list_notifications))
        .route("/notifications/{id}/read", post(mark_read))
        .route("/notifications/read-all", post(mark_all_read))
        .route("/notifications/unread-count", get(unread_count))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct NotificationQuery {
    /// Filter by read state. Omit for both.
    pub read: Option<bool>,
    /// 1-based page number. Defaults to 1.
    pub page: Option<i64>,
    /// Rows per page. Clamped to `[1, 50]`. Defaults to 20.
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Pagination {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}

/// Paginated notifications response. Note: this endpoint historically
/// returned `data + pagination + meta` at the top level (not the usual
/// `ApiResponse<T>` envelope) — kept as-is to avoid breaking the front.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationsListResponse {
    pub data: Vec<Notification>,
    pub pagination: Pagination,
    pub meta: crate::api_response::MetaInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountResponse {
    pub unread_count: i64,
}

/// List the caller's notifications, paginated. Optional `read` filter
/// splits inbox vs archive views.
#[utoipa::path(
    get,
    path = "/api/notifications",
    tag = "profile",
    params(NotificationQuery),
    responses(
        (status = 200, description = "Paginated notifications", body = NotificationsListResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<NotificationsListResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let (notifications, total) = if let Some(read_filter) = query.read {
        let notifs: Vec<Notification> = sqlx::query_as(
            "SELECT * FROM notifications
              WHERE user_id = $1 AND read = $2
              ORDER BY COALESCE(updated_at, created_at) DESC
              LIMIT $3 OFFSET $4",
        )
        .bind(auth.user_id)
        .bind(read_filter)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = $2",
        )
        .bind(auth.user_id)
        .bind(read_filter)
        .fetch_one(&state.db)
        .await?;

        (notifs, count)
    } else {
        let notifs: Vec<Notification> = sqlx::query_as(
            "SELECT * FROM notifications
              WHERE user_id = $1
              ORDER BY COALESCE(updated_at, created_at) DESC
              LIMIT $2 OFFSET $3",
        )
        .bind(auth.user_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
                .bind(auth.user_id)
                .fetch_one(&state.db)
                .await?;

        (notifs, count)
    };

    Ok(Json(NotificationsListResponse {
        data: notifications,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages: (total as f64 / per_page as f64).ceil() as i64,
        },
        meta: crate::api_response::MetaInfo::now(),
    }))
}

/// Mark one notification as read. No-op when the notification is
/// already read or belongs to another user (silently ignored via the
/// user_id filter).
#[utoipa::path(
    post,
    path = "/api/notifications/{id}/read",
    tag = "profile",
    params(("id" = Uuid, Path, description = "Notification UUID")),
    responses(
        (status = 200, description = "Marked as read", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "notificationsMarkRead",
)]
pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let result = sqlx::query(
        "UPDATE notifications SET read = TRUE WHERE id = $1 AND user_id = $2 AND read = FALSE",
    )
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() > 0 {
        notify::decrement_counter(&mut state.redis.clone(), auth.user_id).await?;
    }

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Notification marked as read",
    ))))
}

/// Mark every unread notification for the caller as read. Also resets
/// the Redis unread counter used by the WS badge.
#[utoipa::path(
    post,
    path = "/api/notifications/read-all",
    tag = "profile",
    responses(
        (status = 200, description = "All notifications marked as read", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mark_all_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1 AND read = FALSE")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    notify::reset_counter(&mut state.redis.clone(), auth.user_id).await?;

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "All notifications marked as read",
    ))))
}

/// Cheap unread-badge counter — served from Redis with a DB fallback
/// when the cache is cold.
#[utoipa::path(
    get,
    path = "/api/notifications/unread-count",
    tag = "profile",
    responses(
        (status = 200, description = "Unread notification count", body = ApiResponse<UnreadCountResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<UnreadCountResponse>>, AppError> {
    let count = notify::unread_count(&state.db, &mut state.redis.clone(), auth.user_id).await?;

    Ok(Json(ApiResponse::new(UnreadCountResponse {
        unread_count: count,
    })))
}
