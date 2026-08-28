//! Endpoints publics pour enfiler / interroger les jobs IA — Phase 5.
//!
//! POST /api/assistant/code-review       {submission_id, ...}       → job_id
//! POST /api/assistant/recommendations   {user_snapshot, candidates} → job_id
//! POST /api/admin/assistant/hidden-gems {talents}                   → job_id (admin)
//! POST /api/admin/assistant/churn       {talents, horizon_days}     → job_id (admin)
//! GET  /api/assistant/jobs/{job_id}                                 → result | pending
//!
//! Sous `/assistant` et non `/ai` : `ai` est le nom d'un domaine de métier —
//! dix orientations, des grilles de revue, des artefacts. L'assistant qui
//! relit une soumission n'en fait pas partie, et les servir sous le même
//! préfixe rendait l'API illisible pour qui la découvre.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn ai_job_routes() -> Router<AppState> {
    Router::new()
        .route("/assistant/code-review", post(request_code_review))
        .route("/assistant/recommendations", post(request_recommendations))
        .route("/assistant/jobs/{job_id}", get(get_job_result))
        .route("/admin/assistant/hidden-gems", post(admin_hidden_gems))
        .route("/admin/assistant/churn", post(admin_churn))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CodeReviewBody {
    pub submission_id: Uuid,
    pub challenge_id: Uuid,
    /// Judge0 language slug (`rust`, `python`, …).
    #[schema(max_length = 10000)]
    pub language: String,
    /// Optional caller-provided level hint. Defaults to `intermediate`.
    #[schema(max_length = 10000)]
    pub user_level: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiJobEnqueuedResponse {
    /// Opaque job id — poll `/api/assistant/jobs/{job_id}` until `status ==
    /// "ready"` to fetch the result.
    pub job_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiJobResultResponse {
    /// Either `"ready"` or `"pending"`. When `pending`, `result` is
    /// absent and `job_id` is echoed back.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

/// Enqueue a code-review job for one of the caller's submissions.
/// The heavy lifting happens in a background worker (skilluv-ia); poll
/// `/api/assistant/jobs/{job_id}` for the result.
#[utoipa::path(
    post,
    path = "/api/assistant/code-review",
    tag = "challenges",
    request_body = CodeReviewBody,
    responses(
        (status = 200, description = "Job enqueued", body = ApiResponse<AiJobEnqueuedResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Submission or challenge not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_code_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CodeReviewBody>,
) -> Result<Json<ApiResponse<AiJobEnqueuedResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(AiJobEnqueuedResponse { job_id })))
}

/// Enqueue a personalised-recommendations job. Payload shape is
/// front-defined (user snapshot + candidate list) — the backend
/// spoofs the `user.user_id` field to the authenticated user's id.
/// Kept as free-form JSON since the payload evolves quickly.
#[utoipa::path(
    post,
    path = "/api/assistant/recommendations",
    tag = "feed",
    request_body(content = serde_json::Value, description = "Free-form snapshot + candidates payload"),
    responses(
        (status = 200, description = "Job enqueued", body = ApiResponse<AiJobEnqueuedResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_recommendations(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<AiJobEnqueuedResponse>>, AppError> {
    // La construction du payload complet (snapshot user + candidats filtrés)
    // reste au client pour éviter une requête DB coûteuse ici : le front peut
    // pré-filtrer la liste des candidats. On force cependant l'user_id à celui
    // de l'authentifié pour éviter le spoofing.
    let mut merged = body;
    if let Some(user) = merged.get_mut("user")
        && let Some(obj) = user.as_object_mut()
    {
        obj.insert("user_id".into(), json!(auth.user_id.to_string()));
    }
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_recommendations(&mut redis, &merged).await?;
    Ok(Json(ApiResponse::new(AiJobEnqueuedResponse { job_id })))
}

/// Poll an AI job result. Returns `status: "ready"` + result when the
/// worker has finished, otherwise `status: "pending"` + job_id.
#[utoipa::path(
    get,
    path = "/api/assistant/jobs/{job_id}",
    tag = "challenges",
    params(("job_id" = String, Path, description = "Opaque job id from an enqueue call")),
    responses(
        (status = 200, description = "Job result or still-pending marker", body = ApiResponse<AiJobResultResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_job_result(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(job_id): Path<String>,
) -> Result<Json<ApiResponse<AiJobResultResponse>>, AppError> {
    let mut redis = state.redis.clone();
    let resp = match crate::services::ai_queue::fetch_result(&mut redis, &job_id).await? {
        Some(result) => AiJobResultResponse {
            status: "ready".to_string(),
            result: Some(result),
            job_id: None,
        },
        None => AiJobResultResponse {
            status: "pending".to_string(),
            result: None,
            job_id: Some(job_id),
        },
    };
    Ok(Json(ApiResponse::new(resp)))
}

/// Admin only: enqueue a hidden-gems talent-mining job. Payload shape
/// is defined by skilluv-ia (talent pool + filters).
#[utoipa::path(
    post,
    path = "/api/admin/assistant/hidden-gems",
    tag = "admin",
    request_body(content = serde_json::Value, description = "Talents pool + filter parameters"),
    responses(
        (status = 200, description = "Job enqueued", body = ApiResponse<AiJobEnqueuedResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_hidden_gems(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<AiJobEnqueuedResponse>>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_hidden_gems(&mut redis, &body).await?;
    Ok(Json(ApiResponse::new(AiJobEnqueuedResponse { job_id })))
}

/// Admin only: enqueue a churn-analysis job.
#[utoipa::path(
    post,
    path = "/api/admin/assistant/churn",
    tag = "admin",
    request_body(content = serde_json::Value, description = "Talents + horizon_days"),
    responses(
        (status = 200, description = "Job enqueued", body = ApiResponse<AiJobEnqueuedResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_churn(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<AiJobEnqueuedResponse>>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let mut redis = state.redis.clone();
    let job_id = crate::services::ai_queue::enqueue_churn_analysis(&mut redis, &body).await?;
    Ok(Json(ApiResponse::new(AiJobEnqueuedResponse { job_id })))
}
