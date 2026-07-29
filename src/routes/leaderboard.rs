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
use crate::routes::notifications::Pagination;
use crate::services::LeaderboardService;

pub fn leaderboard_routes() -> Router<AppState> {
    Router::new()
        .route("/leaderboards", get(list_leaderboards))
        .route("/leaderboards/{domain}", get(get_leaderboard))
        .route("/leaderboards/{domain}/me", get(my_rank))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardMeta {
    /// `global`, `code`, `design`, `game`, `security`.
    pub domain: &'static str,
    pub periods: &'static [&'static str],
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardsIndexResponse {
    pub leaderboards: Vec<LeaderboardMeta>,
}

/// List the domains + periods the leaderboard service supports.
/// Public, SSR-ready.
#[utoipa::path(
    get,
    path = "/api/leaderboards",
    tag = "profile",
    responses(
        (status = 200, description = "Available leaderboards", body = ApiResponse<LeaderboardsIndexResponse>),
    ),
)]
pub async fn list_leaderboards() -> Json<ApiResponse<LeaderboardsIndexResponse>> {
    let domains = ["global", "code", "design", "game", "security"];
    let periods: &'static [&'static str] = &["alltime", "weekly", "monthly"];

    let leaderboards: Vec<LeaderboardMeta> = domains
        .iter()
        .map(|d| LeaderboardMeta { domain: d, periods })
        .collect();

    Json(ApiResponse::new(LeaderboardsIndexResponse { leaderboards }))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct LeaderboardQuery {
    /// `alltime` (default), `weekly`, `monthly`.
    pub period: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardEntry {
    pub rank: usize,
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub title: String,
    pub golden_stars: i32,
    pub country: Option<String>,
    pub score: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardPage {
    pub domain: String,
    pub period: String,
    pub entries: Vec<LeaderboardEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardPageResponse {
    pub data: LeaderboardPage,
    pub pagination: Pagination,
    pub meta: MetaInfo,
}

/// Paginated leaderboard for a given domain + period. Public,
/// SSR-ready. Ordered by score desc.
#[utoipa::path(
    get,
    path = "/api/leaderboards/{domain}",
    tag = "profile",
    params(
        ("domain" = String, Path, description = "Leaderboard domain (global, code, design, game, security)"),
        LeaderboardQuery,
    ),
    responses(
        (status = 200, description = "Ranked entries", body = LeaderboardPageResponse),
        (status = 400, description = "Invalid domain or period", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardPageResponse>, AppError> {
    LeaderboardService::validate_domain(&domain)?;
    let period = query.period.as_deref().unwrap_or("alltime");
    LeaderboardService::validate_period(period)?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let mut redis = state.redis.clone();

    let entries = LeaderboardService::get_page(
        &mut redis,
        &domain,
        period,
        offset as isize,
        per_page as isize,
    )
    .await?;

    let total = LeaderboardService::get_total(&mut redis, &domain, period).await?;

    // Fetch user info for the entries
    let user_ids: Vec<Uuid> = entries
        .iter()
        .filter_map(|(id_str, _)| id_str.parse().ok())
        .collect();

    let ranked_entries: Vec<LeaderboardEntry> = if user_ids.is_empty() {
        vec![]
    } else {
        let users: Vec<(Uuid, String, String, String, i32, Option<String>)> = sqlx::query_as(
            "SELECT id, username, display_name, title, golden_stars, country FROM users WHERE id = ANY($1)",
        )
        .bind(&user_ids)
        .fetch_all(&state.db)
        .await?;

        let user_map: std::collections::HashMap<Uuid, _> =
            users.into_iter().map(|u| (u.0, u)).collect();

        entries
            .iter()
            .enumerate()
            .filter_map(|(i, (id_str, score))| {
                let uid: Uuid = id_str.parse().ok()?;
                let user = user_map.get(&uid)?;
                Some(LeaderboardEntry {
                    rank: offset as usize + i + 1,
                    user_id: uid,
                    username: user.1.clone(),
                    display_name: user.2.clone(),
                    title: user.3.clone(),
                    golden_stars: user.4,
                    country: user.5.clone(),
                    score: *score as i64,
                })
            })
            .collect()
    };

    Ok(Json(LeaderboardPageResponse {
        data: LeaderboardPage {
            domain,
            period: period.to_string(),
            entries: ranked_entries,
        },
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages: (total as f64 / per_page as f64).ceil() as i64,
        },
        meta: MetaInfo::now(),
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MyRankQuery {
    pub period: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyRankResponse {
    pub domain: String,
    pub period: String,
    /// `None` when the user has no score in this leaderboard.
    pub rank: Option<i64>,
    pub score: Option<i64>,
    pub total_participants: i64,
}

/// Caller's rank + score for a specific leaderboard.
#[utoipa::path(
    get,
    path = "/api/leaderboards/{domain}/me",
    tag = "profile",
    params(
        ("domain" = String, Path, description = "Leaderboard domain"),
        MyRankQuery,
    ),
    responses(
        (status = 200, description = "Caller's rank", body = ApiResponse<MyRankResponse>),
        (status = 400, description = "Invalid domain or period", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_rank(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Query(query): Query<MyRankQuery>,
) -> Result<Json<ApiResponse<MyRankResponse>>, AppError> {
    LeaderboardService::validate_domain(&domain)?;
    let period = query.period.as_deref().unwrap_or("alltime");
    LeaderboardService::validate_period(period)?;

    let mut redis = state.redis.clone();

    let rank = LeaderboardService::get_rank(&mut redis, &domain, period, auth.user_id).await?;
    let score = LeaderboardService::get_score(&mut redis, &domain, period, auth.user_id).await?;
    let total = LeaderboardService::get_total(&mut redis, &domain, period).await?;

    Ok(Json(ApiResponse::new(MyRankResponse {
        domain,
        period: period.to_string(),
        rank,
        score: score.map(|s| s as i64),
        total_participants: total,
    })))
}
