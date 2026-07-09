use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Notification;
use crate::services::NotificationService;

pub fn notification_routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list_notifications))
        .route("/notifications/{id}/read", post(mark_read))
        .route("/notifications/read-all", post(mark_all_read))
        .route("/notifications/unread-count", get(unread_count))
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct NotificationQuery {
    read: Option<bool>,
    page: Option<i64>,
    per_page: Option<i64>,
}

// GET /api/notifications
async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let (notifications, total) = if let Some(read_filter) = query.read {
        let notifs: Vec<Notification> = sqlx::query_as(
            "SELECT * FROM notifications WHERE user_id = $1 AND read = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4",
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
            "SELECT * FROM notifications WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
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

    Ok(Json(json!({
        "data": notifications,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// POST /api/notifications/:id/read
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = sqlx::query(
        "UPDATE notifications SET read = TRUE WHERE id = $1 AND user_id = $2 AND read = FALSE",
    )
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() > 0 {
        NotificationService::decrement_counter(&mut state.redis.clone(), auth.user_id).await?;
    }

    Ok(Json(build_response(json!({
        "message": "Notification marked as read"
    }))))
}

// POST /api/notifications/read-all
async fn mark_all_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1 AND read = FALSE")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    NotificationService::reset_counter(&mut state.redis.clone(), auth.user_id).await?;

    Ok(Json(build_response(json!({
        "message": "All notifications marked as read"
    }))))
}

// GET /api/notifications/unread-count
async fn unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let count =
        NotificationService::unread_count(&state.db, &mut state.redis.clone(), auth.user_id)
            .await?;

    Ok(Json(build_response(json!({ "unread_count": count }))))
}
