use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::SkillFragment;

pub fn gamification_routes() -> Router<AppState> {
    Router::new()
        .route("/skills/tree", get(my_skill_tree))
        .route("/skills/tree/{user_id}", get(user_skill_tree))
        .route("/activity/heatmap", get(my_heatmap))
        .route("/activity/heatmap/{user_id}", get(user_heatmap))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillLeaf {
    pub sub_skill: String,
    pub fragments: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainBranch {
    pub domain: String,
    pub total_fragments: i32,
    pub skills: Vec<SkillLeaf>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillTreeUser {
    pub id: Uuid,
    pub display_name: String,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SkillTreeResponse {
    pub user: SkillTreeUser,
    pub tree: Vec<DomainBranch>,
}

/// GET /api/skills/tree — the caller's own skill tree.
#[utoipa::path(
    get,
    path = "/api/skills/tree",
    tag = "profile",
    responses(
        (status = 200, description = "Skill tree", body = ApiResponse<SkillTreeResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_skill_tree(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<SkillTreeResponse>>, AppError> {
    skill_tree_for_user(&state, auth.user_id).await
}

/// GET /api/skills/tree/{user_id} — public skill tree of another user.
#[utoipa::path(
    get,
    path = "/api/skills/tree/{user_id}",
    operation_id = "gamificationUserSkillTree",
    tag = "profile",
    params(("user_id" = Uuid, Path, description = "Target user UUID")),
    responses(
        (status = 200, description = "Skill tree", body = ApiResponse<SkillTreeResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "User not found or profile inactive", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn user_skill_tree(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SkillTreeResponse>>, AppError> {
    // Verify user exists and has active profile
    let active: Option<bool> = sqlx::query_scalar("SELECT profile_active FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?;

    match active {
        Some(true) => skill_tree_for_user(&state, user_id).await,
        Some(false) => Err(AppError::NotFound("Profile not active".to_string())),
        None => Err(AppError::NotFound("User not found".to_string())),
    }
}

async fn skill_tree_for_user(
    state: &AppState,
    user_id: Uuid,
) -> Result<Json<ApiResponse<SkillTreeResponse>>, AppError> {
    // Source unique user_skills + skill_nodes (skill_fragments droppée en P8.7).
    let fragments: Vec<SkillFragment> =
        crate::services::SkillsService::list_user_skill_fragments_or_backfill(
            &state.db,
            user_id,
            crate::services::SkillFragmentOrder::ByDomainThenSubskill,
        )
        .await?;

    // Group by domain
    let mut domains: std::collections::HashMap<String, Vec<SkillLeaf>> =
        std::collections::HashMap::new();

    for f in &fragments {
        domains
            .entry(f.skill_domain.clone())
            .or_default()
            .push(SkillLeaf {
                sub_skill: f.sub_skill.clone(),
                fragments: f.fragments,
            });
    }

    // Build tree with domain totals
    let tree: Vec<DomainBranch> = domains
        .into_iter()
        .map(|(domain, skills)| {
            let total: i32 = skills.iter().map(|s| s.fragments).sum();
            DomainBranch {
                domain,
                total_fragments: total,
                skills,
            }
        })
        .collect();

    // User summary
    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(ApiResponse::new(SkillTreeResponse {
        user: SkillTreeUser {
            id: user.id,
            display_name: user.display_name,
            title: user.title,
            golden_stars: user.golden_stars,
            total_fragments: user.total_fragments,
        },
        tree,
    })))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ActivityDay {
    pub activity_date: chrono::NaiveDate,
    pub challenges_completed: i32,
    pub fragments_earned: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HeatmapSummary {
    pub days_active: usize,
    pub total_challenges: i32,
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HeatmapResponse {
    /// Per-day activity for the last 365 days. Days with zero activity
    /// are omitted (front renders empty cells for gaps).
    pub heatmap: Vec<ActivityDay>,
    pub summary: HeatmapSummary,
}

/// GET /api/activity/heatmap — 12-month activity heatmap for the
/// caller. Front renders it as a GitHub-style contribution grid.
#[utoipa::path(
    get,
    path = "/api/activity/heatmap",
    tag = "profile",
    responses(
        (status = 200, description = "12-month heatmap", body = ApiResponse<HeatmapResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_heatmap(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<HeatmapResponse>>, AppError> {
    heatmap_for_user(&state, auth.user_id).await
}

/// GET /api/activity/heatmap/{user_id} — public heatmap of another user.
#[utoipa::path(
    get,
    path = "/api/activity/heatmap/{user_id}",
    tag = "profile",
    params(("user_id" = Uuid, Path, description = "Target user UUID")),
    responses(
        (status = 200, description = "12-month heatmap", body = ApiResponse<HeatmapResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "User not found or profile inactive", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn user_heatmap(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<HeatmapResponse>>, AppError> {
    let active: Option<bool> = sqlx::query_scalar("SELECT profile_active FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?;

    match active {
        Some(true) => heatmap_for_user(&state, user_id).await,
        Some(false) => Err(AppError::NotFound("Profile not active".to_string())),
        None => Err(AppError::NotFound("User not found".to_string())),
    }
}

async fn heatmap_for_user(
    state: &AppState,
    user_id: Uuid,
) -> Result<Json<ApiResponse<HeatmapResponse>>, AppError> {
    let one_year_ago = chrono::Utc::now().date_naive() - chrono::Duration::days(365);

    let activity: Vec<ActivityDay> = sqlx::query_as(
        "SELECT activity_date, challenges_completed, fragments_earned FROM user_activity WHERE user_id = $1 AND activity_date >= $2 ORDER BY activity_date",
    )
    .bind(user_id)
    .bind(one_year_ago)
    .fetch_all(&state.db)
    .await?;

    let total_days_active = activity.len();
    let total_challenges: i32 = activity.iter().map(|a| a.challenges_completed).sum();

    Ok(Json(ApiResponse::new(HeatmapResponse {
        heatmap: activity,
        summary: HeatmapSummary {
            days_active: total_days_active,
            total_challenges,
            period_start: one_year_ago,
            period_end: chrono::Utc::now().date_naive(),
        },
    })))
}
