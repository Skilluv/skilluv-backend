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
use crate::services::tournament;

pub fn tournament_routes() -> Router<AppState> {
    Router::new()
        // Public — tournaments read + registration.
        .route("/tournaments", get(list_tournaments))
        .route("/tournaments/{slug}", get(get_tournament))
        .route("/tournaments/{slug}/leaderboard", get(get_leaderboard))
        .route("/tournaments/{slug}/register", post(register))
        // Code contests (migration 0189) — hand in an entry, read the entries.
        .route(
            "/tournaments/{slug}/submissions",
            get(list_submissions).post(submit_entry),
        )
        .route("/submissions/{id}/judge", post(judge_entry))
        // Public events feed
        .route("/events", get(events_feed))
}

/// Trello vx5q6jW4 — les admin routes de seasons/tournaments vivaient dans
/// `tournament_routes` sans admin_gate (juste un `if auth.role != "admin"`
/// inline). Split maintenant pour permettre à `lib.rs` de nest ce sous-router
/// derrière `admin_gate` (ensure_admin_origin + ensure_admin_2fa) comme les
/// autres surfaces admin.
pub fn admin_tournament_routes() -> Router<AppState> {
    Router::new()
        // Seasons admin (les GET /seasons + /seasons/current publics vivent
        // dans seasons.rs Phase P6). Endpoints admin propres au workflow tournois.
        .route("/admin/seasons", post(admin_create_season))
        .route("/admin/seasons/{id}/status", post(admin_set_season_status))
        .route("/admin/seasons/{id}/close", post(admin_close_season))
        // Tournaments admin.
        .route("/admin/tournaments", post(admin_create_tournament))
        .route(
            "/admin/tournaments/{id}/status",
            post(admin_set_tournament_status),
        )
        .route("/admin/tournaments/{id}/score", post(admin_set_score))
        .route("/admin/tournaments/{id}/conclude", post(admin_conclude))
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

// ─── Seasons admin ───────────────────────────────────────────────
// GET /seasons + /seasons/current publics : voir routes/seasons.rs (P6).

/// Admin: create a season (workflow tournois).
#[utoipa::path(
    post, path = "/api/admin/seasons", tag = "admin",
    request_body(content = serde_json::Value, description = "CreateSeasonInput"),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_create_season(
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct StatusBody {
    #[schema(max_length = 10000)]
    pub status: String,
}

/// Admin: change season status.
#[utoipa::path(
    post, path = "/api/admin/seasons/{id}/status", tag = "admin",
    params(("id" = Uuid, Path)), request_body = StatusBody,
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_set_season_status(
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

/// Admin: close a season and distribute final rewards.
#[utoipa::path(
    post, path = "/api/admin/seasons/{id}/close", tag = "admin",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_close_season(
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListTournamentsQuery {
    #[param(max_length = 50)]
    pub status: Option<String>,
    pub upcoming: Option<bool>,
    #[param(minimum = 1, maximum = 200)]
    pub limit: Option<i64>,
}

/// List tournaments (filterable by status / upcoming).
#[utoipa::path(
    get, path = "/api/tournaments", tag = "challenges",
    params(ListTournamentsQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_tournaments(
    State(state): State<AppState>,
    Query(q): Query<ListTournamentsQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_max_len_opt(&q.status, "status", 50)?;
    crate::validators::check_range_opt(q.limit, "limit", 1, 200)?;
    let rows = tournament::list_tournaments(
        &state.db,
        q.status.as_deref(),
        q.upcoming.unwrap_or(false),
        q.limit.unwrap_or(50),
    )
    .await?;
    Ok(Json(build_response(json!({ "tournaments": rows }))))
}

/// Fetch a tournament by slug.
#[utoipa::path(
    get, path = "/api/tournaments/{slug}", tag = "challenges",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value), (status = 404, body = crate::api_response::ErrorResponse)),
)]
pub async fn get_tournament(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "tournament": t }))))
}

