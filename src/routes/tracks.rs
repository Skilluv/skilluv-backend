//! Routes HTTP pour les tracks + eligibility (Phase P3).
//!
//! Endpoints :
//!   GET   /api/tracks                        — liste des tracks actifs (public)
//!   GET   /api/tracks/{slug}                 — détail d'un track (public)
//!   POST  /api/tracks/{slug}/enroll          — s'enroller (auth requis)
//!   GET   /api/tracks/{slug}/progress        — progression du user courant (auth)
//!   GET   /api/users/me/tracks               — tous les tracks d'un user (auth)
//!   GET   /api/challenges/{id}/eligibility   — le user courant peut-il start ? (auth)
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections 5.5 et B.10-11.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::TracksService;

pub fn track_routes() -> Router<AppState> {
    Router::new()
        .route("/tracks", get(list_tracks))
        .route("/tracks/{slug}", get(get_track))
        .route("/tracks/{slug}/enroll", post(enroll_track))
        .route("/tracks/{slug}/progress", get(track_progress))
        .route("/users/me/tracks", get(my_tracks))
        .route("/challenges/{id}/eligibility", get(challenge_eligibility))
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

// ═══════════════════════════════════════════════════════════════════
// Tracks : lecture publique
// ═══════════════════════════════════════════════════════════════════

async fn list_tracks(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let tracks = TracksService::list_active(&state.db).await?;
    Ok(Json(build_response(json!({ "tracks": tracks }))))
}

async fn get_track(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let track = TracksService::get_by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "track": track }))))
}

// ═══════════════════════════════════════════════════════════════════
// Enrollment (auth)
// ═══════════════════════════════════════════════════════════════════

async fn enroll_track(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user_track = TracksService::enroll(&state.db, auth.user_id, &slug).await?;
    Ok(Json(build_response(json!({
        "user_track": user_track,
        "message": "Enrolled in track. Follow /tracks/{slug}/progress to track your progress."
    }))))
}

async fn track_progress(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let progress = TracksService::get_progress(&state.db, auth.user_id, &slug).await?;
    Ok(Json(build_response(json!({ "progress": progress }))))
}

async fn my_tracks(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Value>, AppError> {
    let user_tracks = TracksService::list_user_tracks(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "user_tracks": user_tracks }))))
}

// ═══════════════════════════════════════════════════════════════════
// Éligibilité pour démarrer un challenge (DAG check)
// ═══════════════════════════════════════════════════════════════════

async fn challenge_eligibility(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let check = TracksService::check_eligibility(&state.db, auth.user_id, challenge_id).await?;
    Ok(Json(build_response(json!({ "eligibility": check }))))
}
