//! Endpoints publics pour enfiler / interroger les jobs IA — Phase 5.
//!
//! POST /api/ai/code-review          {submission_id, ...}       → job_id
//! POST /api/ai/recommendations      {user_snapshot, candidates} → job_id
//! POST /api/admin/ai/hidden-gems    {talents}                   → job_id (admin)
//! POST /api/admin/ai/churn          {talents, horizon_days}     → job_id (admin)
//! GET  /api/ai/jobs/{job_id}                                     → result | pending

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn ai_job_routes() -> Router<AppState> {
    Router::new()
        .route("/ai/code-review", post(request_code_review))
        .route("/ai/recommendations", post(request_recommendations))
        .route("/ai/jobs/{job_id}", get(get_job_result))
        .route("/admin/ai/hidden-gems", post(admin_hidden_gems))
        .route("/admin/ai/churn", post(admin_churn))
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

#[derive(Deserialize)]
struct CodeReviewBody {
    submission_id: Uuid,
    challenge_id: Uuid,
    language: String,
    user_level: Option<String>,
}

async fn request_code_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CodeReviewBody>,
) -> Result<Json<Value>, AppError> {
    // Récupération de la soumission + challenge
    let sub = sqlx::query(
        r#"
        SELECT source_code, test_output
        FROM challenge_submissions
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(body.submission_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("submission not found".into()))?;

    use sqlx::Row;
    let source_code: String = sub.get("source_code");
    let test_output: Option<String> = sub.get("test_output");

    let ch =
        sqlx::query("SELECT title, description, difficulty FROM challenge_templates WHERE id = $1")
            .bind(body.challenge_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("challenge not found".into()))?;
    let title: String = ch.get("title");
    let description: String = ch.get("description");
    let difficulty: i32 = ch.get("difficulty");

    let submission_id = body.submission_id.to_string();
    let challenge_id = body.challenge_id.to_string();
    let user_id = auth.user_id.to_string();
    let user_level = body.user_level.unwrap_or_else(|| "intermediate".into());
    let payload = crate::services::ai_queue::CodeReviewPayload {
        submission_id: &submission_id,
        challenge_id: &challenge_id,
        user_id: &user_id,
        language: &body.language,
        source_code: &source_code,
        challenge_title: &title,
        challenge_description: &description,
        difficulty,
        test_output: test_output.as_deref(),
        user_level: &user_level,
    };
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_code_review(&mut redis, &payload).await?;
    Ok(Json(build_response(json!({ "job_id": job_id }))))
}

async fn request_recommendations(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    // La construction du payload complet (snapshot user + candidats filtrés)
    // reste au client pour éviter une requête DB coûteuse ici : le front peut
    // pré-filtrer la liste des candidats. On force cependant l'user_id à celui
    // de l'authentifié pour éviter le spoofing.
    let mut merged = body;
    if let Some(user) = merged.get_mut("user") {
        if let Some(obj) = user.as_object_mut() {
            obj.insert("user_id".into(), json!(auth.user_id.to_string()));
        }
    }
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_recommendations(&mut redis, &merged).await?;
    Ok(Json(build_response(json!({ "job_id": job_id }))))
}

async fn get_job_result(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mut redis = state.redis.clone();
    match crate::services::ai_queue::fetch_result(&mut redis, &job_id).await? {
        Some(result) => Ok(Json(build_response(json!({
            "status": "ready",
            "result": result
        })))),
        None => Ok(Json(build_response(json!({
            "status": "pending",
            "job_id": job_id
        })))),
    }
}

async fn admin_hidden_gems(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_hidden_gems(&mut redis, &body).await?;
    Ok(Json(build_response(json!({ "job_id": job_id }))))
}

async fn admin_churn(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_churn_analysis(&mut redis, &body).await?;
    Ok(Json(build_response(json!({ "job_id": job_id }))))
}
