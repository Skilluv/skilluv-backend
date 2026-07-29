//! Routes HTTP pour la file d'attente de review humaine (Phase P2.2).
//!
//! Endpoints publics (auth requis) :
//!   GET   /api/review-queue                   — liste des tasks open éligibles
//!   POST  /api/review-queue/{task_id}/claim   — claim une task (soft-lock 2h)
//!   POST  /api/deliverables/{id}/reviews      — soumet un verdict
//!
//! **Cold start (12 premiers mois)** : ces endpoints devraient être restreints
//! aux rôles admin/steward. Cette itération P2.2 les rend accessibles à tous
//! les users authentifiés — restriction à ajouter en Phase P3 quand la
//! réputation reviewer commence à se construire (voir H.2 cold start policy).

use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::review_queue::ReviewTask;
use crate::services::reviews::SubmitOutcome;
use crate::services::{
    ReviewQueueFilter, ReviewQueueService, ReviewSubmitParams, ReviewsService, SeniorityLevel,
    Verdict,
};

pub fn review_queue_routes() -> Router<AppState> {
    Router::new()
        .route("/review-queue", get(list_open))
        .route("/review-queue/{id}", get(get_task))
        .route("/review-queue/{id}/claim", post(claim_task))
        .route("/deliverables/{id}/reviews", post(submit_review))
        .route("/deliverables/{id}/reviews", get(list_reviews))
}

// ═══════════════════════════════════════════════════════════════════
// Types de réponse
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
pub struct QueueQuery {
    pub domain: Option<String>,
    /// `any` (default), `contribs`, `impact`.
    pub seniority: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TasksListResponse {
    pub tasks: Vec<ReviewTask>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TaskResponse {
    pub task: ReviewTask,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClaimResponse {
    pub task: ReviewTask,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitReviewBody {
    /// `approve`, `request_changes`, `reject`, `abstain`.
    pub verdict: String,
    pub body: String,
    pub time_spent_seconds: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmitReviewResponse {
    pub outcome: SubmitOutcome,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReviewRow {
    pub id: Uuid,
    pub reviewer_user_id: Uuid,
    pub verdict: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReviewsListResponse {
    pub reviews: Vec<ReviewRow>,
}

// ═══════════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════════

/// Paginated open review tasks. Optional filters on domain and
/// required seniority.
#[utoipa::path(
    get,
    path = "/api/review-queue",
    tag = "moderation",
    params(QueueQuery),
    responses(
        (status = 200, description = "Open tasks", body = ApiResponse<TasksListResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_open(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<Json<ApiResponse<TasksListResponse>>, AppError> {
    let max_seniority = match q.seniority.as_deref() {
        Some("impact") => SeniorityLevel::Impact,
        Some("contribs") => SeniorityLevel::Contribs,
        _ => SeniorityLevel::Any,
    };

    let filter = ReviewQueueFilter {
        primary_domain: q.domain,
        max_seniority,
        page: q.page.unwrap_or(1),
        per_page: q.per_page.unwrap_or(20),
    };

    let tasks = ReviewQueueService::list_open(&state.db, &filter).await?;
    Ok(Json(ApiResponse::new(TasksListResponse { tasks })))
}

/// Fetch a specific review task by id.
#[utoipa::path(
    get,
    path = "/api/review-queue/{id}",
    tag = "moderation",
    params(("id" = Uuid, Path, description = "Review-task UUID")),
    responses(
        (status = 200, description = "Task detail", body = ApiResponse<TaskResponse>),
        (status = 404, description = "Task not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_task(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let task = ReviewQueueService::get(&state.db, id).await?;
    Ok(Json(ApiResponse::new(TaskResponse { task })))
}

/// Claim an open review task (2-hour soft-lock). Idempotent when the
/// caller already holds the claim.
#[utoipa::path(
    post,
    path = "/api/review-queue/{id}/claim",
    tag = "moderation",
    params(("id" = Uuid, Path, description = "Review-task UUID")),
    responses(
        (status = 200, description = "Claimed", body = ApiResponse<ClaimResponse>),
        (status = 400, description = "Task already claimed by someone else", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Task not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn claim_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<ClaimResponse>>, AppError> {
    let task = ReviewQueueService::claim(&state.db, id, auth.user_id).await?;
    Ok(Json(ApiResponse::new(ClaimResponse {
        task,
        message: "Task claimed. You have 2 hours to submit your verdict.".to_string(),
    })))
}

/// Submit a review verdict on a deliverable. Verdict values:
/// `approve`, `request_changes`, `reject`, `abstain`.
#[utoipa::path(
    post,
    path = "/api/deliverables/{id}/reviews",
    tag = "moderation",
    params(("id" = Uuid, Path, description = "Deliverable UUID")),
    request_body = SubmitReviewBody,
    responses(
        (status = 200, description = "Review recorded", body = ApiResponse<SubmitReviewResponse>),
        (status = 400, description = "Invalid verdict or empty body", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(deliverable_id): Path<Uuid>,
    Json(payload): Json<SubmitReviewBody>,
) -> Result<Json<ApiResponse<SubmitReviewResponse>>, AppError> {
    let verdict = Verdict::from_str(&payload.verdict).map_err(|_| {
        AppError::Validation(format!(
            "invalid verdict '{}'; expected approve|request_changes|reject|abstain",
            payload.verdict
        ))
    })?;

    if payload.body.trim().is_empty() {
        return Err(AppError::Validation(
            "review body cannot be empty".to_string(),
        ));
    }

    let params = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: auth.user_id,
        verdict,
        body: payload.body,
        time_spent_seconds: payload.time_spent_seconds,
    };

    let outcome = ReviewsService::submit_verdict(&state.db, params).await?;
    Ok(Json(ApiResponse::new(SubmitReviewResponse { outcome })))
}

/// List every submitted review for a deliverable, ordered oldest first.
#[utoipa::path(
    get,
    path = "/api/deliverables/{id}/reviews",
    tag = "moderation",
    params(("id" = Uuid, Path, description = "Deliverable UUID")),
    responses(
        (status = 200, description = "Reviews", body = ApiResponse<ReviewsListResponse>),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_reviews(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(deliverable_id): Path<Uuid>,
) -> Result<Json<ApiResponse<ReviewsListResponse>>, AppError> {
    let reviews: Vec<(Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
            SELECT id, reviewer_user_id, verdict, body, created_at
            FROM reviews
            WHERE deliverable_id = $1
            ORDER BY created_at ASC
            "#,
    )
    .bind(deliverable_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<ReviewRow> = reviews
        .into_iter()
        .map(|(id, reviewer_id, verdict, body, created_at)| ReviewRow {
            id,
            reviewer_user_id: reviewer_id,
            verdict,
            body,
            created_at,
        })
        .collect();

    Ok(Json(ApiResponse::new(ReviewsListResponse {
        reviews: items,
    })))
}
