//! Routes HTTP pour seasons + stewards (Phase P6).
//!
//! Endpoints seasons :
//!   GET  /api/seasons                          — liste (public)
//!   GET  /api/seasons/current                  — saison active (public)
//!   GET  /api/seasons/{slug}                   — détail (public)
//!   POST /api/seasons                          — création (admin)
//!   POST /api/seasons/{slug}/activate          — activate (admin)
//!
//! Endpoints stewards :
//!   GET    /api/projects/{project_id}/stewards           — liste (public)
//!   POST   /api/projects/{project_id}/stewards           — add (project owner ou admin)
//!   DELETE /api/projects/{project_id}/stewards/{user_id}/{role} — remove
//!   GET    /api/users/me/stewardships                    — mes projets

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::{ApiResponse, SimpleMessage};
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::admin::require_admin;
use crate::services::seasons::Season;
use crate::services::stewards::ProjectSteward;
use crate::services::{CreateSeasonParams, SeasonsService, StewardsService};

pub fn season_routes() -> Router<AppState> {
    Router::new()
        .route("/seasons", get(list_seasons).post(create_season))
        .route("/seasons/current", get(current_season))
        .route("/seasons/{slug}", get(get_season))
        .route("/seasons/{slug}/activate", post(activate_season))
        .route(
            "/projects/{project_id}/stewards",
            get(list_project_stewards).post(add_steward),
        )
        .route(
            "/projects/{project_id}/stewards/{user_id}/{role}",
            delete(remove_steward),
        )
        .route("/users/me/stewardships", get(my_stewardships))
}


