//! The game domain, from a creator's side.
//!
//! ## What is public and what needs a session
//!
//! Reading is open: the jams, a jam's detail, the composition of a project, the
//! featured creators, a slice's playtest verdicts and where it stands against
//! the validation gate. A programme nobody can read is a programme nobody
//! joins. Everything that writes — submitting to a jam, voting, playtesting,
//! registering a mod, recomputing your own score — needs a session, because
//! each is a person's own act.
//!
//! Validation, confirmation and finalisation are not here: they are reviewer
//! and admin acts, and they live in [`crate::routes::admin_game`].

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{
    game_composition, game_featured, game_jams, game_mods, game_playtests, game_profile,
};

pub fn game_routes() -> Router<AppState> {
    Router::new()
        // Jams
        .route("/game/jams/{id}", get(jam_detail))
        .route("/game/jams/{id}/submit", post(jam_submit))
        .route("/game/jams/{id}/vote", post(jam_vote))
        // Playtests
        .route(
            "/game/slices/{slice_id}/playtests",
            get(playtests_list).post(playtest_submit),
        )
        .route(
            "/game/slices/{slice_id}/playtests/recruit",
            post(recruit_open),
        )
        .route("/game/slices/{slice_id}/gate", get(gate_status))
        .route(
            "/game/playtests/recruitments/{id}/close",
            post(recruit_close),
        )
        // Mods
        .route("/game/mods", post(mod_register))
        .route("/game/mods/mine", get(mods_mine))
        .route("/game/mods/{id}", get(mod_get))
        // Composition
        .route("/game/projects/{id}/composition", get(composition))
        // Featured
        .route("/game/featured", get(featured_recent))
        .route("/game/featured/week/{date}", get(featured_of_week))
        .route("/game/creators/{user_id}/featured", get(featured_of_user))
        // Own craft score
        .route("/game/profile", get(profile_get))
        .route("/game/profile/recompute", post(profile_recompute))
}

// ── Jams ───────────────────────────────────────────────────────────

async fn jam_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let jam = game_jams::get(&state.db, id).await?;
    Ok(Json(ApiResponse::new(json!({ "jam": jam }))))
}

async fn jam_submit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<game_jams::SubmitInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let submission_id = game_jams::submit(&state.db, auth.user_id, id, input).await?;
    Ok(Json(ApiResponse::new(
        json!({ "submission_id": submission_id }),
    )))
}

#[derive(Deserialize)]
struct VoteBody {
    submission_id: Uuid,
    axis: String,
    score: i16,
}

async fn jam_vote(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VoteBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    game_jams::vote(
        &state.db,
        auth.user_id,
        id,
        body.submission_id,
        &body.axis,
        body.score,
    )
    .await?;
    Ok(Json(ApiResponse::new(json!({ "voted": true }))))
}

// ── Playtests ──────────────────────────────────────────────────────

async fn playtests_list(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let playtests = game_playtests::list_for_slice(&state.db, slice_id).await?;
    Ok(Json(ApiResponse::new(json!({ "playtests": playtests }))))
}

async fn playtest_submit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(mut input): Json<game_playtests::SubmitInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // The slice in the path is the authority; a mismatched body cannot retarget.
    input.slice_id = slice_id;
    let playtest = game_playtests::submit(&state.db, auth.user_id, input).await?;
    Ok(Json(ApiResponse::new(json!({ "playtest": playtest }))))
}

async fn gate_status(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let gate = game_playtests::gate_status(&state.db, slice_id).await?;
    Ok(Json(ApiResponse::new(json!({ "gate": gate }))))
}

async fn recruit_open(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(mut input): Json<game_playtests::OpenRecruitmentInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    input.slice_id = slice_id;
    let recruitment = game_playtests::open_recruitment(&state.db, auth.user_id, input).await?;
    Ok(Json(ApiResponse::new(
        json!({ "recruitment": recruitment }),
    )))
}

async fn recruit_close(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let recruitment = game_playtests::close_recruitment(&state.db, id, auth.user_id).await?;
    Ok(Json(ApiResponse::new(
        json!({ "recruitment": recruitment }),
    )))
}

// ── Mods ───────────────────────────────────────────────────────────

async fn mod_register(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<game_mods::RegisterInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let game_mod = game_mods::register(&state.db, auth.user_id, input).await?;
    Ok(Json(ApiResponse::new(json!({ "mod": game_mod }))))
}

async fn mods_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let mods = game_mods::list_for_author(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(json!({ "mods": mods }))))
}

async fn mod_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let game_mod = game_mods::get(&state.db, id).await?;
    Ok(Json(ApiResponse::new(json!({ "mod": game_mod }))))
}

// ── Composition ────────────────────────────────────────────────────

async fn composition(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let composition = game_composition::composition_of(&state.db, id).await?;
    Ok(Json(ApiResponse::new(
        json!({ "composition": composition }),
    )))
}

// ── Featured ───────────────────────────────────────────────────────

async fn featured_recent(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let featured = game_featured::recent(&state.db, 20).await?;
    Ok(Json(ApiResponse::new(json!({ "featured": featured }))))
}

async fn featured_of_week(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let featured = game_featured::of_week(&state.db, date).await?;
    Ok(Json(ApiResponse::new(json!({ "featured": featured }))))
}

async fn featured_of_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let featured = game_featured::latest_for_user(&state.db, user_id).await?;
    Ok(Json(ApiResponse::new(json!({ "featured": featured }))))
}

// ── Own craft score ────────────────────────────────────────────────

async fn profile_get(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let score = game_profile::compute(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(json!({
        "score": score,
        "cap": game_profile::CAP,
    }))))
}

async fn profile_recompute(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let score = game_profile::recompute(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(json!({ "score": score }))))
}
