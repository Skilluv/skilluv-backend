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
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::TracksService;
use crate::services::tracks::{EligibilityCheck, Track, TrackProgress, UserTrack};

pub fn track_routes() -> Router<AppState> {
    Router::new()
        .route("/tracks", get(list_tracks))
        .route("/tracks/{slug}", get(get_track))
        .route("/tracks/{slug}/enroll", post(enroll_track))
        .route("/tracks/{slug}/progress", get(track_progress))
        .route("/users/me/tracks", get(my_tracks))
        .route("/challenges/{id}/eligibility", get(challenge_eligibility))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TracksListResponse {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrackDetailResponse {
    pub track: Track,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnrollResponse {
    pub user_track: UserTrack,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrackProgressResponse {
    pub progress: TrackProgress,
}

/// Enriched view of a user's track — the `slug` / `title` are joined
/// from the `tracks` table so the front doesn't need an N+1 lookup to
/// render the "My tracks" dashboard tile.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyTrackEntry {
    pub track_id: Uuid,
    pub slug: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub current_challenge_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyTracksResponse {
    pub user_tracks: Vec<MyTrackEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EligibilityResponse {
    pub eligibility: EligibilityCheck,
}

// ═══════════════════════════════════════════════════════════════════
// Tracks : lecture publique
// ═══════════════════════════════════════════════════════════════════

/// List every active track. Public — no auth required.
#[utoipa::path(
    get,
    path = "/api/tracks",
    tag = "challenges",
    responses(
        (status = 200, description = "Active tracks", body = ApiResponse<TracksListResponse>),
    ),
)]
pub async fn list_tracks(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<TracksListResponse>>, AppError> {
    let tracks = TracksService::list_active(&state.db).await?;
    Ok(Json(ApiResponse::new(TracksListResponse { tracks })))
}

/// Fetch a track by slug. Public.
#[utoipa::path(
    get,
    path = "/api/tracks/{slug}",
    tag = "challenges",
    params(("slug" = String, Path, description = "Track slug")),
    responses(
        (status = 200, description = "Track detail", body = ApiResponse<TrackDetailResponse>),
        (status = 404, description = "Slug not found", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_track(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<TrackDetailResponse>>, AppError> {
    let track = TracksService::get_by_slug(&state.db, &slug).await?;
    Ok(Json(ApiResponse::new(TrackDetailResponse { track })))
}

// ═══════════════════════════════════════════════════════════════════
// Enrollment (auth)
// ═══════════════════════════════════════════════════════════════════

/// Enroll the current user in a track. Idempotent on the service layer.
#[utoipa::path(
    post,
    path = "/api/tracks/{slug}/enroll",
    tag = "challenges",
    params(("slug" = String, Path, description = "Track slug")),
    responses(
        (status = 200, description = "Enrolled", body = ApiResponse<EnrollResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Track not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn enroll_track(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<EnrollResponse>>, AppError> {
    let user_track = TracksService::enroll(&state.db, auth.user_id, &slug).await?;
    Ok(Json(ApiResponse::new(EnrollResponse {
        user_track,
        message: "Enrolled in track. Follow /tracks/{slug}/progress to track your progress."
            .to_string(),
    })))
}

/// Read the current user's progress on a specific track.
#[utoipa::path(
    get,
    path = "/api/tracks/{slug}/progress",
    tag = "challenges",
    params(("slug" = String, Path, description = "Track slug")),
    responses(
        (status = 200, description = "Progress snapshot", body = ApiResponse<TrackProgressResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Track not found or not enrolled", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn track_progress(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<TrackProgressResponse>>, AppError> {
    let progress = TracksService::get_progress(&state.db, auth.user_id, &slug).await?;
    Ok(Json(ApiResponse::new(TrackProgressResponse { progress })))
}

/// List every track the caller is enrolled in, enriched with slug +
/// title so the front can render dashboard tiles without an extra
/// round trip (BE-P0-34 fix).
#[utoipa::path(
    get,
    path = "/api/users/me/tracks",
    tag = "challenges",
    responses(
        (status = 200, description = "Enrolled tracks", body = ApiResponse<MyTracksResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_tracks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyTracksResponse>>, AppError> {
    let user_tracks = TracksService::list_user_tracks(&state.db, auth.user_id).await?;

    // BE-P0-34 : join with tracks so the front doesn't need N+1 lookups just
    // to show a track name in the "My tracks" dashboard tile.
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, slug, name FROM tracks WHERE id = ANY($1)")
            .bind(user_tracks.iter().map(|t| t.track_id).collect::<Vec<_>>())
            .fetch_all(&state.db)
            .await?;
    let by_id: std::collections::HashMap<Uuid, (String, String)> = rows
        .into_iter()
        .map(|(id, slug, name)| (id, (slug, name)))
        .collect();

    let enriched: Vec<MyTrackEntry> = user_tracks
        .iter()
        .map(|t| {
            let (slug, title) = by_id
                .get(&t.track_id)
                .cloned()
                .unwrap_or_else(|| (String::new(), String::new()));
            MyTrackEntry {
                track_id: t.track_id,
                slug,
                title,
                started_at: t.started_at,
                completed_at: t.completed_at,
                current_challenge_id: t.current_challenge_id,
            }
        })
        .collect();

    Ok(Json(ApiResponse::new(MyTracksResponse {
        user_tracks: enriched,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Éligibilité pour démarrer un challenge (DAG check)
// ═══════════════════════════════════════════════════════════════════

/// Check whether the current user can start a given challenge — walks
/// the prerequisite DAG and reports missing required + recommended
/// prerequisites separately.
#[utoipa::path(
    get,
    path = "/api/challenges/{id}/eligibility",
    tag = "challenges",
    params(("id" = Uuid, Path, description = "Challenge UUID")),
    responses(
        (status = 200, description = "Eligibility report", body = ApiResponse<EligibilityResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Challenge not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn challenge_eligibility(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<ApiResponse<EligibilityResponse>>, AppError> {
    let check = TracksService::check_eligibility(&state.db, auth.user_id, challenge_id).await?;
    Ok(Json(ApiResponse::new(EligibilityResponse {
        eligibility: check,
    })))
}
