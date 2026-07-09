//! Tournaments + seasons + events routes — Phase 2 Sprint 6.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::analytics_consent;
use crate::services::analytics::props;
use crate::services::{NotificationService, tournament};

pub fn tournament_routes() -> Router<AppState> {
    Router::new()
        // Seasons
        .route("/seasons", get(list_seasons))
        .route("/seasons/current", get(current_season))
        .route("/admin/seasons", post(admin_create_season))
        .route("/admin/seasons/{id}/status", post(admin_set_season_status))
        .route("/admin/seasons/{id}/close", post(admin_close_season))
        // Tournaments
        .route("/tournaments", get(list_tournaments))
        .route("/tournaments/{slug}", get(get_tournament))
        .route("/tournaments/{slug}/leaderboard", get(get_leaderboard))
        .route("/tournaments/{slug}/register", post(register))
        .route("/admin/tournaments", post(admin_create_tournament))
        .route("/admin/tournaments/{id}/status", post(admin_set_tournament_status))
        .route("/admin/tournaments/{id}/score", post(admin_set_score))
        .route("/admin/tournaments/{id}/conclude", post(admin_conclude))
        // Public events feed
        .route("/events", get(events_feed))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ─── Seasons ─────────────────────────────────────────────────────

async fn list_seasons(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let rows = tournament::list_seasons(&state.db).await?;
    Ok(Json(build_response(json!({ "seasons": rows }))))
}

async fn current_season(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let s = tournament::current_season(&state.db).await?;
    Ok(Json(build_response(json!({ "season": s }))))
}

async fn admin_create_season(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<tournament::CreateSeasonInput>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let s = tournament::create_season(&state.db, input).await?;
    Ok(Json(build_response(json!({ "season": s }))))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
}

async fn admin_set_season_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let s = tournament::set_season_status(&state.db, id, &body.status).await?;
    Ok(Json(build_response(json!({ "season": s }))))
}

async fn admin_close_season(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let report = tournament::close_season(&state.db, id).await?;
    metrics::counter!("skilluv_seasons_closed_total").increment(1);
    Ok(Json(build_response(json!({ "close_report": report }))))
}

// ─── Tournaments ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListTournamentsQuery {
    status: Option<String>,
    upcoming: Option<bool>,
    limit: Option<i64>,
}

async fn list_tournaments(
    State(state): State<AppState>,
    Query(q): Query<ListTournamentsQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = tournament::list_tournaments(
        &state.db,
        q.status.as_deref(),
        q.upcoming.unwrap_or(false),
        q.limit.unwrap_or(50),
    )
    .await?;
    Ok(Json(build_response(json!({ "tournaments": rows }))))
}

async fn get_tournament(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "tournament": t }))))
}

async fn get_leaderboard(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    let rows = tournament::leaderboard_of(&state.db, t.id).await?;
    Ok(Json(build_response(json!({ "leaderboard": rows }))))
}

#[derive(Deserialize)]
struct RegisterBody {
    /// Required for guild_war tournaments; ignored otherwise.
    guild_id: Option<Uuid>,
}

async fn register(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    let participant = if t.kind == "guild_war" {
        let guild_id = body
            .guild_id
            .ok_or(AppError::Validation("guild_id is required for guild_war".into()))?;
        tournament::register_guild(&state.db, t.id, auth.user_id, guild_id).await?
    } else {
        tournament::register_individual(&state.db, t.id, auth.user_id).await?
    };
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            "tournament_registered",
            props(&[
                ("tournament_id", json!(t.id)),
                ("kind", json!(t.kind)),
            ]),
        );
    }
    metrics::counter!("skilluv_tournament_registrations_total", "kind" => t.kind.clone()).increment(1);
    Ok(Json(build_response(json!({ "participant": participant }))))
}

async fn admin_create_tournament(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<tournament::CreateTournamentInput>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let t = tournament::create_tournament(&state.db, auth.user_id, input).await?;
    metrics::counter!("skilluv_tournaments_created_total", "kind" => t.kind.clone()).increment(1);
    Ok(Json(build_response(json!({ "tournament": t }))))
}

async fn admin_set_tournament_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let t = tournament::set_status(&state.db, id, &body.status).await?;
    Ok(Json(build_response(json!({ "tournament": t }))))
}

#[derive(Deserialize)]
struct ScoreBody {
    participant_type: String,
    participant_id: Uuid,
    score: i32,
}

async fn admin_set_score(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ScoreBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    tournament::set_participant_score(
        &state.db,
        id,
        &body.participant_type,
        body.participant_id,
        body.score,
    )
    .await?;
    Ok(Json(build_response(json!({ "updated": true }))))
}

async fn admin_conclude(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let report = tournament::conclude_tournament(&state.db, id).await?;

    // Notify the top 3 (users only — guilds get their GP, officers will see it in their dashboard).
    let top: Vec<(String, Uuid, i32, i32, i32)> = sqlx::query_as(
        r#"
        SELECT participant_type, participant_id, rank, prize_fragments_awarded, prize_gp_awarded
        FROM tournament_participants
        WHERE tournament_id = $1 AND rank IS NOT NULL AND rank <= 3
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let tname_row: Option<(String,)> =
        sqlx::query_as("SELECT name FROM tournaments WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let tname = tname_row.map(|(n,)| n).unwrap_or_else(|| "Tournoi".into());
    for (ptype, pid, rank, frags, gp) in &top {
        if ptype == "user" {
            let _ = NotificationService::send(
                &state.db,
                &mut state.redis.clone(),
                &state.ws,
                *pid,
                "tournament.podium",
                "Podium d'un tournoi !",
                Some(&format!("{tname} — rang #{rank} (+{frags} fragments)")),
                Some(json!({
                    "tournament_id": id,
                    "rank": rank,
                    "fragments": frags,
                    "gp": gp,
                })),
            )
            .await;
        }
    }
    metrics::counter!("skilluv_tournaments_concluded_total").increment(1);
    Ok(Json(build_response(json!({ "conclusion": report }))))
}

// ─── Events feed (public landing) ────────────────────────────────

async fn events_feed(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let upcoming = tournament::list_tournaments(&state.db, None, true, 20).await?;
    let current = tournament::current_season(&state.db).await?;
    Ok(Json(build_response(json!({
        "current_season": current,
        "upcoming_tournaments": upcoming,
    }))))
}
