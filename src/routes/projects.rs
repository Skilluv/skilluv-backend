//! Projects routes — Phase 2 Sprint 5.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::projects;

pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/projects", post(create_project))
        .route("/projects/looking-for-contributors", get(list_looking))
        .route("/projects/curated", get(list_curated))
        .route("/projects/{slug}", get(by_slug))
        .route(
            "/projects/{slug}/contributors",
            get(list_contributors).post(add_contributor),
        )
        .route(
            "/projects/{slug}/contributors/{user_id}",
            delete(remove_contributor),
        )
        .route("/projects/{slug}/archive", post(archive))
        // P26 v2 SKI-122 — public: how many Skilluvers are active on this
        // repo in the last N days (default 30).
        .route("/projects/{slug}/active-skilluvers", get(active_skilluvers))
        .route("/u/{username}/projects", get(by_user))
        .route("/guilds/{slug}/projects", get(by_guild_slug))
        .route("/admin/projects/{slug}/curated", post(admin_set_curated))
        // P12.1 — recommandations projets pour le user courant
        .route(
            "/users/me/recommendations/projects",
            get(my_project_recommendations),
        )
        // P12.2 — marque d'intérêt (onboarding + feed)
        .route(
            "/users/me/interests/projects",
            get(list_my_project_interests).post(mark_projects_interested),
        )
        .route(
            "/users/me/interests/projects/{project_id}",
            delete(unmark_project_interested),
        )
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// Create a new project. Owner is either the caller (user) or a guild
/// they administer. Request body typed by services::projects.
#[utoipa::path(
    post,
    path = "/api/projects",
    tag = "projects",
    request_body(content = serde_json::Value, description = "CreateProjectInput"),
    responses(
        (status = 200, description = "Project created", body = serde_json::Value),
        (status = 401, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "projectsCreateProject",
)]
pub async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<projects::CreateProjectInput>,
) -> Result<Json<Value>, AppError> {
    let project = projects::create(&state.db, auth.user_id, &auth.role, body).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::PROJECT_CREATED,
            props(&[
                ("owner_type", json!(project.owner_type)),
                ("is_oss", json!(project.is_oss)),
            ]),
        );
    }
    metrics::counter!(
        "skilluv_projects_created_total",
        "owner_type" => project.owner_type.clone()
    )
    .increment(1);
    Ok(Json(build_response(json!({ "project": project }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct LimitQuery {
    #[param(minimum = 1, maximum = 200)]
    limit: Option<i64>,
}

/// List projects looking for contributors. Public.
#[utoipa::path(
    get,
    path = "/api/projects/looking-for-contributors",
    tag = "projects",
    params(LimitQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_looking(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_range_opt(q.limit, "limit", 1, 200)?;
    let rows = projects::list_looking_for_contributors(&state.db, q.limit.unwrap_or(50)).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

/// List curated projects (admin-picked showcase). Public.
#[utoipa::path(
    get,
    path = "/api/projects/curated",
    tag = "projects",
    params(LimitQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_curated(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_range_opt(q.limit, "limit", 1, 200)?;
    let rows = projects::list_curated(&state.db, q.limit.unwrap_or(50)).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

/// Get a project by slug. Public.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}",
    tag = "projects",
    params(("slug" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let p = projects::by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "project": p }))))
}

/// Payload of `GET /api/u/{username}/projects`.
///
/// SKI-291 — the route is not paginated: a user owns a handful of projects,
/// and the profile page renders all of them at once.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserProjectsData {
    pub projects: Vec<projects::Project>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserProjectsResponse {
    pub data: UserProjectsData,
    pub meta: crate::api_response::MetaInfo,
}

/// List projects owned by a specific user (by username). Public.
///
/// A user who exists and owns nothing answers 200 with an empty list. 404 is
/// reserved for a username that really has no account — otherwise the front
/// cannot tell "no projects" from "no such user".
#[utoipa::path(
    get,
    path = "/api/u/{username}/projects",
    tag = "projects",
    params(("username" = String, Path)),
    responses(
        (status = 200, description = "Projects owned by this user, possibly empty", body = UserProjectsResponse),
        (status = 404, description = "No such username, or the profile is hidden", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn by_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    // SKI-291 — visibility is `profile_hidden`, matching `GET
    // /api/profile/{username}`. This route used to gate on `profile_active`,
    // which only records whether onboarding was cleared: every account that
    // had signed up without completing a challenge answered 200 on the
    // profile and 404 here, for the same username. Same bug as SKI-70, fixed
    // on one route only.
    let user: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE username = $1 AND profile_hidden = FALSE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;
    let (uid,) = user.ok_or(AppError::NotFound("user not found".into()))?;
    let rows = projects::list_for_owner(&state.db, "user", uid).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

/// List projects owned by a specific guild (by slug). Public.
#[utoipa::path(
    get,
    path = "/api/guilds/{slug}/projects",
    tag = "guilds",
    params(("slug" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn by_guild_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let guild: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM guilds WHERE slug = $1 AND disbanded_at IS NULL")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let (gid,) = guild.ok_or(AppError::NotFound("guild not found".into()))?;
    let rows = projects::list_for_owner(&state.db, "guild", gid).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

/// List contributors on a project.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/contributors",
    tag = "projects",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_contributors(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    let rows = projects::list_contributors(&state.db, project.id).await?;
    Ok(Json(build_response(json!({ "contributors": rows }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct AddContributorBody {
    user_id: Uuid,
    role: Option<String>,
}

/// Add a contributor to a project (owner or admin).
#[utoipa::path(
    post,
    path = "/api/projects/{slug}/contributors",
    tag = "projects",
    params(("slug" = String, Path)),
    request_body = AddContributorBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_contributor(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<AddContributorBody>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::add_contributor(
        &state.db,
        project.id,
        auth.user_id,
        &auth.role,
        body.user_id,
        body.role.as_deref().unwrap_or("contributor"),
    )
    .await?;
    Ok(Json(build_response(json!({ "added": true }))))
}

/// Remove a contributor from a project.
#[utoipa::path(
    delete,
    path = "/api/projects/{slug}/contributors/{user_id}",
    tag = "projects",
    params(
        ("slug" = String, Path),
        ("user_id" = Uuid, Path),
    ),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove_contributor(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((slug, user_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::remove_contributor(&state.db, project.id, auth.user_id, &auth.role, user_id).await?;
    Ok(Json(build_response(json!({ "removed": true }))))
}

/// Archive a project (soft-delete). Owner or admin only.
#[utoipa::path(
    post,
    path = "/api/projects/{slug}/archive",
    tag = "projects",
    params(("slug" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn archive(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::archive(&state.db, project.id, auth.user_id, &auth.role).await?;
    Ok(Json(build_response(json!({ "archived": true }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct SetCuratedBody {
    curated: bool,
}

/// Admin only: mark a project as curated (or un-curated).
#[utoipa::path(
    post,
    path = "/api/admin/projects/{slug}/curated",
    tag = "admin",
    params(("slug" = String, Path)),
    request_body = SetCuratedBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_set_curated(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<SetCuratedBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::admin_set_curated(&state.db, project.id, body.curated).await?;
    Ok(Json(build_response(json!({ "curated": body.curated }))))
}

// ═══════════════════════════════════════════════════════════════════
// P12.1 — Recommandations projets
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RecoQuery {
    pub limit: Option<i64>,
}

/// GET /api/users/me/recommendations/projects?limit=10
///
/// Retourne les projets qui matchent les skills prouvés du user, exclut ceux
/// où il a déjà un deliverable verified, triés par match_score DESC.
/// Personalised project recommendations for the caller.
#[utoipa::path(
    get,
    path = "/api/users/me/recommendations/projects",
    tag = "projects",
    params(RecoQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_project_recommendations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RecoQuery>,
) -> Result<Json<Value>, AppError> {
    let recos =
        projects::recommend_for_user(&state.db, auth.user_id, q.limit.unwrap_or(10)).await?;
    Ok(Json(build_response(json!({
        "recommendations": recos,
        "count": recos.len(),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// P12.2 — Marque d'intérêt user → project
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MarkInterestedBody {
    /// Batch d'IDs projets (onboarding : le user coche les projets qui l'intéressent).
    pub project_ids: Vec<Uuid>,
}

/// POST /api/users/me/interests/projects
///
/// Marque plusieurs projets comme intéressants. Score par défaut 50.
/// Batch-mark projects as interesting (onboarding step).
#[utoipa::path(
    post,
    path = "/api/users/me/interests/projects",
    tag = "projects",
    request_body = MarkInterestedBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mark_projects_interested(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<MarkInterestedBody>,
) -> Result<Json<Value>, AppError> {
    if body.project_ids.is_empty() {
        return Err(AppError::Validation("project_ids must not be empty".into()));
    }
    if body.project_ids.len() > 50 {
        return Err(AppError::Validation(
            "cannot mark more than 50 projects at once".into(),
        ));
    }
    let count = projects::mark_interested_batch(&state.db, auth.user_id, &body.project_ids).await?;
    metrics::counter!("skilluv_project_interests_marked_total").increment(count as u64);
    Ok(Json(build_response(json!({ "marked": count }))))
}

/// GET /api/users/me/interests/projects
///
/// Liste mes projets d'intérêt (score > 0), triés par score DESC.
/// List the caller's project interests.
#[utoipa::path(
    get,
    path = "/api/users/me/interests/projects",
    tag = "projects",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_my_project_interests(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = projects::list_interests(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "interests": rows,
    }))))
}

/// DELETE /api/users/me/interests/projects/{project_id}
///
/// Retire un projet de mes intérêts (score → 0).
/// Remove a project from the caller's interest list.
#[utoipa::path(
    delete,
    path = "/api/users/me/interests/projects/{project_id}",
    tag = "projects",
    params(("project_id" = Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn unmark_project_interested(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let affected = projects::unmark_interested(&state.db, auth.user_id, project_id).await?;
    Ok(Json(build_response(json!({ "removed": affected > 0 }))))
}

// ═══════════════════════════════════════════════════════════════════
// P26 v2 SKI-122 — active Skilluvers on a project
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ActiveWindowQuery {
    /// Rolling window in days (default 30, capped 180 to keep query cheap).
    #[serde(default)]
    days: Option<i32>,
}

/// GET /api/projects/{slug}/active-skilluvers?days=30
///
/// Public — anyone can see the "community pulse" on a repo. Returns:
///   { count: N, users: [{username, avatar, last_activity}] }
///
/// A user is "active" if in the window they either claimed a slice on
/// this project, submitted a PR, or had one validated. Distinct users;
/// the payload is capped at 20 users to keep the response bounded.
#[utoipa::path(
    get, path = "/api/projects/{slug}/active-skilluvers", tag = "projects",
    params(("slug" = String, Path, description = "Project slug")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such project", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn active_skilluvers(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    axum::extract::Query(q): axum::extract::Query<ActiveWindowQuery>,
) -> Result<Json<Value>, AppError> {
    let days = q.days.unwrap_or(30).clamp(1, 180);

    // Total distinct actives count first (uncapped), then the top-20
    // detail list. Two queries so the header count is accurate.
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT s.claimed_by_user_id)::bigint
          FROM project_slices s
          JOIN projects p ON p.id = s.project_id
         WHERE p.slug = $1
           AND s.claimed_by_user_id IS NOT NULL
           AND GREATEST(
                 COALESCE(s.claimed_at,      TIMESTAMP 'epoch'),
                 COALESCE(s.submitted_at,    TIMESTAMP 'epoch'),
                 COALESCE(s.validated_at,    TIMESTAMP 'epoch')
               ) > NOW() - ($2 || ' days')::interval
        "#,
    )
    .bind(&slug)
    .bind(days.to_string())
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let rows: Vec<(String, Option<String>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT u.username, u.avatar_url,
               MAX(GREATEST(
                 COALESCE(s.claimed_at,      TIMESTAMP 'epoch'),
                 COALESCE(s.submitted_at,    TIMESTAMP 'epoch'),
                 COALESCE(s.validated_at,    TIMESTAMP 'epoch')
               )) AS last_activity
          FROM project_slices s
          JOIN projects p ON p.id = s.project_id
          JOIN users u    ON u.id = s.claimed_by_user_id
         WHERE p.slug = $1
           AND s.claimed_by_user_id IS NOT NULL
           AND GREATEST(
                 COALESCE(s.claimed_at,      TIMESTAMP 'epoch'),
                 COALESCE(s.submitted_at,    TIMESTAMP 'epoch'),
                 COALESCE(s.validated_at,    TIMESTAMP 'epoch')
               ) > NOW() - ($2 || ' days')::interval
         GROUP BY u.username, u.avatar_url
         ORDER BY last_activity DESC
         LIMIT 20
        "#,
    )
    .bind(&slug)
    .bind(days.to_string())
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let users: Vec<Value> = rows
        .into_iter()
        .map(|(username, avatar, last)| {
            json!({
                "username": username,
                "avatar_url": avatar,
                "last_activity": last.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "count": count,
        "users": users,
        "window_days": days,
    }))))
}
