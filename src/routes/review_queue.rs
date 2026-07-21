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

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
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
// GET /api/review-queue
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct QueueQuery {
    domain: Option<String>,
    seniority: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_open(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({ "tasks": tasks }))))
}

async fn get_task(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let task = ReviewQueueService::get(&state.db, id).await?;
    Ok(Json(build_response(json!({ "task": task }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/review-queue/{id}/claim
// ═══════════════════════════════════════════════════════════════════

async fn claim_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let task = ReviewQueueService::claim(&state.db, id, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "task": task,
        "message": "Task claimed. You have 2 hours to submit your verdict."
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/deliverables/{id}/reviews
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct SubmitReviewBody {
    verdict: String,
    body: String,
    time_spent_seconds: Option<i32>,
}

async fn submit_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(deliverable_id): Path<Uuid>,
    Json(payload): Json<SubmitReviewBody>,
) -> Result<Json<Value>, AppError> {
    let verdict = Verdict::from_str(&payload.verdict).ok_or_else(|| {
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
    Ok(Json(build_response(json!({ "outcome": outcome }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/deliverables/{id}/reviews
// ═══════════════════════════════════════════════════════════════════

async fn list_reviews(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(deliverable_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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

    let items: Vec<Value> = reviews
        .into_iter()
        .map(|(id, reviewer_id, verdict, body, created_at)| {
            json!({
                "id": id,
                "reviewer_user_id": reviewer_id,
                "verdict": verdict,
                "body": body,
                "created_at": created_at,
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "reviews": items }))))
}
