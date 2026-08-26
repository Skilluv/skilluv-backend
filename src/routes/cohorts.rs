//! SKI-40 (Post-MVP T2-01) — cohort HTTP surface.
//!
//! Endpoints:
//!   POST   /api/cohorts                          (auth)
//!   GET    /api/cohorts                          (public — discovery)
//!   GET    /api/cohorts/{id}                     (public if the cohort is)
//!   PATCH  /api/cohorts/{id}                     (organizer)
//!   POST   /api/cohorts/{id}/join                (auth)
//!   DELETE /api/cohorts/{id}/leave               (member)
//!   POST   /api/cohorts/{id}/members             (organizer — add/promote)
//!   DELETE /api/cohorts/{id}/members/{user_id}   (organizer)
//!   GET    /api/cohorts/{id}/members             (readable by viewer)
//!   POST   /api/cohorts/{id}/milestones          (organizer)
//!   GET    /api/cohorts/{id}/milestones          (readable by viewer)
//!   DELETE /api/cohorts/{id}/milestones/{mid}    (organizer)
//!   POST   /api/cohorts/{id}/messages            (member)
//!   GET    /api/cohorts/{id}/messages            (member)
//!   GET    /api/users/me/cohorts                 (auth)
//!
//! Chat messages are persisted and then broadcast to the cohort's
//! WebSocket room. Persistence first, on purpose: a member who is offline
//! must still find the message on reconnect, so the database is the source
//! of truth and the socket is only the fast path.

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
use crate::middleware::{AuthUser, OptionalAuth};
use crate::services::cohorts;
use crate::websocket::WsMessage;

const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 50;