// ═══════════════════════════════════════════════════════════════════
// Response wrappers
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, ToSchema)]
pub struct SeasonsListResponse {
    pub seasons: Vec<Season>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SeasonResponse {
    pub season: Season,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentSeasonResponse {
    /// `None` when no season is currently active.
    pub season: Option<Season>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StewardsListResponse {
    pub stewards: Vec<ProjectSteward>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StewardResponse {
    pub steward: ProjectSteward,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StewardshipsResponse {
    pub stewardships: Vec<ProjectSteward>,
}

// ═══════════════════════════════════════════════════════════════════
// Seasons
// ═══════════════════════════════════════════════════════════════════

/// List every season (past + future + current). Public.
#[utoipa::path(
    get,
    path = "/api/seasons",
    tag = "challenges",
    responses((status = 200, description = "All seasons", body = ApiResponse<SeasonsListResponse>)),
)]
pub async fn list_seasons(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SeasonsListResponse>>, AppError> {
    let seasons = SeasonsService::list_all(&state.db).await?;
    Ok(Json(ApiResponse::new(SeasonsListResponse { seasons })))
}

/// Currently-active season, or `None` when between seasons.
#[utoipa::path(
    get,
    path = "/api/seasons/current",
    tag = "challenges",
    responses((status = 200, description = "Active season or null", body = ApiResponse<CurrentSeasonResponse>)),
)]
pub async fn current_season(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CurrentSeasonResponse>>, AppError> {
    let season = SeasonsService::get_current(&state.db).await?;
    Ok(Json(ApiResponse::new(CurrentSeasonResponse { season })))
}

/// Fetch a season by slug.
#[utoipa::path(
    get,
    path = "/api/seasons/{slug}",
    tag = "challenges",
    params(("slug" = String, Path, description = "Season slug")),
    responses(
        (status = 200, description = "Season detail", body = ApiResponse<SeasonResponse>),
        (status = 404, description = "Slug unknown", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_season(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<SeasonResponse>>, AppError> {
    let season = SeasonsService::get_by_slug(&state.db, &slug).await?;
    Ok(Json(ApiResponse::new(SeasonResponse { season })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSeasonBody {
    #[schema(max_length = 10000)]
    pub slug: String,
    #[schema(max_length = 10000)]
    pub name: String,
    #[schema(max_length = 10000)]
    pub theme: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// Admin only: create a new season.
#[utoipa::path(
    post,
    path = "/api/seasons",
    tag = "admin",
    request_body = CreateSeasonBody,
    responses(
        (status = 200, description = "Season created", body = ApiResponse<SeasonResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_season(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateSeasonBody>,
) -> Result<Json<ApiResponse<SeasonResponse>>, AppError> {
    require_admin(&state, &auth).await?;
    let params = CreateSeasonParams {
        slug: body.slug,
        name: body.name,
        theme: body.theme,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    let season = SeasonsService::create(&state.db, params).await?;
    Ok(Json(ApiResponse::new(SeasonResponse { season })))
}

/// Admin only: promote a season to `active`. Automatically deactivates
/// the previously-active season.
#[utoipa::path(
    post,
    path = "/api/seasons/{slug}/activate",
    tag = "admin",
    params(("slug" = String, Path, description = "Season slug")),
    responses(
        (status = 200, description = "Season activated", body = ApiResponse<SeasonResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Slug unknown", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn activate_season(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<SeasonResponse>>, AppError> {
    require_admin(&state, &auth).await?;
    let season = SeasonsService::activate(&state.db, &slug).await?;
    Ok(Json(ApiResponse::new(SeasonResponse { season })))
}

// ═══════════════════════════════════════════════════════════════════
// Stewards
// ═══════════════════════════════════════════════════════════════════

/// Public: list active stewards for a project.
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/stewards",
    tag = "projects",
    params(("project_id" = Uuid, Path, description = "Project UUID")),
    responses(
        (status = 200, description = "Active stewards", body = ApiResponse<StewardsListResponse>),
    ),
)]
pub async fn list_project_stewards(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<ApiResponse<StewardsListResponse>>, AppError> {
    let stewards = StewardsService::list_project_stewards(&state.db, project_id).await?;
    Ok(Json(ApiResponse::new(StewardsListResponse { stewards })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddStewardBody {
    pub user_id: Uuid,
    /// One of the roles in `StewardsService::VALID_ROLES`.
    #[schema(max_length = 10000)]
    pub role: String,
}

/// Add a steward to a project. Restricted to the project owner or an
/// admin.
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/stewards",
    tag = "projects",
    params(("project_id" = Uuid, Path, description = "Project UUID")),
    request_body = AddStewardBody,
    responses(
        (status = 200, description = "Steward appointed", body = ApiResponse<StewardResponse>),
        (status = 403, description = "Not the project owner nor admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_steward(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<AddStewardBody>,
) -> Result<Json<ApiResponse<StewardResponse>>, AppError> {
    // Autorisation : admin OU project owner
    let is_admin: bool = sqlx::query_scalar("SELECT role = 'admin' FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !is_admin {
        let is_owner: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM projects
                WHERE id = $1 AND owner_type = 'user' AND owner_id = $2
            )",
        )
        .bind(project_id)
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
        if !is_owner {
            return Err(AppError::Forbidden);
        }
    }

    let steward = StewardsService::add(
        &state.db,
        project_id,
        body.user_id,
        &body.role,
        auth.user_id,
    )
    .await?;
    Ok(Json(ApiResponse::new(StewardResponse { steward })))
}

/// Remove a steward from a project (marks `ended_at`). Restricted to
/// the project owner or an admin.
#[utoipa::path(
    delete,
    path = "/api/projects/{project_id}/stewards/{user_id}/{role}",
    tag = "projects",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("user_id" = Uuid, Path, description = "Steward user UUID"),
        ("role" = String, Path, description = "Role slug"),
    ),
    responses(
        (status = 200, description = "Steward removed", body = ApiResponse<SimpleMessage>),
        (status = 403, description = "Not the project owner nor admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove_steward(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((project_id, user_id, role)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let is_admin: bool = sqlx::query_scalar("SELECT role = 'admin' FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !is_admin {
        let is_owner: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM projects
                WHERE id = $1 AND owner_type = 'user' AND owner_id = $2
            )",
        )
        .bind(project_id)
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
        if !is_owner {
            return Err(AppError::Forbidden);
        }
    }

    StewardsService::remove(&state.db, project_id, user_id, &role).await?;
    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Steward removed",
    ))))
}

/// List every project where the caller is an active steward.
#[utoipa::path(
    get,
    path = "/api/users/me/stewardships",
    tag = "projects",
    responses(
        (status = 200, description = "Caller's stewardships", body = ApiResponse<StewardshipsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_stewardships(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<StewardshipsResponse>>, AppError> {
    let stewardships = StewardsService::list_user_stewardships(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(StewardshipsResponse {
        stewardships,
    })))
}
