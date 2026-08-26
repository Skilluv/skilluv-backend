//! SKI-41 (Post-MVP T2-02) — peer coaching HTTP surface.
//!
//! Endpoints:
//!   POST   /api/users/me/peer-matching/enroll              (auth)
//!   DELETE /api/users/me/peer-matching/enroll/{oid}        (auth)
//!   GET    /api/users/me/peer-matching/enrollments         (auth)
//!   GET    /api/peer-matching/proposals?orientation_id=    (auth)
//!   POST   /api/peer-matching/matches                      (auth)
//!   GET    /api/users/me/peer-matches                      (auth)
//!   DELETE /api/peer-matches/{id}                          (participant)
//!   POST   /api/peer-matches/{id}/sessions                 (participant)
//!   GET    /api/peer-matches/{id}/sessions                 (participant)
//!   PATCH  /api/peer-sessions/{id}                         (participant)
//!   DELETE /api/peer-sessions/{id}                         (participant)
//!
//! Everything is scoped to the caller: a match is only ever addressable by
//! one of its two participants, so there is no ownership parameter to get
//! wrong.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::peer_matching;

pub fn peer_matching_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/peer-matching/enroll", post(enroll))
        .route(
            "/users/me/peer-matching/enroll/{orientation_id}",
            delete(unenroll),
        )
        .route("/users/me/peer-matching/enrollments", get(list_enrollments))
        .route("/peer-matching/proposals", get(proposals))
        .route("/peer-matching/matches", post(create_match))
        .route("/users/me/peer-matches", get(list_matches))
        .route("/peer-matches/{id}", delete(end_match))
        .route(
            "/peer-matches/{id}/sessions",
            post(schedule_session).get(list_sessions),
        )
        .route(
            "/peer-sessions/{id}",
            axum::routing::patch(check_in).delete(cancel_session),
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

/// Join result for the enrollment listing.
///
/// `#[sqlx(flatten)]` rather than a tuple: `query_as` reads each tuple
/// element as one scalar column, so a struct member has to be flattened
/// explicitly.
#[derive(Debug, sqlx::FromRow)]
struct EnrollmentRow {
    #[sqlx(flatten)]
    enrollment: peer_matching::Enrollment,
    orientation_slug: String,
    orientation_name: String,
}

/// Join result for "my matches", carrying the resolved peer identity.
#[derive(Debug, sqlx::FromRow)]
struct MatchRow {
    #[sqlx(flatten)]
    peer_match: peer_matching::PeerMatch,
    peer_id: Uuid,
    peer_name: String,
    orientation_slug: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PeerMatchingEnrollBody)]
pub struct EnrollBody {
    pub orientation_id: Uuid,
    /// Sessions per week, 1..5. Defaults to 1.
    #[serde(default)]
    pub weekly_cadence: Option<i16>,
}

/// Open yourself to being matched with a peer on one orientation.
#[utoipa::path(
    post, path = "/api/users/me/peer-matching/enroll", tag = "social",
    request_body = EnrollBody,
    responses(
        (status = 201, description = "Open to being matched on that orientation"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn enroll(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<EnrollBody>,
) -> Result<impl IntoResponse, AppError> {
    let enrollment = peer_matching::enroll(
        &state.db,
        auth.user_id,
        body.orientation_id,
        body.weekly_cadence.unwrap_or(1),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "enrollment": enrollment }))),
    ))
}

