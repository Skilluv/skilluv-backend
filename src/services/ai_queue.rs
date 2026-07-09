//! Enqueue jobs vers skilluv-ia via Redis queues — Phase 5 integration.
//!
//! Format uniforme (aligné sur skilluv-ia/README) :
//! ```json
//! { "job_id": "uuid", "job_type": "...", "payload": {...}, "created_at": "..." }
//! ```
//! Publié sur les queues :
//!   - `skilluv:queue:code_review`
//!   - `skilluv:queue:recommendations`
//!   - `skilluv:queue:analytics_hidden_gems`
//!   - `skilluv:queue:analytics_churn`
//!   - `skilluv:queue:matching` (déjà utilisé pour talent_matcher)
//!   - `skilluv:queue:plagiarism`
//!
//! Le résultat sera lu depuis `skilluv:result:{job_id}` et publié sur le
//! canal pub/sub `skilluv:notifications`.

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::Serialize;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Serialize)]
struct QueueMessage<'a, T: Serialize> {
    job_id: String,
    job_type: &'a str,
    payload: &'a T,
    created_at: String,
    retry_count: u32,
}

async fn enqueue<T: Serialize>(
    redis: &mut ConnectionManager,
    queue: &str,
    job_type: &str,
    payload: &T,
) -> Result<String, AppError> {
    let job_id = Uuid::new_v4().to_string();
    let msg = QueueMessage {
        job_id: job_id.clone(),
        job_type,
        payload,
        created_at: chrono::Utc::now().to_rfc3339(),
        retry_count: 0,
    };
    let json = serde_json::to_string(&msg)
        .map_err(|e| AppError::Internal(format!("ai_queue serialize: {e}")))?;
    let key = format!("skilluv:queue:{queue}");
    let _: () = redis.lpush(&key, &json).await?;
    metrics::counter!(
        "skilluv_ai_jobs_enqueued_total",
        "queue" => queue.to_string()
    )
    .increment(1);
    Ok(job_id)
}

// ─── High-level enqueue helpers ──────────────────────────────────

#[derive(Serialize)]
pub struct CodeReviewPayload<'a> {
    pub submission_id: &'a str,
    pub challenge_id: &'a str,
    pub user_id: &'a str,
    pub language: &'a str,
    pub source_code: &'a str,
    pub challenge_title: &'a str,
    pub challenge_description: &'a str,
    pub difficulty: i32,
    pub test_output: Option<&'a str>,
    pub user_level: &'a str,
}

pub async fn enqueue_code_review(
    redis: &mut ConnectionManager,
    payload: &CodeReviewPayload<'_>,
) -> Result<String, AppError> {
    enqueue(redis, "code_review", "code_review", payload).await
}

pub async fn enqueue_recommendations(
    redis: &mut ConnectionManager,
    payload: &serde_json::Value,
) -> Result<String, AppError> {
    enqueue(redis, "recommendations", "recommendation", payload).await
}

pub async fn enqueue_hidden_gems(
    redis: &mut ConnectionManager,
    payload: &serde_json::Value,
) -> Result<String, AppError> {
    enqueue(
        redis,
        "analytics_hidden_gems",
        "analytics_hidden_gems",
        payload,
    )
    .await
}

pub async fn enqueue_churn_analysis(
    redis: &mut ConnectionManager,
    payload: &serde_json::Value,
) -> Result<String, AppError> {
    enqueue(redis, "analytics_churn", "analytics_churn", payload).await
}

/// Récupère un résultat depuis `skilluv:result:{job_id}` (TTL 24h).
pub async fn fetch_result(
    redis: &mut ConnectionManager,
    job_id: &str,
) -> Result<Option<serde_json::Value>, AppError> {
    let key = format!("skilluv:result:{job_id}");
    let raw: Option<String> = redis.get(&key).await.ok().flatten();
    match raw {
        None => Ok(None),
        Some(s) => Ok(serde_json::from_str(&s).ok()),
    }
}
