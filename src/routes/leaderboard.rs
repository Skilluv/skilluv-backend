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
    /// `global`, or any of `validators::SKILL_DOMAINS`.
    pub domain: &'static str,
    pub periods: &'static [&'static str],
}

/// `global`, or any of the twelve skill domains.
///
/// An enum and not a `schema_with` on an `IntoParams` struct: that produced a
/// *second* `domain` path parameter in the document — utoipa derives one from
/// the handler's `Path<String>` as well — and schemathesis read the looser of
/// the pair, so `/api/leaderboards/0` counted as compliant and the endpoint's
/// correct 400 read as a contract violation. The tuple form takes a type, and
/// a type is unambiguous.
///
/// The list is transcribed, which is the thing this file spent a commit
/// removing elsewhere. `a_leaderboard_domain_is_global_or_a_skill_domain`
/// compares it to `validators::SKILL_DOMAINS` so the copy cannot drift
/// silently — the same contract `SkillDomain` carries.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeaderboardDomain {
    Global,
    Code,
    Design,
    Game,
    Security,
    Ops,
    Ai,
    SoftSkills,
    Audio,
    Quality,
    Leadership,
    Communication,
    Education,
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
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct LeaderboardQuery {
    /// `alltime` (default), `weekly`, `monthly`.
    #[param(pattern = r"^(alltime|weekly|monthly)$")]
    pub period: Option<String>,
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 50)]
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
        ("domain" = LeaderboardDomain, Path, description = "Leaderboard domain"),
        LeaderboardQuery,
    ),
    responses(
        (status = 200, description = "Ranked entries", body = LeaderboardPageResponse),
        (status = 400, description = "Invalid domain or period", body = crate::api_response::ErrorResponse),
    ),
    operation_id = "leaderboardGetLeaderboard",
)]
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardPageResponse>, AppError> {
    LeaderboardService::validate_domain(&domain)?;
    let period = query.period.as_deref().unwrap_or("alltime");
    LeaderboardService::validate_period(period)?;
    crate::validators::check_range_opt(query.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(query.per_page, "per_page", 1, 50)?;

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
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct MyRankQuery {
    #[param(pattern = r"^(alltime|weekly|monthly)$")]
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
        ("domain" = LeaderboardDomain, Path, description = "Leaderboard domain"),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The path parameter lists exactly what `LeaderboardService::validate_domain`
    /// accepts: `global` plus every skill domain.
    ///
    /// The enum is a transcription and this is what stops it drifting. The
    /// previous document froze at four domains while the guard took thirteen,
    /// and every `ops`, `ai` or `audio` leaderboard was reachable and
    /// undocumented.
    #[test]
    fn a_leaderboard_domain_is_global_or_a_skill_domain() {
        let schema =
            serde_json::to_value(<LeaderboardDomain as utoipa::PartialSchema>::schema()).unwrap();
        let documented: Vec<String> = schema["enum"]
            .as_array()
            .expect("a unit enum documents its values under `enum`")
            .iter()
            .map(|v| v.as_str().expect("each value is a string").to_string())
            .collect();

        let expected: Vec<String> = std::iter::once("global".to_string())
            .chain(
                crate::validators::SKILL_DOMAINS
                    .iter()
                    .map(|d| (*d).to_string()),
            )
            .collect();

        assert_eq!(
            documented, expected,
            "the documented leaderboard domains and the guard have drifted"
        );

        // And each one really is accepted, which is the claim a reader of the
        // document makes when they build a URL from it.
        for domain in &documented {
            assert!(
                crate::services::LeaderboardService::validate_domain(domain).is_ok(),
                "{domain} is documented and refused"
            );
        }
    }
}
