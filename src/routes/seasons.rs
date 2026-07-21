//! Routes HTTP pour seasons + stewards (Phase P6).
//!
//! Endpoints seasons :
//!   GET  /api/seasons                          — liste (public)
//!   GET  /api/seasons/current                  — saison active (public)
//!   GET  /api/seasons/{slug}                   — détail (public)
//!   POST /api/seasons                          — création (admin)
//!   POST /api/seasons/{slug}/activate          — activate (admin)
//!
//! Endpoints stewards :
//!   GET    /api/projects/{project_id}/stewards           — liste (public)
//!   POST   /api/projects/{project_id}/stewards           — add (project owner ou admin)
//!   DELETE /api/projects/{project_id}/stewards/{user_id}/{role} — remove
//!   GET    /api/users/me/stewardships                    — mes projets

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{CreateSeasonParams, SeasonsService, StewardsService};

pub fn season_routes() -> Router<AppState> {
    Router::new()
        .route("/seasons", get(list_seasons).post(create_season))
        .route("/seasons/current", get(current_season))
        .route("/seasons/{slug}", get(get_season))
        .route("/seasons/{slug}/activate", post(activate_season))
        .route(
            "/projects/{project_id}/stewards",
            get(list_project_stewards).post(add_steward),
        )
        .route(
            "/projects/{project_id}/stewards/{user_id}/{role}",
            delete(remove_steward),
        )
        .route("/users/me/stewardships", get(my_stewardships))
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

// P21.1 : délègue à user_capabilities (source de vérité canonique).
async fn require_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await
}

// ═══════════════════════════════════════════════════════════════════
// Seasons
// ═══════════════════════════════════════════════════════════════════

async fn list_seasons(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let seasons = SeasonsService::list_all(&state.db).await?;
    Ok(Json(build_response(json!({ "seasons": seasons }))))
}

async fn current_season(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let season = SeasonsService::get_current(&state.db).await?;
    Ok(Json(build_response(json!({ "season": season }))))
}

async fn get_season(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let season = SeasonsService::get_by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "season": season }))))
}

#[derive(Deserialize)]
struct CreateSeasonBody {
    slug: String,
    name: String,
    theme: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
}

async fn create_season(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateSeasonBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let params = CreateSeasonParams {
        slug: body.slug,
        name: body.name,
        theme: body.theme,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    let season = SeasonsService::create(&state.db, params).await?;
    Ok(Json(build_response(json!({ "season": season }))))
}

async fn activate_season(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let season = SeasonsService::activate(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "season": season }))))
}

// ═══════════════════════════════════════════════════════════════════
// Stewards
// ═══════════════════════════════════════════════════════════════════

async fn list_project_stewards(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let stewards = StewardsService::list_project_stewards(&state.db, project_id).await?;
    Ok(Json(build_response(json!({ "stewards": stewards }))))
}

#[derive(Deserialize)]
struct AddStewardBody {
    user_id: Uuid,
    role: String,
}

async fn add_steward(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<AddStewardBody>,
) -> Result<Json<Value>, AppError> {
    // Autorisation : admin OU project owner
    let is_admin: bool = sqlx::query_scalar("SELECT role = 'admin' FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !is_admin {
        let is_owner: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM projects
                WHERE id = $1 AND owner_type = 'user' AND owner_id = $2
            )",
        )
        .bind(project_id)
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
        if !is_owner {
            return Err(AppError::Forbidden);
        }
    }

    let steward = StewardsService::add(
        &state.db,
        project_id,
        body.user_id,
        &body.role,
        auth.user_id,
    )
    .await?;
    Ok(Json(build_response(json!({ "steward": steward }))))
}

async fn remove_steward(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((project_id, user_id, role)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<Value>, AppError> {
    let is_admin: bool = sqlx::query_scalar("SELECT role = 'admin' FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !is_admin {
        let is_owner: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM projects
                WHERE id = $1 AND owner_type = 'user' AND owner_id = $2
            )",
        )
        .bind(project_id)
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
        if !is_owner {
            return Err(AppError::Forbidden);
        }
    }

    StewardsService::remove(&state.db, project_id, user_id, &role).await?;
    Ok(Json(build_response(json!({ "removed": true }))))
}

async fn my_stewardships(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let stewardships = StewardsService::list_user_stewardships(&state.db, auth.user_id).await?;
    Ok(Json(build_response(
        json!({ "stewardships": stewardships }),
    )))
}
