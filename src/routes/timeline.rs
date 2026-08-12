//! SKI-39 (Post-MVP T1-04) — profile timeline read surface.
//!
//! Endpoints:
//!   GET  /api/users/{id}/timeline           (public when the profile is)
//!   POST /api/admin/users/{id}/backfill-timeline   (admin)
//!
//! Visibility mirrors `routes::profile`: a timeline is readable when the
//! profile is, i.e. `profile_hidden = FALSE AND is_banned = FALSE`, with
//! the owner always able to read their own. Hidden profiles answer 404
//! rather than 403 — the same answer as a nonexistent user, so the
//! endpoint cannot be used to enumerate who exists.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::admin_gate::AdminGate;
use crate::middleware::{AuthUser, OptionalAuth};
use crate::services::timeline;

const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 50;

pub fn timeline_routes() -> Router<AppState> {
    Router::new().route("/users/{id}/timeline", get(get_timeline))
}

pub fn admin_timeline_routes() -> Router<AppState> {
    Router::new().route(
        "/admin/users/{id}/backfill-timeline",
        post(admin_backfill_timeline),
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

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    /// Narrow to a single event type.
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

async fn get_timeline(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(user_id): Path<Uuid>,
    Query(q): Query<TimelineQuery>,
) -> Result<impl IntoResponse, AppError> {
    let viewer = auth.map(|a| a.user_id);
    let is_owner = viewer == Some(user_id);

    let readable: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM users
              WHERE id = $1
                AND ($2::BOOLEAN OR (profile_hidden = FALSE AND is_banned = FALSE))
         )",
    )
    .bind(user_id)
    .bind(is_owner)
    .fetch_one(&state.db)
    .await?;

    if !readable {
        return Err(AppError::NotFound(format!("user {user_id} not found")));
    }

    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);
    let event_type = q.event_type.as_deref();

    let events = timeline::list_for_user(&state.db, user_id, event_type, limit, offset).await?;
    let total = timeline::count_for_user(&state.db, user_id, event_type).await?;

    Ok(Json(wrap(json!({
        "events": events,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))))
}

/// Replay the timeline for one user from the source tables.
///
/// Idempotent — `rows_inserted: 0` means the timeline was already
/// complete, which is the normal answer and the point of running it.
///
/// `AdminGate` only enforces the admin origin and mandatory 2FA; it
/// deliberately does not check the role (see its doc comment). The
/// capability check below is what actually restricts this to admins.
async fn admin_backfill_timeline(
    _gate: AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("user {user_id} not found")));
    }

    let report = timeline::backfill(&state.db, Some(user_id)).await?;
    Ok(Json(wrap(json!({
        "user_id": user_id,
        "rows_inserted": report.total(),
        "detail": report,
    }))))
}
