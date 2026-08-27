//! The game domain's reviewer and admin surface.
//!
//! Not gated on `admin` alone. A game slice is validated and a mod is confirmed
//! by a game reviewer — the derived `game_reviewer:{family}` capabilities, or
//! `game_reviewer:all`, or an administrator. Finalising a jam and featuring a
//! creator are editorial acts reserved to administrators. The split mirrors
//! security: the people who judge the work are not the same as the people who
//! run the platform, and the routing says so.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::middleware::capabilities::require_any_capability;
use crate::services::{game_attestations, game_featured, game_jams, game_mods, game_playtests};

/// Any game reviewer, or an administrator. Playtests are the hard gate on a
/// slice; a reviewer of any family may sign off the validation once it is met.
const GAME_REVIEWER_CAPS: &[&str] = &[
    "game_reviewer:programming",
    "game_reviewer:design",
    "game_reviewer:art-animation",
    "game_reviewer:community",
    "game_reviewer:web3",
    "game_reviewer:all",
    "admin",
];

/// The community family reviews mods, plus the umbrella and admin.
const MOD_REVIEWER_CAPS: &[&str] = &["game_reviewer:community", "game_reviewer:all", "admin"];

pub fn admin_game_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/game/slices/{slice_id}/validate",
            post(validate_slice),
        )
        .route("/admin/game/mods/pending", get(mods_pending))
        .route("/admin/game/mods/{id}/confirm", post(mod_confirm))
        .route("/admin/game/mods/{id}/refuse", post(mod_refuse))
        .route("/admin/game/mods/{id}/downloads", post(mod_downloads))
        .route("/admin/game/jams", post(jam_create))
        .route("/admin/game/jams/{id}/finalize", post(jam_finalize))
        .route(
            "/admin/game/attestations/shipped-title",
            post(issue_shipped_title),
        )
        .route(
            "/admin/game/attestations/open-source",
            post(issue_open_source),
        )
        .route("/admin/game/featured", post(feature_creator))
}

// ── Slices ─────────────────────────────────────────────────────────

async fn validate_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, GAME_REVIEWER_CAPS).await?;
    let deliverable_id = game_playtests::validate_slice(&state.db, slice_id, auth.user_id).await?;
    Ok(Json(ApiResponse::new(
        json!({ "validated": true, "deliverable_id": deliverable_id }),
    )))
}

// ── Mods ───────────────────────────────────────────────────────────

async fn mods_pending(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, MOD_REVIEWER_CAPS).await?;
    let mods = game_mods::list_pending(&state.db, 100).await?;
    Ok(Json(ApiResponse::new(json!({ "mods": mods }))))
}

#[derive(Deserialize)]
struct ReasonBody {
    reason: String,
}

async fn mod_confirm(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, MOD_REVIEWER_CAPS).await?;
    let game_mod = game_mods::confirm(&state.db, id, auth.user_id, &body.reason).await?;
    Ok(Json(ApiResponse::new(json!({ "mod": game_mod }))))
}

async fn mod_refuse(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, MOD_REVIEWER_CAPS).await?;
    let game_mod = game_mods::refuse(&state.db, id, auth.user_id, &body.reason).await?;
    Ok(Json(ApiResponse::new(json!({ "mod": game_mod }))))
}

#[derive(Deserialize)]
struct DownloadsBody {
    downloads: i32,
}

async fn mod_downloads(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DownloadsBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, MOD_REVIEWER_CAPS).await?;
    let game_mod = game_mods::update_downloads(&state.db, id, auth.user_id, body.downloads).await?;
    Ok(Json(ApiResponse::new(json!({ "mod": game_mod }))))
}

// ── Jams ───────────────────────────────────────────────────────────

async fn jam_create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<game_jams::CreateJamInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // An organiser is an administrator or a game reviewer of any family — the
    // person who created it earns the `game_jam_organized` badge.
    require_any_capability(&state.db, auth.user_id, GAME_REVIEWER_CAPS).await?;
    let jam = game_jams::create(&state.db, auth.user_id, input).await?;
    Ok(Json(ApiResponse::new(json!({ "jam": jam }))))
}

async fn jam_finalize(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, &["admin"]).await?;
    let report = game_jams::finalize(&state.db, id).await?;
    Ok(Json(ApiResponse::new(json!({ "report": report }))))
}

// ── Reviewer-confirmed attestations ────────────────────────────────

#[derive(Deserialize)]
struct ShippedTitleBody {
    user_id: Uuid,
    deliverable_id: Uuid,
    store_url: String,
    title: String,
}

async fn issue_shipped_title(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ShippedTitleBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, GAME_REVIEWER_CAPS).await?;
    let issued = game_attestations::issue_shipped_title(
        &state.db,
        body.user_id,
        body.deliverable_id,
        &body.store_url,
        &body.title,
    )
    .await?;
    // The attestation feeds the score and the rank — recompute now.
    let _ = crate::services::proof_hooks::recompute_all_for_user(&state.db, body.user_id).await;
    Ok(Json(ApiResponse::new(json!({ "attestation": issued }))))
}

#[derive(Deserialize)]
struct OpenSourceBody {
    user_id: Uuid,
    deliverable_id: Uuid,
    pr_url: String,
    what_changed: String,
}

async fn issue_open_source(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<OpenSourceBody>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, GAME_REVIEWER_CAPS).await?;
    let issued = game_attestations::issue_open_source_contribution(
        &state.db,
        body.user_id,
        body.deliverable_id,
        &body.pr_url,
        &body.what_changed,
    )
    .await?;
    let _ = crate::services::proof_hooks::recompute_all_for_user(&state.db, body.user_id).await;
    Ok(Json(ApiResponse::new(json!({ "attestation": issued }))))
}

// ── Featured ───────────────────────────────────────────────────────

async fn feature_creator(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<game_featured::FeatureInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, &["admin"]).await?;
    let featured = game_featured::feature(&state.db, input).await?;
    Ok(Json(ApiResponse::new(json!({ "featured": featured }))))
}
