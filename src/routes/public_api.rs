use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
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

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "api_version": "v1",
        }
    })
}

#[derive(Debug, sqlx::FromRow)]
struct PublicUser {
    id: Uuid,
    username: String,
    display_name: String,
    skill_domain: String,
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

// GET /api/v1/users/{username} — profile via API key
async fn get_user_profile(
    State(state): State<AppState>,
    api_key: ApiKeyAuth,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
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

    Ok(Json(build_response(json!({
        "user": {
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "skill_domain": user.skill_domain,
            "title": user.title,
            "golden_stars": user.golden_stars,
            "total_fragments": user.total_fragments,
            "streak_current": user.streak_current,
            "country": user.country,
            "bio": user.bio,
            "avatar_url": user.avatar_url,
            "github": user.github,
            "linkedin": user.linkedin,
            "website": user.website,
            "twitter": user.twitter,
            "challenges_completed": challenges_completed,
            "member_since": user.created_at.to_rfc3339(),
        }
    }))))
}

// GET /api/v1/users/{username}/badges
async fn get_user_badges(
    State(state): State<AppState>,
    api_key: ApiKeyAuth,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
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

    Ok(Json(build_response(json!({
        "username": username,
        "badges": badges,
        "total": badges.len(),
    }))))
}

// GET /api/v1/users/{username}/skills
async fn get_user_skills(
    State(state): State<AppState>,
    api_key: ApiKeyAuth,
    Path(username): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    api_key.require_permission("read:skills")?;

    let user_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("User not found".to_string()))?;

    // P8.6 : fallback vers user_skills si skill_fragments vide pour ce user.
    let fragments: Vec<SkillFragment> =
        crate::services::SkillsService::list_user_skill_fragments_or_backfill(
            &state.db,
            user_id,
            crate::services::SkillFragmentOrder::ByDomainThenFragmentsDesc,
        )
        .await?;

    // Group by domain
    let mut domains: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for f in &fragments {
        domains
            .entry(f.skill_domain.clone())
            .or_default()
            .push(json!({
                "sub_skill": f.sub_skill,
                "fragments": f.fragments,
            }));
    }

    let tree: Vec<serde_json::Value> = domains
        .into_iter()
        .map(|(domain, skills)| {
            let total: i32 = skills
                .iter()
                .filter_map(|s| s["fragments"].as_i64())
                .sum::<i64>() as i32;
            json!({
                "domain": domain,
                "total_fragments": total,
                "skills": skills,
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "username": username,
        "skill_tree": tree,
    }))))
}
