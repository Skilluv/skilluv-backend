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
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn event_routes() -> Router<AppState> {
    Router::new()
        .route("/badge-events", get(list_events))
        .route("/badge-events/{slug}/join", post(join_event))
        .route("/users/me/badge-events", get(my_events))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct EventRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Free-form theme payload (colours, hero image URL, etc.) — the
    /// front renders it; the backend does not care about the shape.
    pub visual_theme: serde_json::Value,
    /// True for partner-hosted events (Hacktoberfest, external
    /// hackathons); false for Skilluv-native events.
    pub is_partner: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventsListResponse {
    pub events: Vec<EventRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinEventResponse {
    pub joined: bool,
    pub event_slug: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct MyEventRow {
    pub event_slug: String,
    pub event_name: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    /// URL of the PR / repo / contribution counted for this event.
    pub contribution_ref: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyEventsResponse {
    pub events: Vec<MyEventRow>,
}

/// List every active badge-event (Hacktoberfest, Skilluv Fest, etc.).
/// Public — no auth required.
#[utoipa::path(
    get,
    path = "/api/badge-events",
    tag = "profile",
    responses(
        (status = 200, description = "Active events", body = ApiResponse<EventsListResponse>),
    ),
)]
pub async fn list_events(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<EventsListResponse>>, AppError> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT id, slug, name, description, starts_at, ends_at, visual_theme, is_partner
         FROM events
         WHERE is_active = TRUE
         ORDER BY starts_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(EventsListResponse { events: rows })))
}

/// Enroll the current user in an event. Idempotent (ON CONFLICT DO
/// NOTHING). The associated event_stamp is emitted separately by the
/// badge engine when the event's rule fires.
#[utoipa::path(
    post,
    path = "/api/badge-events/{slug}/join",
    tag = "profile",
    params(("slug" = String, Path, description = "Event slug")),
    responses(
        (status = 200, description = "Joined (or already joined)", body = ApiResponse<JoinEventResponse>),
        (status = 400, description = "Event exists but is inactive", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Event slug unknown", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn join_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<JoinEventResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(JoinEventResponse {
        joined: true,
        event_slug: slug,
    })))
}

/// List every event the caller has joined, with the enriched name +
/// contribution_ref for the "My events" dashboard tile.
#[utoipa::path(
    get,
    path = "/api/users/me/badge-events",
    tag = "profile",
    responses(
        (status = 200, description = "Caller's joined events", body = ApiResponse<MyEventsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_events(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyEventsResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(MyEventsResponse { events: rows })))
}
