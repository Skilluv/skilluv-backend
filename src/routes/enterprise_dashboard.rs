use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Enterprise;

pub fn enterprise_dashboard_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/dashboard/platform-stats", get(platform_stats))
        .route("/enterprise/dashboard/my-stats", get(my_stats))
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

async fn require_enterprise(state: &AppState, auth: &AuthUser) -> Result<Enterprise, AppError> {
    crate::routes::enterprise::resolve_active_enterprise(
        &state.db,
        auth.user_id,
        auth.active_enterprise_id,
    )
    .await
}

#[derive(Debug, sqlx::FromRow)]
struct DomainCount {
    skill_domain: String,
    count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TitleCount {
    title: String,
    count: i64,
}

// GET /api/enterprise/dashboard/platform-stats
async fn platform_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let _enterprise = require_enterprise(&state, &auth).await?;

    let total_talents: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE role = 'user' AND profile_active = TRUE AND is_banned = FALSE",
    )
    .fetch_one(&state.db)
    .await?;

    let by_domain: Vec<DomainCount> = sqlx::query_as(
        "SELECT skill_domain, COUNT(*) as count FROM users WHERE role = 'user' AND profile_active = TRUE AND is_banned = FALSE GROUP BY skill_domain ORDER BY count DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let by_title: Vec<TitleCount> = sqlx::query_as(
        "SELECT title, COUNT(*) as count FROM users WHERE role = 'user' AND profile_active = TRUE AND is_banned = FALSE GROUP BY title ORDER BY count DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let avg_fragments: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(total_fragments)::FLOAT8 FROM users WHERE role = 'user' AND profile_active = TRUE AND is_banned = FALSE",
    )
    .fetch_one(&state.db)
    .await?;

    let active_last_30d: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT user_id) FROM user_activity WHERE activity_date >= NOW() - INTERVAL '30 days'",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "total_talents": total_talents,
        "by_domain": by_domain.iter().map(|d| json!({"domain": d.skill_domain, "count": d.count})).collect::<Vec<_>>(),
        "by_title": by_title.iter().map(|t| json!({"title": t.title, "count": t.count})).collect::<Vec<_>>(),
        "avg_fragments": avg_fragments.unwrap_or(0.0).round() as i64,
        "active_last_30d": active_last_30d,
    }))))
}

// GET /api/enterprise/dashboard/my-stats
async fn my_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let bookmarks_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM enterprise_bookmarks WHERE enterprise_id = $1")
            .bind(enterprise.id)
            .fetch_one(&state.db)
            .await?;

    let lists_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM talent_lists WHERE enterprise_id = $1")
            .bind(enterprise.id)
            .fetch_one(&state.db)
            .await?;

    let interests_sent: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM interest_requests WHERE enterprise_id = $1")
            .bind(enterprise.id)
            .fetch_one(&state.db)
            .await?;

    let interests_accepted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM interest_requests WHERE enterprise_id = $1 AND status = 'accepted'",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    let interests_declined: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM interest_requests WHERE enterprise_id = $1 AND status = 'declined'",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    let interests_pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM interest_requests WHERE enterprise_id = $1 AND status = 'pending'",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    let active_conversations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE enterprise_id = $1 AND closed = FALSE",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    let team_size: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enterprise_members WHERE enterprise_id = $1 AND status = 'active'",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "bookmarks": bookmarks_count,
        "talent_lists": lists_count,
        "interest_requests": {
            "total": interests_sent,
            "pending": interests_pending,
            "accepted": interests_accepted,
            "declined": interests_declined,
        },
        "active_conversations": active_conversations,
        "team_size": team_size,
    }))))
}