pub fn cohort_routes() -> Router<AppState> {
    Router::new()
        .route("/cohorts", post(create).get(list))
        .route("/cohorts/{id}", get(fetch).patch(update))
        .route("/cohorts/{id}/join", post(join))
        .route("/cohorts/{id}/leave", delete(leave))
        .route("/cohorts/{id}/members", post(add_member).get(list_members))
        .route("/cohorts/{id}/members/{user_id}", delete(remove_member))
        .route(
            "/cohorts/{id}/milestones",
            post(create_milestone).get(list_milestones),
        )
        .route(
            "/cohorts/{id}/milestones/{milestone_id}",
            delete(delete_milestone),
        )
        .route(
            "/cohorts/{id}/messages",
            post(post_message).get(list_messages),
        )
        .route("/users/me/cohorts", get(list_mine))
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

fn validate_slug(slug: &str) -> Result<(), AppError> {
    let ok = (3..=60).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(AppError::Validation(
            "slug must match [a-z0-9-] and be 3..60 characters".into(),
        ))
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateCohortBody {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    /// 2..30. Defaults to 20.
    #[serde(default)]
    pub max_members: Option<i32>,
    #[serde(default)]
    pub orientation_id: Option<Uuid>,
    /// Public cohorts are discoverable and self-serve joinable. Private
    /// ones are invite-only via `POST /cohorts/{id}/members`.
    #[serde(default = "default_true")]
    pub is_public: bool,
}

fn default_true() -> bool {
    true
}

/// Open a cohort. The caller leads it.
#[utoipa::path(
    post, path = "/api/cohorts",
    operation_id = "cohortsCreate",
    tag = "education",
    request_body = CreateCohortBody,
    responses(
        (status = 201, description = "The cohort was opened, with the caller leading it"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateCohortBody>,
) -> Result<impl IntoResponse, AppError> {
    validate_slug(&body.slug)?;

    let name = body.name.trim();
    if !(3..=120).contains(&name.chars().count()) {
        return Err(AppError::Validation(
            "name must be 3..120 characters".into(),
        ));
    }
    let description = body.description.unwrap_or_default();
    if description.chars().count() > 4000 {
        return Err(AppError::Validation(
            "description must be at most 4000 characters".into(),
        ));
    }
    if body.ends_at <= body.starts_at {
        return Err(AppError::Validation(
            "ends_at must be after starts_at".into(),
        ));
    }
    if body.ends_at <= chrono::Utc::now() {
        return Err(AppError::Validation(
            "ends_at must be in the future — a cohort is a cycle to run, not a record".into(),
        ));
    }
    let max_members = body.max_members.unwrap_or(20);
    if !(2..=30).contains(&max_members) {
        return Err(AppError::Validation(
            "max_members must be between 2 and 30".into(),
        ));
    }

    let cohort = cohorts::create(
        &state.db,
        auth.user_id,
        cohorts::CreateCohortParams {
            slug: &body.slug,
            name,
            description: &description,
            starts_at: body.starts_at,
            ends_at: body.ends_at,
            max_members,
            orientation_id: body.orientation_id,
            is_public: body.is_public,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(wrap(json!({ "cohort": cohort })))))
}

/// Join result for the discovery listing.
///
/// `#[sqlx(flatten)]` maps `c.*` onto the `Cohort` struct while the two
/// aggregate columns stay siblings — a plain tuple would not work, since
/// `query_as` treats each tuple element as one scalar column.
#[derive(Debug, sqlx::FromRow)]
struct CohortListRow {
    #[sqlx(flatten)]
    cohort: cohorts::Cohort,
    member_count: i64,
    orientation_slug: Option<String>,
}

/// Join result for "my cohorts", carrying the caller's role.
#[derive(Debug, sqlx::FromRow)]
struct MyCohortRow {
    #[sqlx(flatten)]
    cohort: cohorts::Cohort,
    role: String,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListCohortsQuery {
    /// Filter by orientation slug (not id — this is the discovery surface,
    /// and slugs are what appear in URLs).
    #[serde(default)]
    pub orientation: Option<String>,
    /// Only cohorts that have not started yet — the ones you can still
    /// join from the beginning.
    #[serde(default)]
    pub upcoming_only: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Discovery listing. Public, non-archived cohorts only — private cohorts
/// never appear here, even for their own members (they reach them through
/// `/users/me/cohorts`).
#[utoipa::path(
    get, path = "/api/cohorts",
    operation_id = "cohortsList",
    tag = "education",
    params(ListCohortsQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListCohortsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let rows: Vec<CohortListRow> = sqlx::query_as(
        r#"
        SELECT c.*,
               (SELECT COUNT(*) FROM cohort_members m WHERE m.cohort_id = c.id)
                   AS member_count,
               o.slug AS orientation_slug
          FROM cohorts c
          LEFT JOIN orientations o ON o.id = c.orientation_id
         WHERE c.is_public = TRUE
           AND c.archived_at IS NULL
           AND ($1::TEXT IS NULL OR o.slug = $1)
           AND (NOT $2::BOOLEAN OR c.starts_at > NOW())
         ORDER BY c.starts_at ASC
         LIMIT $3 OFFSET $4
        "#,
    )
    .bind(q.orientation.as_deref())
    .bind(q.upcoming_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let cohorts_json: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let seats_left = (r.cohort.max_members as i64 - r.member_count).max(0);
            json!({
                "cohort": r.cohort,
                "orientation_slug": r.orientation_slug,
                "member_count": r.member_count,
                "seats_left": seats_left,
            })
        })
        .collect();

    Ok(Json(wrap(json!({
        "cohorts": cohorts_json,
        "limit": limit,
        "offset": offset,
    }))))
}

/// One cohort. Private cohorts answer only to their own members.
#[utoipa::path(
    get, path = "/api/cohorts/{id}",
    operation_id = "cohortsFetch",
    tag = "education",
    params(("id" = uuid::Uuid, Path, description = "Cohort id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such cohort, or a private one you are not in", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn fetch(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let viewer = auth.map(|a| a.user_id);
    let cohort = cohorts::assert_readable(&state.db, id, viewer).await?;

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cohort_members WHERE cohort_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    let orientation_slug: Option<String> = match cohort.orientation_id {
        Some(oid) => {
            sqlx::query_scalar("SELECT slug FROM orientations WHERE id = $1")
                .bind(oid)
                .fetch_optional(&state.db)
                .await?
        }
        None => None,
    };
    let my_role = match viewer {
        Some(v) => cohorts::role_of(&state.db, id, v).await?,
        None => None,
    };

    Ok(Json(wrap(json!({
        "cohort": cohort,
        "orientation_slug": orientation_slug,
        "member_count": member_count,
        "seats_left": (cohort.max_members as i64 - member_count).max(0),
        "my_role": my_role,
    }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateCohortBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub max_members: Option<i32>,
    #[serde(default)]
    pub is_public: Option<bool>,
    /// Freeze the cohort. One-way: un-archiving would resurrect a cycle
    /// that members have already been told is over.
    #[serde(default)]
    pub archive: Option<bool>,
}

/// Change a cohort. Whoever leads it.
#[utoipa::path(
    patch, path = "/api/cohorts/{id}",
    operation_id = "cohortsUpdate",
    tag = "education",
    params(("id" = uuid::Uuid, Path)),
    request_body = UpdateCohortBody,
    responses(
        (status = 200, description = "Updated"),
        (status = 403, description = "Only the people leading a cohort change it", body = crate::api_response::ErrorResponse),
        (status = 409, description = "The change conflicts with where the cohort already is", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCohortBody>,
) -> Result<impl IntoResponse, AppError> {
    let cohort = cohorts::assert_organizer(&state.db, id, auth.user_id).await?;

    if let Some(n) = body.name.as_deref()
        && !(3..=120).contains(&n.trim().chars().count())
    {
        return Err(AppError::Validation(
            "name must be 3..120 characters".into(),
        ));
    }
    if let Some(d) = body.description.as_deref()
        && d.chars().count() > 4000
    {
        return Err(AppError::Validation(
            "description must be at most 4000 characters".into(),
        ));
    }
    if let Some(e) = body.ends_at
        && e <= cohort.starts_at
    {
        return Err(AppError::Validation(
            "ends_at must be after starts_at".into(),
        ));
    }
    if let Some(m) = body.max_members {
        if !(2..=30).contains(&m) {
            return Err(AppError::Validation(
                "max_members must be between 2 and 30".into(),
            ));
        }
        // Lowering the cap below the current headcount would leave the
        // cohort permanently "over capacity" with no way to reconcile.
        let current: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cohort_members WHERE cohort_id = $1")
                .bind(id)
                .fetch_one(&state.db)
                .await?;
        if (m as i64) < current {
            return Err(AppError::Conflict(format!(
                "cohort already has {current} members — remove some before lowering the cap"
            )));
        }
    }

    let updated: cohorts::Cohort = sqlx::query_as(
        r#"
        UPDATE cohorts SET
            name        = COALESCE($2, name),
            description = COALESCE($3, description),
            ends_at     = COALESCE($4, ends_at),
            max_members = COALESCE($5, max_members),
            is_public   = COALESCE($6, is_public),
            archived_at = CASE WHEN $7::BOOLEAN IS TRUE THEN COALESCE(archived_at, NOW())
                               ELSE archived_at END,
            updated_at  = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.description.as_deref())
    .bind(body.ends_at)
    .bind(body.max_members)
    .bind(body.is_public)
    .bind(body.archive)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(wrap(json!({ "cohort": updated }))))
}

/// Join a public cohort.
#[utoipa::path(
    post, path = "/api/cohorts/{id}/join", tag = "education",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 201, description = "Joined"),
        (status = 403, description = "Private cohorts are joined by invitation", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such cohort", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn join(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let member = cohorts::join(&state.db, id, auth.user_id).await?;
    Ok((StatusCode::CREATED, Json(wrap(json!({ "member": member })))))
}

/// Leave a cohort. Recorded as a departure, not as an absence — a cohort
/// that counts only its survivors cannot tell you it is failing.
#[utoipa::path(
    delete, path = "/api/cohorts/{id}/leave", tag = "education",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 204, description = "Left"),
        (status = 404, description = "You are not in that cohort", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn leave(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::leave(&state.db, id, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CohortAddMemberBody)]
pub struct AddMemberBody {
    pub user_id: Uuid,
    /// `member` (default) or `organizer`. Re-posting with a different role
    /// promotes or demotes an existing member.
    #[serde(default)]
    pub role: Option<String>,
}

/// Add somebody to a cohort.
#[utoipa::path(
    post, path = "/api/cohorts/{id}/members", tag = "education",
    params(("id" = uuid::Uuid, Path)),
    request_body = AddMemberBody,
    responses(
        (status = 201, description = "Added"),
        (status = 403, description = "Only the people leading a cohort add members", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberBody>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::assert_organizer(&state.db, id, auth.user_id).await?;
    let role = body.role.as_deref().unwrap_or(cohorts::ROLE_MEMBER);
    let member = cohorts::add_member(&state.db, id, body.user_id, role).await?;
    Ok((StatusCode::CREATED, Json(wrap(json!({ "member": member })))))
}

/// Remove somebody from a cohort.
#[utoipa::path(
    delete, path = "/api/cohorts/{id}/members/{user_id}", tag = "education",
    params(("id" = uuid::Uuid, Path, description = "Cohort id"), ("user_id" = uuid::Uuid, Path, description = "Who to remove")),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, description = "Only the people leading a cohort remove members", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such cohort, or no such member", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::assert_organizer(&state.db, id, auth.user_id).await?;
    // Routed through `leave` so the last-organizer rule applies to an
    // organizer removing themselves through this endpoint too.
    cohorts::leave(&state.db, id, target).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Who is in a cohort. Follows the same visibility rule as the cohort.
#[utoipa::path(
    get, path = "/api/cohorts/{id}/members", tag = "education",
    params(("id" = uuid::Uuid, Path, description = "Cohort id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such cohort, or a private one you are not in", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list_members(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::assert_readable(&state.db, id, auth.map(|a| a.user_id)).await?;

    let rows: Vec<(Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT m.user_id,
               COALESCE(NULLIF(u.display_name, ''), u.username),
               m.role,
               m.joined_at
          FROM cohort_members m
          JOIN users u ON u.id = m.user_id
         WHERE m.cohort_id = $1
         ORDER BY m.role DESC, m.joined_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let members: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(user_id, display_name, role, joined_at)| {
            json!({
                "user_id": user_id,
                "display_name": display_name,
                "role": role,
                "joined_at": joined_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(wrap(json!({ "members": members }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMilestoneBody {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub target_date: chrono::NaiveDate,
}

/// Add a milestone to a cohort's plan.
#[utoipa::path(
    post, path = "/api/cohorts/{id}/milestones", tag = "education",
    params(("id" = uuid::Uuid, Path)),
    request_body = CreateMilestoneBody,
    responses(
        (status = 201, description = "Added to the plan"),
        (status = 403, description = "Only the people leading a cohort set milestones", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateMilestoneBody>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::assert_organizer(&state.db, id, auth.user_id).await?;

    let title = body.title.trim();
    if !(3..=200).contains(&title.chars().count()) {
        return Err(AppError::Validation(
            "title must be 3..200 characters".into(),
        ));
    }
    let description = body.description.unwrap_or_default();
    if description.chars().count() > 4000 {
        return Err(AppError::Validation(
            "description must be at most 4000 characters".into(),
        ));
    }

    let milestone: cohorts::CohortMilestone = sqlx::query_as(
        "INSERT INTO cohort_milestones (cohort_id, title, description, target_date)
         VALUES ($1, $2, $3, $4)
         RETURNING *",
    )
    .bind(id)
    .bind(title)
    .bind(&description)
    .bind(body.target_date)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "milestone": milestone }))),
    ))
}

/// A cohort's plan. Public where the cohort is.
#[utoipa::path(
    get, path = "/api/cohorts/{id}/milestones", tag = "education",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such cohort", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list_milestones(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::assert_readable(&state.db, id, auth.map(|a| a.user_id)).await?;

    let milestones: Vec<cohorts::CohortMilestone> = sqlx::query_as(
        "SELECT * FROM cohort_milestones WHERE cohort_id = $1 ORDER BY target_date ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(wrap(json!({ "milestones": milestones }))))
}

/// Remove a milestone from a cohort's plan.
#[utoipa::path(
    delete, path = "/api/cohorts/{id}/milestones/{milestone_id}", tag = "education",
    params(("id" = uuid::Uuid, Path, description = "Cohort id"), ("milestone_id" = uuid::Uuid, Path, description = "Which milestone")),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "No such milestone in that cohort", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn delete_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, milestone_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    cohorts::assert_organizer(&state.db, id, auth.user_id).await?;

    let affected = sqlx::query("DELETE FROM cohort_milestones WHERE id = $1 AND cohort_id = $2")
        .bind(milestone_id)
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "milestone {milestone_id} not found"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PostMessageBody {
    pub body: String,
}

/// Post to a cohort's thread. Members only.
#[utoipa::path(
    post, path = "/api/cohorts/{id}/messages", tag = "education",
    params(("id" = uuid::Uuid, Path, description = "Cohort id")),
    request_body = PostMessageBody,
    responses(
        (status = 201, description = "Posted"),
        (status = 403, description = "Only members write to a cohort", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such cohort", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn post_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PostMessageBody>,
) -> Result<impl IntoResponse, AppError> {
    let message = cohorts::post_message(&state.db, id, auth.user_id, &body.body).await?;

    // Persisted first, broadcast second: offline members must find the
    // message on reconnect, so the socket is a fast path and never the
    // record.
    state
        .ws
        .broadcast_to_room(
            &cohorts::room_key(id),
            WsMessage {
                event: "cohort_message".to_string(),
                room: Some(cohorts::room_key(id)),
                payload: json!({
                    "cohort_id": id,
                    "message_id": message.id,
                    "sender_id": message.sender_id,
                    "body": message.body,
                    "created_at": message.created_at.to_rfc3339(),
                }),
            },
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "message": message }))),
    ))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListMessagesQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    /// Cursor: return messages strictly older than this instant.
    #[serde(default)]
    pub before: Option<chrono::DateTime<chrono::Utc>>,
}

/// A cohort's thread. Members only.
#[utoipa::path(
    get, path = "/api/cohorts/{id}/messages",
    operation_id = "cohortsListMessages",
    tag = "education",
    params(("id" = uuid::Uuid, Path), ListMessagesQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Members only", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such cohort", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let messages = cohorts::list_messages(&state.db, id, auth.user_id, limit, q.before).await?;
    Ok(Json(wrap(json!({ "messages": messages, "limit": limit }))))
}

/// The caller's cohorts, including private ones. This is how a member
/// reaches a private cohort — it never appears in discovery.
#[utoipa::path(
    get, path = "/api/users/me/cohorts",
    operation_id = "cohortsListMine",
    tag = "education",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<MyCohortRow> = sqlx::query_as(
        r#"
        SELECT c.*, m.role
          FROM cohorts c
          JOIN cohort_members m ON m.cohort_id = c.id
         WHERE m.user_id = $1
         ORDER BY c.archived_at NULLS FIRST, c.starts_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| json!({ "cohort": r.cohort, "my_role": r.role }))
        .collect();

    Ok(Json(wrap(json!({ "cohorts": items }))))
}
