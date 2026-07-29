//! Routes HTTP pour les skills (Phase P4).
//!
//! Endpoints :
//!   GET /api/skills                              — catalogue skill_nodes (public)
//!   GET /api/skills/{slug}/talents               — recherche recruteur par skill
//!   GET /api/users/{user_id}/skills              — skill map d'un profil
//!   GET /api/users/me/skill-recommendations      — slices reco basées sur skills
//!                                                  proches d'un level-up (auth)

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::{ApiResponse, MetaInfo};
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::SkillNode;
use crate::services::skills::{SkillTalent, SliceRecommendation, UserSkillEnriched};
use crate::services::{SkillsService, TalentSearchFilter};

pub fn skill_routes() -> Router<AppState> {
    Router::new()
        .route("/skills", get(list_skills))
        .route("/skills/{slug}/talents", get(find_talents))
        .route("/users/{user_id}/skills", get(user_skills))
        .route(
            "/users/me/skill-recommendations",
            get(my_skill_recommendations),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SkillsQuery {
    /// Optional filter on skill_nodes.domain (`code`, `design`, …).
    pub domain: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillsListResponse {
    pub skills: Vec<SkillNode>,
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/skills
// ═══════════════════════════════════════════════════════════════════

/// Public skill catalogue. Optional `?domain=` filter.
#[utoipa::path(
    get,
    path = "/api/skills",
    tag = "profile",
    params(SkillsQuery),
    responses(
        (status = 200, description = "Skill nodes catalogue", body = ApiResponse<SkillsListResponse>),
    ),
)]
pub async fn list_skills(
    State(state): State<AppState>,
    Query(q): Query<SkillsQuery>,
) -> Result<Json<ApiResponse<SkillsListResponse>>, AppError> {
    let skills = SkillsService::list_skills(&state.db, q.domain.as_deref()).await?;
    Ok(Json(ApiResponse::new(SkillsListResponse { skills })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/skills/{slug}/talents
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
pub struct TalentsQuery {
    /// Minimum proficiency level. Defaults to 3.
    pub min_level: Option<i16>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillTalentsResponse {
    pub data: Vec<SkillTalent>,
    pub pagination: crate::routes::notifications::Pagination,
    pub meta: MetaInfo,
}

/// Recruiter view: find talents proficient in a given skill. Paginated.
#[utoipa::path(
    get,
    path = "/api/skills/{slug}/talents",
    tag = "enterprise",
    params(
        ("slug" = String, Path, description = "Skill slug"),
        TalentsQuery,
    ),
    responses(
        (status = 200, description = "Matching talents (paginated)", body = SkillTalentsResponse),
    ),
)]
pub async fn find_talents(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<TalentsQuery>,
) -> Result<Json<SkillTalentsResponse>, AppError> {
    let filter = TalentSearchFilter {
        min_level: q.min_level.unwrap_or(3),
        page: q.page.unwrap_or(1),
        per_page: q.per_page.unwrap_or(20),
    };
    let (talents, total) = SkillsService::find_talents_by_skill(&state.db, &slug, &filter).await?;

    let page = filter.page.max(1);
    let per_page = filter.per_page.clamp(1, 100);
    Ok(Json(SkillTalentsResponse {
        data: talents,
        pagination: crate::routes::notifications::Pagination {
            page,
            per_page,
            total,
            total_pages: (total as f64 / per_page as f64).ceil() as i64,
        },
        meta: MetaInfo::now(),
    }))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/users/{user_id}/skills
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, ToSchema)]
pub struct UserSkillsResponse {
    pub skills: Vec<UserSkillEnriched>,
}

/// Public: enriched skill map of a user (proficiency levels + proven
/// counts + top proof deliverable ids).
#[utoipa::path(
    get,
    path = "/api/users/{user_id}/skills",
    tag = "profile",
    params(("user_id" = Uuid, Path, description = "User UUID")),
    responses(
        (status = 200, description = "User skill map", body = ApiResponse<UserSkillsResponse>),
    ),
)]
pub async fn user_skills(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<UserSkillsResponse>>, AppError> {
    let skills = SkillsService::list_user_skills(&state.db, user_id).await?;
    Ok(Json(ApiResponse::new(UserSkillsResponse { skills })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/users/me/skill-recommendations
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
pub struct RecommendationsQuery {
    /// Max recommendations to return. Defaults to 10.
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillRecommendationsResponse {
    pub recommendations: Vec<SliceRecommendation>,
    /// Human-readable reasoning shown next to the list.
    pub reasoning: String,
}

/// Personalised slice recommendations for the caller: open slices that
/// touch skills where they are ≤3 wpc from a level-up.
#[utoipa::path(
    get,
    path = "/api/users/me/skill-recommendations",
    tag = "profile",
    params(RecommendationsQuery),
    responses(
        (status = 200, description = "Recommended slices", body = ApiResponse<SkillRecommendationsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_skill_recommendations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RecommendationsQuery>,
) -> Result<Json<ApiResponse<SkillRecommendationsResponse>>, AppError> {
    let limit = q.limit.unwrap_or(10);
    let recommendations =
        SkillsService::recommend_slices_for_user(&state.db, auth.user_id, limit).await?;

    Ok(Json(ApiResponse::new(SkillRecommendationsResponse {
        recommendations,
        reasoning: "Slices ouvertes qui touchent des skills où tu es à ≤ 3 points d'un level-up"
            .to_string(),
    })))
}
