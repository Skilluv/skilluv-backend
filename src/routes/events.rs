//! Events — the whole of them, not just the stamp.
//!
//! Migration 0093 gave events a slug, a window and a visual theme: enough to
//! say "this person was at Hacktoberfest". They are now the object the brand
//! line sells against — a type, a place, a jury, a livestream, sponsors — and
//! there is still one table and one set of routes, because a hackathon that
//! issues a stamp *and* sells sponsorship is one event with one date.
//!
//! The stamp itself is still emitted by `badge_engine`, which reads the
//! participation rows below.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::sponsorship;

pub const ROLES: &[&str] = &["participant", "jury", "organizer", "speaker", "sponsor_rep"];

pub fn event_routes() -> Router<AppState> {
    Router::new()
        .route("/events", get(list_events))
        .route("/events/{slug}", get(read_event))
        .route("/events/{slug}/join", post(join_event))
        .route("/users/me/events", get(my_events))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct EventRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub event_type: String,
    pub domain_focus: Vec<String>,
    pub location_type: String,
    pub location_details: Value,
    pub max_participants: Option<i32>,
    pub showcase_page_url: Option<String>,
    pub status: String,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Free-form theme payload (colours, hero image URL, etc.) — the front
    /// renders it; the backend does not care about the shape.
    pub visual_theme: Value,
    /// True for partner-hosted events (Hacktoberfest, external hackathons);
    /// false for Skilluv-native ones.
    pub is_partner: bool,
}

const EVENT_SELECT: &str = r#"
    SELECT id, slug, name, description, event_type, domain_focus, location_type,
           location_details, max_participants, showcase_page_url, status,
           starts_at, ends_at, visual_theme, is_partner
      FROM events
"#;