/// Stop being offered matches on one orientation.
#[utoipa::path(
    delete, path = "/api/users/me/peer-matching/enroll/{orientation_id}", tag = "social",
    params(("orientation_id" = uuid::Uuid, Path, description = "Which orientation to stop on")),
    responses(
        (status = 204, description = "No longer open to matches on that orientation"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn unenroll(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(orientation_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    peer_matching::unenroll(&state.db, auth.user_id, orientation_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The orientations the caller opened themselves to being matched on.
#[utoipa::path(
    get, path = "/api/users/me/peer-matching/enrollments", tag = "social",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_enrollments(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<EnrollmentRow> = sqlx::query_as(
        r#"
        SELECT e.*,
               o.slug AS orientation_slug,
               o.name AS orientation_name
          FROM peer_matching_enrollments e
          JOIN orientations o ON o.id = e.orientation_id
         WHERE e.user_id = $1
         ORDER BY e.active DESC, e.enrolled_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let enrollments: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "enrollment": r.enrollment,
                "orientation_slug": r.orientation_slug,
                "orientation_name": r.orientation_name,
            })
        })
        .collect();

    Ok(Json(wrap(json!({ "enrollments": enrollments }))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ProposalsQuery {
    pub orientation_id: Uuid,
}

/// Candidate peers for one orientation, ranked. Proposals only — pairing
/// is a separate, deliberate act.
#[utoipa::path(
    get, path = "/api/peer-matching/proposals", tag = "social",
    params(ProposalsQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn proposals(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ProposalsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let proposals = peer_matching::propose(&state.db, auth.user_id, q.orientation_id).await?;
    Ok(Json(wrap(json!({
        "proposals": proposals,
        "orientation_id": q.orientation_id,
    }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMatchBody {
    pub peer_id: Uuid,
    pub orientation_id: Uuid,
}

/// Pair with a peer. A deliberate act on a proposal, never automatic.
#[utoipa::path(
    post, path = "/api/peer-matching/matches", tag = "social",
    request_body = CreateMatchBody,
    responses(
        (status = 201, description = "Paired"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_match(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateMatchBody>,
) -> Result<impl IntoResponse, AppError> {
    let m = peer_matching::create_match(&state.db, auth.user_id, body.peer_id, body.orientation_id)
        .await?;
    Ok((StatusCode::CREATED, Json(wrap(json!({ "match": m })))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListMatchesQuery {
    /// Include matches that have ended. Off by default.
    #[serde(default)]
    pub include_ended: bool,
}

/// The caller's peer matches, with the other side resolved.
#[utoipa::path(
    get, path = "/api/users/me/peer-matches", tag = "social",
    params(ListMatchesQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_matches(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListMatchesQuery>,
) -> Result<impl IntoResponse, AppError> {
    // The peer is whichever side of the ordered pair is not the caller.
    let rows: Vec<MatchRow> = sqlx::query_as(
        r#"
        SELECT m.*,
               peer.id AS peer_id,
               COALESCE(NULLIF(peer.display_name, ''), peer.username) AS peer_name,
               o.slug AS orientation_slug
          FROM peer_matches m
          JOIN users peer
            ON peer.id = CASE WHEN m.user_a = $1 THEN m.user_b ELSE m.user_a END
          JOIN orientations o ON o.id = m.orientation_id
         WHERE (m.user_a = $1 OR m.user_b = $1)
           AND ($2::BOOLEAN OR m.active = TRUE)
         ORDER BY m.active DESC, m.matched_at DESC
        "#,
    )
    .bind(auth.user_id)
    .bind(q.include_ended)
    .fetch_all(&state.db)
    .await?;

    let matches: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "match": r.peer_match,
                "peer": { "user_id": r.peer_id, "display_name": r.peer_name },
                "orientation_slug": r.orientation_slug,
            })
        })
        .collect();

    Ok(Json(wrap(json!({ "matches": matches }))))
}

/// End a pairing.
#[utoipa::path(
    delete, path = "/api/peer-matches/{id}", tag = "social",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 204, description = "Ended"),
        (status = 403, description = "Only the two sides of a match end it", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn end_match(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    peer_matching::end_match(&state.db, id, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduleSessionBody {
    pub session_at: chrono::DateTime<chrono::Utc>,
}

/// Schedule a session between the two sides of a match.
#[utoipa::path(
    post, path = "/api/peer-matches/{id}/sessions", tag = "social",
    params(("id" = uuid::Uuid, Path, description = "The match")),
    request_body = ScheduleSessionBody,
    responses(
        (status = 201, description = "Scheduled"),
        (status = 403, description = "Only the two sides of a match schedule its sessions", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such match", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn schedule_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ScheduleSessionBody>,
) -> Result<impl IntoResponse, AppError> {
    let session =
        peer_matching::schedule_session(&state.db, id, auth.user_id, body.session_at).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "session": session }))),
    ))
}

/// The sessions on one match.
#[utoipa::path(
    get, path = "/api/peer-matches/{id}/sessions",
    operation_id = "peerMatchingListSessions",
    tag = "social",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Only the two sides of a match read its sessions", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Membership check first: a non-participant must not learn how many
    // sessions a match has, only that they cannot see it.
    peer_matching::get_match_for(&state.db, id, auth.user_id).await?;

    let sessions: Vec<peer_matching::PeerSession> =
        sqlx::query_as("SELECT * FROM peer_sessions WHERE match_id = $1 ORDER BY session_at DESC")
            .bind(id)
            .fetch_all(&state.db)
            .await?;

    Ok(Json(wrap(json!({ "sessions": sessions }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PeerSessionCheckInBody)]
pub struct CheckInBody {
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub rating: Option<i16>,
}

/// Record that a session happened, and what came of it.
#[utoipa::path(
    patch, path = "/api/peer-sessions/{id}",
    operation_id = "peerMatchingCheckIn",
    tag = "social",
    params(("id" = uuid::Uuid, Path)),
    request_body = CheckInBody,
    responses(
        (status = 200, description = "Recorded"),
        (status = 403, description = "Only the two sides of a session check in", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn check_in(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CheckInBody>,
) -> Result<impl IntoResponse, AppError> {
    let session = peer_matching::check_in(
        &state.db,
        id,
        auth.user_id,
        body.notes.as_deref(),
        body.rating,
    )
    .await?;
    Ok(Json(wrap(json!({ "session": session }))))
}

/// Cancel a scheduled session.
#[utoipa::path(
    delete, path = "/api/peer-sessions/{id}",
    operation_id = "peerMatchingCancelSession",
    tag = "social",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, description = "Cancelled"),
        (status = 403, description = "Only the two sides of a session cancel it", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn cancel_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let session = peer_matching::cancel_session(&state.db, id, auth.user_id).await?;
    Ok(Json(wrap(json!({ "session": session }))))
}
