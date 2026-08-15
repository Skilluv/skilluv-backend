use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::api_key::ApiKeyAuth;
use crate::models::{BadgeWithEarnedAt, SkillFragment};

/// Public API v1 routes — authenticated via API key.
pub fn public_api_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/users/{username}", get(get_user_profile))
        .route("/v1/users/{username}/badges", get(get_user_badges))
        .route("/v1/users/{username}/skills", get(get_user_skills))
}

/// v1 envelope — {data, meta{api_version: "v1"}}. Distinct from the
/// internal ApiResponse<T> because third parties depend on the
/// `api_version` marker for version-negotiation without an extra
/// request.
#[derive(Debug, Serialize, ToSchema)]
pub struct V1Envelope<T: ToSchema> {
    pub data: T,
    pub meta: V1Meta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct V1Meta {
    pub request_id: String,
    pub timestamp: String,
    pub api_version: &'static str,
}

impl V1Meta {
    fn now() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            api_version: "v1",
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PublicUser {
    id: Uuid,
    username: String,
    display_name: String,
    /// Nullable since migration 0049 — see the note on the profile route.
    skill_domain: Option<String>,
    title: String,
    golden_stars: i32,
    total_fragments: i32,
    streak_current: i32,
    country: Option<String>,
    bio: Option<String>,
    avatar_url: Option<String>,
    github: Option<String>,
    linkedin: Option<String>,
    website: Option<String>,
    twitter: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Public API v1 profile projection. Fields align 1:1 with what
/// `PublicUser` exposes; `challenges_completed` is joined in.
#[derive(Debug, Serialize, ToSchema)]
pub struct V1UserProfile {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    /// `null` when the user has not picked a domain yet.
    pub skill_domain: Option<String>,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub streak_current: i32,
    pub country: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub github: Option<String>,
    pub linkedin: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub challenges_completed: i64,
    /// RFC 3339 timestamp of account creation.
    pub member_since: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct V1UserProfileResponse {
    pub user: V1UserProfile,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct V1UserBadgesResponse {
    pub username: String,
    pub badges: Vec<BadgeWithEarnedAt>,
    pub total: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct V1SkillLeaf {
    pub sub_skill: String,
    pub fragments: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct V1DomainBranch {
    pub domain: String,
    pub total_fragments: i32,
    pub skills: Vec<V1SkillLeaf>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct V1UserSkillsResponse {
    pub username: String,
    pub skill_tree: Vec<V1DomainBranch>,
}

/// Public API v1: get a user profile by username.
/// Requires an API key with `read:profile` permission.
#[utoipa::path(
    get,
    path = "/api/v1/users/{username}",
    tag = "profile",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "Public profile", body = V1Envelope<V1UserProfileResponse>),
        (status = 401, description = "Missing or invalid API key", body = crate::api_response::ErrorResponse),
        (status = 403, description = "API key lacks read:profile permission", body = crate::api_response::ErrorResponse),
        (status = 404, description = "User not found", body = crate::api_response::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_user_profile(
    State(state): State<AppState>,
    api_key: ApiKeyAuth,
    Path(username): Path<String>,
) -> Result<Json<V1Envelope<V1UserProfileResponse>>, AppError> {
    api_key.require_permission("read:profile")?;

    let user: PublicUser = sqlx::query_as(
        "SELECT id, username, display_name, skill_domain, title, golden_stars, total_fragments, streak_current, country, bio, avatar_url, github, linkedin, website, twitter, created_at FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("User not found".to_string()))?;

    // Challenges completed count
    let challenges_completed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions WHERE user_id = $1 AND status = 'success'",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    let _ = json!({}); // keep serde_json in scope for future use
    Ok(Json(V1Envelope {
        data: V1UserProfileResponse {
            user: V1UserProfile {
                id: user.id,
                username: user.username,
                display_name: user.display_name,
                skill_domain: user.skill_domain,
                title: user.title,
                golden_stars: user.golden_stars,
                total_fragments: user.total_fragments,
                streak_current: user.streak_current,
                country: user.country,
                bio: user.bio,
                avatar_url: user.avatar_url,
                github: user.github,
                linkedin: user.linkedin,
                website: user.website,
                twitter: user.twitter,
                challenges_completed,
                member_since: user.created_at.to_rfc3339(),
            },
        },
        meta: V1Meta::now(),
    }))
}

/// Public API v1: list a user's badges. Requires `read:badges`.
#[utoipa::path(
    get,
    path = "/api/v1/users/{username}/badges",
    tag = "profile",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "User badges", body = V1Envelope<V1UserBadgesResponse>),
        (status = 401, description = "Missing or invalid API key", body = crate::api_response::ErrorResponse),
        (status = 403, description = "API key lacks read:badges permission", body = crate::api_response::ErrorResponse),
        (status = 404, description = "User not found", body = crate::api_response::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_user_badges(
    State(state): State<AppState>,
    api_key: ApiKeyAuth,
    Path(username): Path<String>,
) -> Result<Json<V1Envelope<V1UserBadgesResponse>>, AppError> {
    api_key.require_permission("read:badges")?;

    let user_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("User not found".to_string()))?;

    let badges: Vec<BadgeWithEarnedAt> = sqlx::query_as(
        "SELECT b.slug, b.name, b.description, b.icon, b.category, ub.earned_at FROM badges b JOIN user_badges ub ON b.id = ub.badge_id WHERE ub.user_id = $1 ORDER BY ub.earned_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let total = badges.len();
    Ok(Json(V1Envelope {
        data: V1UserBadgesResponse {
            username,
            badges,
            total,
        },
        meta: V1Meta::now(),
    }))
}

/// Public API v1: user's skill tree grouped by domain. Requires
/// `read:skills`.
#[utoipa::path(
    get,
    path = "/api/v1/users/{username}/skills",
    tag = "profile",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "User skill tree", body = V1Envelope<V1UserSkillsResponse>),
        (status = 401, description = "Missing or invalid API key", body = crate::api_response::ErrorResponse),
        (status = 403, description = "API key lacks read:skills permission", body = crate::api_response::ErrorResponse),
        (status = 404, description = "User not found", body = crate::api_response::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_user_skills(
    State(state): State<AppState>,
    api_key: ApiKeyAuth,
    Path(username): Path<String>,
) -> Result<Json<V1Envelope<V1UserSkillsResponse>>, AppError> {
    api_key.require_permission("read:skills")?;

    let user_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("User not found".to_string()))?;

    // Source unique user_skills (skill_fragments droppée en P8.7).
    let fragments: Vec<SkillFragment> =
        crate::services::SkillsService::list_user_skill_fragments_or_backfill(
            &state.db,
            user_id,
            crate::services::SkillFragmentOrder::ByDomainThenFragmentsDesc,
        )
        .await?;

    // Group by domain
    let mut domains: std::collections::HashMap<String, Vec<V1SkillLeaf>> =
        std::collections::HashMap::new();
    for f in &fragments {
        domains
            .entry(f.skill_domain.clone())
            .or_default()
            .push(V1SkillLeaf {
                sub_skill: f.sub_skill.clone(),
                fragments: f.fragments,
            });
    }

    let tree: Vec<V1DomainBranch> = domains
        .into_iter()
        .map(|(domain, skills)| {
            let total: i32 = skills.iter().map(|s| s.fragments).sum();
            V1DomainBranch {
                domain,
                total_fragments: total,
                skills,
            }
        })
        .collect();

    Ok(Json(V1Envelope {
        data: V1UserSkillsResponse {
            username,
            skill_tree: tree,
        },
        meta: V1Meta::now(),
    }))
}
