//! Routes HTTP pour les skills (Phase P4).
//!
//! Endpoints :
//!   GET /api/skills                              — catalogue skill_nodes (public)
//!   GET /api/skills/{slug}/talents               — recherche recruteur par skill
//!   GET /api/users/{user_id}/skills              — skill map d'un profil
//!   GET /api/users/me/skill-recommendations      — slices reco basées sur skills
//!                                                  proches d'un level-up (auth)

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{SkillsService, TalentSearchFilter};

pub fn skill_routes() -> Router<AppState> {
    Router::new()
        .route("/skills", get(list_skills))
        .route("/skills/{slug}/talents", get(find_talents))
        .route("/users/{user_id}/skills", get(user_skills))
        .route(
            "/users/me/skill-recommendations",
            get(my_skill_recommendations),
        )
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
// GET /api/skills
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct SkillsQuery {
    domain: Option<String>,
}

async fn list_skills(
    State(state): State<AppState>,
    Query(q): Query<SkillsQuery>,
) -> Result<Json<Value>, AppError> {
    let skills = SkillsService::list_skills(&state.db, q.domain.as_deref()).await?;
    Ok(Json(build_response(json!({ "skills": skills }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/skills/{slug}/talents
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct TalentsQuery {
    min_level: Option<i16>,
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn find_talents(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<TalentsQuery>,
) -> Result<Json<Value>, AppError> {
    let filter = TalentSearchFilter {
        min_level: q.min_level.unwrap_or(3),
        page: q.page.unwrap_or(1),
        per_page: q.per_page.unwrap_or(20),
    };
    let (talents, total) =
        SkillsService::find_talents_by_skill(&state.db, &slug, &filter).await?;

    Ok(Json(json!({
        "data": talents,
        "pagination": {
            "page": filter.page.max(1),
            "per_page": filter.per_page.clamp(1, 100),
            "total": total,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/users/{user_id}/skills
// ═══════════════════════════════════════════════════════════════════

async fn user_skills(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let skills = SkillsService::list_user_skills(&state.db, user_id).await?;
    Ok(Json(build_response(json!({ "skills": skills }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/users/me/skill-recommendations
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct RecommendationsQuery {
    limit: Option<i64>,
}

async fn my_skill_recommendations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RecommendationsQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(10);
    let recommendations =
        SkillsService::recommend_slices_for_user(&state.db, auth.user_id, limit).await?;

    Ok(Json(build_response(json!({
        "recommendations": recommendations,
        "reasoning": "Slices ouvertes qui touchent des skills où tu es à ≤ 3 points d'un level-up"
    }))))
}
