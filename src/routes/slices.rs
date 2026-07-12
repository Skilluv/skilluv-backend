//! Routes HTTP pour les `project_slices` (Phase P1).
//!
//! Endpoints publics :
//!   GET   /api/slices                    — liste des slices open (filtres domain/difficulty/project)
//!   GET   /api/slices/{id}               — détail d'une slice
//!   POST  /api/slices/{id}/claim         — claim une slice (soft-lock 7j)
//!   POST  /api/slices/{id}/unclaim       — relâche sa slice
//!   GET   /api/users/me/slices           — mes slices actives (claimed / in_review)
//!
//! Voir docs/challenges-target-model-and-roadmap.md partie G.1 et H pour
//! les workflows amont/aval (vérification via webhook, review humaine).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{SlicesListFilter, SlicesService};

pub fn slice_routes() -> Router<AppState> {
    Router::new()
        .route("/slices", get(list_open))
        .route("/slices/{id}", get(get_slice))
        .route("/slices/{id}/claim", post(claim_slice))
        .route("/slices/{id}/unclaim", post(unclaim_slice))
        .route("/slices/{id}/claim-as-team", post(claim_slice_as_team))
        .route("/slices/{id}/unclaim-team", post(unclaim_slice_by_team))
        .route("/users/me/slices", get(my_slices))
        .route("/teams/{team_id}/slices", get(team_slices))
}

// ═══════════════════════════════════════════════════════════════════
// Query / body types
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ListQuery {
    domain: Option<String>,
    difficulty: Option<i16>,
    project_id: Option<Uuid>,
    page: Option<i64>,
    per_page: Option<i64>,
}

impl From<ListQuery> for SlicesListFilter {
    fn from(q: ListQuery) -> Self {
        Self {
            domain: q.domain,
            difficulty: q.difficulty,
            project_id: q.project_id,
            page: q.page.unwrap_or(1),
            per_page: q.per_page.unwrap_or(20),
        }
    }
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

// ═══════════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════════

/// GET /api/slices
///
/// Liste paginée des slices `status='open'`. Public (pas d'auth requise) pour que
/// les visiteurs découvrent l'offre. Trié par difficulty ASC, created_at DESC.
async fn list_open(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let filter: SlicesListFilter = query.into();
    let per_page = filter.per_page.clamp(1, 100);
    let page = filter.page.max(1);

    let (slices, total) = SlicesService::list_open(&state.db, &filter).await?;

    let total_pages = if per_page > 0 {
        (total as f64 / per_page as f64).ceil() as i64
    } else {
        0
    };

    Ok(Json(json!({
        "data": slices,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

/// GET /api/slices/{id}
///
/// Détail public d'une slice (peu importe son status — le status est dans la réponse).
async fn get_slice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slice = SlicesService::get(&state.db, id).await?;
    Ok(Json(build_response(json!({ "slice": slice }))))
}

/// POST /api/slices/{id}/claim
///
/// Auth requis. Le user claim la slice pour 7 jours.
async fn claim_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let slice = SlicesService::claim(&state.db, id, auth.user_id).await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "slice": slice,
            "message": "Slice claimed. You have 7 days to complete it."
        }))),
    ))
}

/// POST /api/slices/{id}/unclaim
///
/// Auth requis. Le user relâche sa slice (retour au pool `open`).
async fn unclaim_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slice = SlicesService::unclaim(&state.db, id, auth.user_id).await?;

    Ok(Json(build_response(json!({
        "slice": slice,
        "message": "Slice released. Others can now claim it."
    }))))
}

/// GET /api/users/me/slices
///
/// Auth requis. Liste des slices claimed/in_review par le user courant.
async fn my_slices(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let slices = SlicesService::list_claimed_by(&state.db, auth.user_id).await?;

    Ok(Json(build_response(json!({ "slices": slices }))))
}

// ═══════════════════════════════════════════════════════════════════
// P10.1 : claim collectif par une team persistente
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ClaimAsTeamBody {
    team_id: Uuid,
}

/// Vérifie que le user est membre de la team (best-effort validation).
async fn require_team_member(
    db: &sqlx::PgPool,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let is_member: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM team_members WHERE team_id = $1 AND user_id = $2",
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    if is_member.is_none() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// POST /api/slices/{id}/claim-as-team
///
/// Auth requis + être membre de la team. Claim collectif 7 jours.
async fn claim_slice_as_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ClaimAsTeamBody>,
) -> Result<impl IntoResponse, AppError> {
    require_team_member(&state.db, body.team_id, auth.user_id).await?;
    let slice = SlicesService::claim_as_team(&state.db, id, body.team_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "slice": slice,
            "message": "Slice claimed by team. 7 days to submit a deliverable."
        }))),
    ))
}

/// POST /api/slices/{id}/unclaim-team
async fn unclaim_slice_by_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ClaimAsTeamBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_team_member(&state.db, body.team_id, auth.user_id).await?;
    let slice = SlicesService::unclaim_by_team(&state.db, id, body.team_id).await?;
    Ok(Json(build_response(json!({
        "slice": slice,
        "message": "Team released the slice. Others can now claim it."
    }))))
}

/// GET /api/teams/{team_id}/slices
///
/// Auth requis + membre de la team. Slices actives de la team.
async fn team_slices(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_team_member(&state.db, team_id, auth.user_id).await?;
    let slices = SlicesService::list_claimed_by_team(&state.db, team_id).await?;
    Ok(Json(build_response(json!({ "slices": slices }))))
}