#[derive(Debug, Serialize, ToSchema)]
pub struct EventsListResponse {
    pub events: Vec<EventRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinEventResponse {
    pub joined: bool,
    pub event_slug: String,
    pub role: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct MyEventRow {
    pub event_slug: String,
    pub event_name: String,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    /// URL of the PR / repo / contribution counted for this event.
    pub contribution_ref: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyEventsResponse {
    pub events: Vec<MyEventRow>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListEventsQuery {
    /// Narrow to one kind of event.
    #[serde(default)]
    pub event_type: Option<String>,
}

/// Every event on. Public — no auth required.
#[utoipa::path(
    get, path = "/api/events", tag = "profile",
    params(ListEventsQuery),
    responses(
        (status = 200, description = "Events currently on", body = ApiResponse<EventsListResponse>),
    ),
)]
pub async fn list_events(
    State(state): State<AppState>,
    Query(q): Query<ListEventsQuery>,
) -> Result<Json<ApiResponse<EventsListResponse>>, AppError> {
    let sql = format!(
        "{EVENT_SELECT} WHERE status IN ('published', 'live')
            AND ($1::TEXT IS NULL OR event_type = $1)
          ORDER BY starts_at DESC"
    );
    let rows: Vec<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(q.event_type.as_deref())
        .fetch_all(&state.db)
        .await?;
    Ok(Json(ApiResponse::new(EventsListResponse { events: rows })))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventDetail {
    pub event: EventRow,
    pub participants: i64,
    /// Signed sponsors, in the order their logos are sized.
    pub sponsors: Vec<Value>,
    pub livestreams: Vec<Value>,
}

/// One event, with who is backing it and where to watch.
#[utoipa::path(
    get, path = "/api/events/{slug}", tag = "profile",
    params(("slug" = String, Path, description = "Event slug")),
    responses(
        (status = 200, body = ApiResponse<EventDetail>),
        (status = 404, description = "Event slug unknown", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn read_event(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<EventDetail>>, AppError> {
    let sql = format!("{EVENT_SELECT} WHERE slug = $1 AND status <> 'draft'");
    let event: Option<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(&slug)
        .fetch_optional(&state.db)
        .await?;
    let event = event.ok_or_else(|| AppError::NotFound(format!("event '{slug}'")))?;

    let participants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_event_participation
          WHERE event_id = $1 AND role = 'participant'",
    )
    .bind(event.id)
    .fetch_one(&state.db)
    .await?;

    // Only the signed ones, and only what a sponsor bought the right to show.
    // The fee is between them and us.
    let sponsors: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'enterprise_id', s.enterprise_id,
                    'company_name', e.company_name,
                    'logo_url', e.logo_url,
                    'tier', s.package_tier,
                    'placement', s.logo_placement,
                    'named_challenge', s.named_challenge_slug,
                    'stand_url', s.virtual_stand_url
                )
           FROM event_sponsorships s
           JOIN enterprises e ON e.id = s.enterprise_id
          WHERE s.event_id = $1 AND s.status IN ('signed', 'honoured')
          ORDER BY s.agreed_fee DESC",
    )
    .bind(event.id)
    .fetch_all(&state.db)
    .await?;

    let livestreams: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'platform', platform, 'url', url,
                    'premium', premium_content_available,
                    'replay_url', replay_url, 'starts_at', starts_at
                )
           FROM event_livestreams WHERE event_id = $1 ORDER BY starts_at",
    )
    .bind(event.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(EventDetail {
        event,
        participants,
        sponsors,
        livestreams,
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JoinBody {
    /// Defaults to `participant`. The other roles are invitations, not
    /// things somebody claims for themselves.
    #[serde(default)]
    pub role: Option<String>,
}

/// Enroll the caller. The associated stamp is emitted separately by the badge
/// engine when the event's rule fires.
#[utoipa::path(
    post, path = "/api/events/{slug}/join", tag = "profile",
    params(("slug" = String, Path, description = "Event slug")),
    request_body = JoinBody,
    responses(
        (status = 200, description = "Joined (or already joined)", body = ApiResponse<JoinEventResponse>),
        (status = 400, description = "Event closed, full, or a role nobody can claim", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Event slug unknown", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn join_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<JoinBody>,
) -> Result<Json<ApiResponse<JoinEventResponse>>, AppError> {
    // A jury seat is an appointment. Letting somebody claim one for
    // themselves would make the judging worthless.
    let role = body.role.as_deref().unwrap_or("participant");
    if role != "participant" {
        return Err(AppError::Validation(
            "only a participant seat can be taken directly — a jury, organizer or \
             speaker place is an appointment"
                .into(),
        ));
    }

    let event: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM events WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await?;
    let (event_id,) = event.ok_or_else(|| AppError::NotFound(format!("event '{slug}'")))?;

    sqlx::query(
        "INSERT INTO user_event_participation (user_id, event_id, role)
         VALUES ($1, $2, 'participant') ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(event_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        // The trigger holds the seat count and the status, and says so in its
        // own words; repeating them here would let the two drift apart.
        if m.contains("participants") || m.contains("registrations") {
            AppError::Validation(
                m.rsplit("ERROR:")
                    .next()
                    .unwrap_or("this event is not taking registrations")
                    .trim()
                    .to_string(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    Ok(Json(ApiResponse::new(JoinEventResponse {
        joined: true,
        event_slug: slug,
        role: "participant".into(),
    })))
}

/// Every event the caller has joined, in any role.
#[utoipa::path(
    get, path = "/api/users/me/events", tag = "profile",
    responses(
        (status = 200, description = "Caller's events", body = ApiResponse<MyEventsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_events(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyEventsResponse>>, AppError> {
    let rows: Vec<MyEventRow> = sqlx::query_as(
        "SELECT e.slug AS event_slug, e.name AS event_name, uep.role,
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

// ═══════════════════════════════════════════════════════════════════
// Admin
// ═══════════════════════════════════════════════════════════════════

pub fn admin_event_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/events/{id}/appoint", post(appoint))
        .route("/admin/events/{id}/status", post(set_status))
        .route("/admin/events/{id}/livestreams", post(add_livestream))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AppointBody {
    pub user_id: Uuid,
    pub role: String,
}

/// Appoint a juror, organizer, speaker or sponsor representative.
///
/// Separate from joining because these are invitations. A jury somebody can
/// join is a jury whose verdict means nothing.
#[utoipa::path(
    post, path = "/api/admin/events/{id}/appoint", tag = "admin",
    params(("id" = Uuid, Path, description = "Event id")),
    request_body = AppointBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a role, or the participant seat which is self-serve", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn appoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AppointBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    if !ROLES.contains(&body.role.as_str()) {
        return Err(AppError::Validation(format!(
            "role must be one of: {}",
            ROLES.join(", ")
        )));
    }
    if body.role == "participant" {
        return Err(AppError::Validation(
            "people enroll themselves as participants".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO user_event_participation (user_id, event_id, role)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(body.user_id)
    .bind(id)
    .bind(&body.role)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": { "appointed": true, "role": body.role }
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StatusBody {
    pub status: String,
}

#[utoipa::path(
    post, path = "/api/admin/events/{id}/status", tag = "admin",
    params(("id" = Uuid, Path, description = "Event id")),
    request_body = StatusBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a status, or an onsite event with no address", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    const STATUSES: &[&str] = &["draft", "published", "live", "finished", "cancelled"];
    if !STATUSES.contains(&body.status.as_str()) {
        return Err(AppError::Validation(format!(
            "status must be one of: {}",
            STATUSES.join(", ")
        )));
    }

    sqlx::query("UPDATE events SET status = $2 WHERE id = $1")
        .bind(id)
        .bind(&body.status)
        .execute(&state.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("onsite_events_say_where") {
                AppError::Validation(
                    "an onsite event needs an address before it is published — one \
                     nobody can find is one nobody attends"
                        .into(),
                )
            } else {
                AppError::from(e)
            }
        })?;

    Ok(Json(serde_json::json!({
        "data": { "status": body.status }
    })))
}

#[utoipa::path(
    post, path = "/api/admin/events/{id}/livestreams", tag = "admin",
    params(("id" = Uuid, Path, description = "Event id")),
    request_body(content = serde_json::Value, description = "LivestreamInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown platform or a non-https URL", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_livestream(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<sponsorship::LivestreamInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let stream_id = sponsorship::add_livestream(&state.db, id, input).await?;
    Ok(Json(serde_json::json!({
        "data": { "livestream_id": stream_id }
    })))
}
