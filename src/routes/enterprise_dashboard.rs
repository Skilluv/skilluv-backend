use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Enterprise;

pub fn enterprise_dashboard_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/dashboard/platform-stats", get(platform_stats))
        .route("/enterprise/dashboard/my-stats", get(my_stats))
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
    /// `GROUP BY skill_domain` yields a NULL bucket for users who have not
    /// picked a domain. Nullable since migration 0049.
    skill_domain: Option<String>,
    count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TitleCount {
    title: String,
    count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainBucket {
    pub domain: String,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TitleBucket {
    pub title: String,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlatformStatsResponse {
    pub total_talents: i64,
    pub by_domain: Vec<DomainBucket>,
    pub by_title: Vec<TitleBucket>,
    /// Rounded to nearest integer.
    pub avg_fragments: i64,
    pub active_last_30d: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InterestRequestsBreakdown {
    pub total: i64,
    pub pending: i64,
    pub accepted: i64,
    pub declined: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyStatsResponse {
    pub bookmarks: i64,
    pub talent_lists: i64,
    pub interest_requests: InterestRequestsBreakdown,
    pub active_conversations: i64,
    pub team_size: i64,
}

/// Platform-wide talent-pool aggregates visible to any enterprise.
/// Bucketed by skill_domain and title.
#[utoipa::path(
    get,
    path = "/api/enterprise/dashboard/platform-stats",
    tag = "enterprise",
    responses(
        (status = 200, description = "Platform aggregates", body = ApiResponse<PlatformStatsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn platform_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<PlatformStatsResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(PlatformStatsResponse {
        total_talents,
        // The NULL bucket is dropped rather than surfaced: these talents
        // belong to no domain, and `total_talents` already counts them.
        // Keeping the wire shape non-nullable avoids changing the contract.
        by_domain: by_domain
            .iter()
            .filter_map(|d| {
                d.skill_domain.clone().map(|domain| DomainBucket {
                    domain,
                    count: d.count,
                })
            })
            .collect(),
        by_title: by_title
            .iter()
            .map(|t| TitleBucket {
                title: t.title.clone(),
                count: t.count,
            })
            .collect(),
        avg_fragments: avg_fragments.unwrap_or(0.0).round() as i64,
        active_last_30d,
    })))
}

/// Enterprise-scoped stats: bookmarks / lists / interest-request funnel
/// / active conversations / team size.
#[utoipa::path(
    get,
    path = "/api/enterprise/dashboard/my-stats",
    tag = "enterprise",
    responses(
        (status = 200, description = "Enterprise stats", body = ApiResponse<MyStatsResponse>),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyStatsResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(MyStatsResponse {
        bookmarks: bookmarks_count,
        talent_lists: lists_count,
        interest_requests: InterestRequestsBreakdown {
            total: interests_sent,
            pending: interests_pending,
            accepted: interests_accepted,
            declined: interests_declined,
        },
        active_conversations,
        team_size,
    })))
}
