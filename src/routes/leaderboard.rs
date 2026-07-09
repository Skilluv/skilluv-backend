use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::LeaderboardService;

pub fn leaderboard_routes() -> Router<AppState> {
    Router::new()
        .route("/leaderboards", get(list_leaderboards))
        .route("/leaderboards/{domain}", get(get_leaderboard))
        .route("/leaderboards/{domain}/me", get(my_rank))
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

// GET /api/leaderboards — list available leaderboards (no auth, SSR-ready)
async fn list_leaderboards() -> Json<serde_json::Value> {
    let domains = ["global", "code", "design", "game", "security"];
    let periods = ["alltime", "weekly", "monthly"];

    let leaderboards: Vec<serde_json::Value> = domains
        .iter()
        .map(|d| {
            json!({
                "domain": d,
                "periods": periods,
            })
        })
        .collect();

    Json(build_response(json!({ "leaderboards": leaderboards })))
}

#[derive(Debug, Deserialize)]
struct LeaderboardQuery {
    period: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

// GET /api/leaderboards/{domain}?period=alltime&page=1&per_page=20 (no auth, SSR-ready)
async fn get_leaderboard(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
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

    let ranked_entries = if user_ids.is_empty() {
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
                Some(json!({
                    "rank": offset as usize + i + 1,
                    "user_id": uid,
                    "username": user.1,
                    "display_name": user.2,
                    "title": user.3,
                    "golden_stars": user.4,
                    "country": user.5,
                    "score": *score as i64,
                }))
            })
            .collect::<Vec<_>>()
    };

    Ok(Json(json!({
        "data": {
            "domain": domain,
            "period": period,
            "entries": ranked_entries,
        },
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

#[derive(Debug, Deserialize)]
struct MyRankQuery {
    period: Option<String>,
}

// GET /api/leaderboards/{domain}/me — my rank (auth required)
async fn my_rank(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(domain): Path<String>,
    Query(query): Query<MyRankQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    LeaderboardService::validate_domain(&domain)?;
    let period = query.period.as_deref().unwrap_or("alltime");
    LeaderboardService::validate_period(period)?;

    let mut redis = state.redis.clone();

    let rank = LeaderboardService::get_rank(&mut redis, &domain, period, auth.user_id).await?;
    let score = LeaderboardService::get_score(&mut redis, &domain, period, auth.user_id).await?;
    let total = LeaderboardService::get_total(&mut redis, &domain, period).await?;

    Ok(Json(build_response(json!({
        "domain": domain,
        "period": period,
        "rank": rank,
        "score": score.map(|s| s as i64),
        "total_participants": total,
    }))))
}