/// Tournament leaderboard.
#[utoipa::path(
    get, path = "/api/tournaments/{slug}/leaderboard", tag = "challenges",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value), (status = 404, body = crate::api_response::ErrorResponse)),
)]
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    // A marathon is scored from upstream contributions nobody files here, so
    // the standing is counted at read time. Once concluded it is frozen —
    // recounting a finished marathon would rewrite a published result.
    if t.kind == "marathon" && t.status != "concluded" {
        crate::services::contest::recompute_marathon_scores(&state.db, t.id).await?;
    }
    let rows = tournament::leaderboard_of(&state.db, t.id).await?;
    Ok(Json(build_response(json!({ "leaderboard": rows }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct RegisterBody {
    /// Required for guild_war tournaments; ignored otherwise.
    pub guild_id: Option<Uuid>,
}

/// Register for a tournament (individual or guild_war).
#[utoipa::path(
    post, path = "/api/tournaments/{slug}/register", tag = "challenges",
    params(("slug" = String, Path)), request_body = RegisterBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn register(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    let participant = if t.kind == "guild_war" {
        let guild_id = body.guild_id.ok_or(AppError::Validation(
            "guild_id is required for guild_war".into(),
        ))?;
        tournament::register_guild(&state.db, t.id, auth.user_id, guild_id).await?
    } else {
        tournament::register_individual(&state.db, t.id, auth.user_id).await?
    };
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            "tournament_registered",
            props(&[("tournament_id", json!(t.id)), ("kind", json!(t.kind))]),
        );
    }
    metrics::counter!("skilluv_tournament_registrations_total", "kind" => t.kind.clone())
        .increment(1);
    Ok(Json(build_response(json!({ "participant": participant }))))
}

// ─── Code contest submissions ────────────────────────────────────

/// Hand in an entry, or revise the one already handed in.
///
/// Revising replaces: every one of these formats asks for one answer. A
/// revision clears any judgement, because a score belongs to the artifact it
/// was given for.
#[utoipa::path(
    post, path = "/api/tournaments/{slug}/submissions", tag = "tournament",
    params(("slug" = String, Path, description = "Tournament slug")),
    request_body(content = serde_json::Value, description = "SubmitInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not open, wrong format, or unregistered", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such contest", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(input): Json<crate::services::contest::SubmitInput>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    let submission = crate::services::contest::submit(&state.db, t.id, auth.user_id, input).await?;
    metrics::counter!("skilluv_contest_submissions_total", "kind" => t.kind.clone()).increment(1);
    Ok(Json(build_response(json!({ "submission": submission }))))
}

/// Every entry in a contest. Public: a contest whose entries cannot be read
/// is a contest whose result cannot be questioned.
#[utoipa::path(
    get, path = "/api/tournaments/{slug}/submissions", tag = "tournament",
    params(("slug" = String, Path, description = "Tournament slug")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such contest", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list_submissions(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let t = tournament::by_slug(&state.db, &slug).await?;
    let submissions = crate::services::contest::list_submissions(&state.db, t.id).await?;
    Ok(Json(build_response(json!({ "submissions": submissions }))))
}

/// Record a judgement and carry it onto the leaderboard.
///
/// Gated on `jury_tournament` rather than on `admin`: judging a TDD contest
/// is a competence, and the people who have it are not the people who
/// administer the platform.
#[utoipa::path(
    post, path = "/api/submissions/{id}/judge", tag = "tournament",
    params(("id" = Uuid, Path, description = "Submission id")),
    request_body(content = serde_json::Value, description = "JudgeInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Refusal with no reason, or a score on a measured contest", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not a juror", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such submission", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn judge_entry(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<crate::services::contest::JudgeInput>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["jury_tournament", "admin"],
    )
    .await?;
    let submission = crate::services::contest::judge(&state.db, id, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "submission": submission }))))
}

/// Admin: create a tournament.
#[utoipa::path(
    post, path = "/api/admin/tournaments", tag = "admin",
    request_body(content = serde_json::Value, description = "CreateTournamentInput"),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_create_tournament(
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

/// Admin: change tournament status.
#[utoipa::path(
    post, path = "/api/admin/tournaments/{id}/status", tag = "admin",
    params(("id" = Uuid, Path)), request_body = StatusBody,
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_set_tournament_status(
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct ScoreBody {
    #[schema(max_length = 10000)]
    pub participant_type: String,
    pub participant_id: Uuid,
    pub score: i32,
}

/// Admin: set a participant's score.
#[utoipa::path(
    post, path = "/api/admin/tournaments/{id}/score", tag = "admin",
    params(("id" = Uuid, Path)), request_body = ScoreBody,
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_set_score(
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

/// Admin: conclude tournament (compute ranks, award prizes, notify top 3).
#[utoipa::path(
    post, path = "/api/admin/tournaments/{id}/conclude", tag = "admin",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_conclude(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    // Count the marathon one last time before the ranks are fixed: a
    // contribution merged on the final day must count, and the concluding
    // admin should not have to remember to refresh first.
    let marathon: bool =
        sqlx::query_scalar("SELECT kind = 'marathon' FROM tournaments WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(false);
    if marathon {
        crate::services::contest::recompute_marathon_scores(&state.db, id).await?;
    }

    let report = tournament::conclude_tournament(&state.db, id).await?;

    // Signed by whoever concluded it: the badge carries a reason, and a
    // reason with no author cannot be questioned.
    let badges_granted = if marathon {
        crate::services::contest::grant_marathon_badges(&state.db, id, auth.user_id).await?
    } else {
        0
    };

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
    let tname_row: Option<(String,)> = sqlx::query_as("SELECT name FROM tournaments WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let tname = tname_row.map(|(n,)| n).unwrap_or_else(|| "Tournoi".into());
    for (ptype, pid, rank, frags, gp) in &top {
        if ptype == "user" {
            let _ = crate::services::notify::send(
                &state,
                crate::services::notify::Recipient::User(*pid),
                "tournament.podium",
            )
            .arg("place", rank.to_string())
            .arg("tournament", tname.clone())
            .payload(json!({
                "tournament_id": id,
                "rank": rank,
                "fragments": frags,
                "gp": gp,
            }))
            .execute()
            .await;
        }
    }
    metrics::counter!("skilluv_tournaments_concluded_total").increment(1);

    // BE-F — audit log unifié.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "tournament_conclude",
            target_type: Some("tournament"),
            target_id: Some(id),
            metadata: Some(json!({
                "name": tname,
                "podium_size": top.len(),
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "conclusion": report,
        "marathon_badges_granted": badges_granted,
    }))))
}

// ─── Events feed (public landing) ────────────────────────────────

/// Public events feed (tournaments + season milestones).
#[utoipa::path(
    get, path = "/api/events", tag = "feed",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn events_feed(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let upcoming = tournament::list_tournaments(&state.db, None, true, 20).await?;
    let current = tournament::current_season(&state.db).await?;
    Ok(Json(build_response(json!({
        "current_season": current,
        "upcoming_tournaments": upcoming,
    }))))
}
