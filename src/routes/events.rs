//! P17.6 — Events + participation (Hacktoberfest, Skilluv Fest, saisons).
//!
//! Routes minimales :
//!   - `GET  /api/events`                         : catalogue actif
//!   - `POST /api/events/{slug}/join`             : rejoint l'event (auth)
//!   - `GET  /api/users/me/events`                : mes events joined
//!
//! L'émission de l'event_stamp associé passe par `badge_engine` (P17.3) via
//! une rule dédiée dont les conditions matchent la participation.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn event_routes() -> Router<AppState> {
    Router::new()
        .route("/badge-events", get(list_events))
        .route("/badge-events/{slug}/join", post(join_event))
        .route("/users/me/badge-events", get(my_events))
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

#[derive(Debug, Serialize, sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    slug: String,
    name: String,
    description: String,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: Option<chrono::DateTime<chrono::Utc>>,
    visual_theme: serde_json::Value,
    is_partner: bool,
}

async fn list_events(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT id, slug, name, description, starts_at, ends_at, visual_theme, is_partner
         FROM events
         WHERE is_active = TRUE
         ORDER BY starts_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "events": rows }))))
}

async fn join_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let ev: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT id, is_active FROM events WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let (event_id, active) = ev.ok_or_else(|| AppError::NotFound(format!("event '{slug}'")))?;
    if !active {
        return Err(AppError::Validation("event is not active".into()));
    }
    sqlx::query(
        "INSERT INTO user_event_participation (user_id, event_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(event_id)
    .execute(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "joined": true, "event_slug": slug }))))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct MyEventRow {
    event_slug: String,
    event_name: String,
    joined_at: chrono::DateTime<chrono::Utc>,
    contribution_ref: Option<String>,
}

async fn my_events(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Value>, AppError> {
    let rows: Vec<MyEventRow> = sqlx::query_as(
        "SELECT e.slug AS event_slug, e.name AS event_name,
                uep.joined_at, uep.contribution_ref
         FROM user_event_participation uep
         JOIN events e ON e.id = uep.event_id
         WHERE uep.user_id = $1
         ORDER BY uep.joined_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "events": rows }))))
}
