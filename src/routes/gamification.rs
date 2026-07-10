use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
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

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// GET /api/skills/tree — my skill tree
async fn my_skill_tree(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    skill_tree_for_user(&state, auth.user_id).await
}

// GET /api/skills/tree/:user_id — public skill tree
async fn user_skill_tree(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
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
) -> Result<Json<serde_json::Value>, AppError> {
    // Source unique user_skills + skill_nodes (skill_fragments droppée en P8.7).
    let fragments: Vec<SkillFragment> =
        crate::services::SkillsService::list_user_skill_fragments_or_backfill(
            &state.db,
            user_id,
            crate::services::SkillFragmentOrder::ByDomainThenSubskill,
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

    // Build tree with domain totals
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

    // User summary
    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "user": {
            "id": user.id,
            "display_name": user.display_name,
            "title": user.title,
            "golden_stars": user.golden_stars,
            "total_fragments": user.total_fragments,
        },
        "tree": tree,
    }))))
}

// GET /api/activity/heatmap — my heatmap (12 months)
async fn my_heatmap(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    heatmap_for_user(&state, auth.user_id).await
}

// GET /api/activity/heatmap/:user_id — public heatmap
async fn user_heatmap(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
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

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct ActivityDay {
    activity_date: chrono::NaiveDate,
    challenges_completed: i32,
    fragments_earned: i32,
}

async fn heatmap_for_user(
    state: &AppState,
    user_id: Uuid,
) -> Result<Json<serde_json::Value>, AppError> {
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

    Ok(Json(build_response(json!({
        "heatmap": activity,
        "summary": {
            "days_active": total_days_active,
            "total_challenges": total_challenges,
            "period_start": one_year_ago,
            "period_end": chrono::Utc::now().date_naive(),
        },
    }))))
}
