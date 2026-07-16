//! IA-D — Helper best-effort pour logger les appels gRPC vers skilluv-ai.
//!
//! Usage typique dans un handler :
//! ```ignore
//! let started = std::time::Instant::now();
//! let result = ai.review_code(...).await;
//! ai_log::record(
//!     &state.db,
//!     "ReviewCode",
//!     Some(submission_id),
//!     Some(auth.user_id),
//!     started.elapsed(),
//!     &result,
//! ).await;
//! ```
//!
//! Le helper est **best-effort** — si l'insert échoue (DB down), on log via
//! `tracing::warn!` mais on ne propage jamais l'erreur au caller.

use std::time::Duration;
use sqlx::PgPool;
use uuid::Uuid;

/// Record un appel gRPC dans `ai_call_log`. Best-effort.
///
/// - `method` : ex `"ReviewCode"`, `"AnalyzePerformance"`.
/// - `submission_id` / `user_id` : contexte lié (optionnels).
/// - `latency` : durée totale de l'appel (côté client Rust).
/// - `result` : le `Result<T, tonic::Status>` retourné par `AiClient`. On extrait
///   le status + error message + tente d'extraire `model_version` si `T` l'expose.
pub async fn record<T>(
    db: &PgPool,
    method: &str,
    submission_id: Option<Uuid>,
    user_id: Option<Uuid>,
    latency: Duration,
    result: &Result<T, tonic::Status>,
    model_version: Option<&str>,
) {
    let latency_ms = latency.as_millis().min(i32::MAX as u128) as i32;
    let (status, error_message) = match result {
        Ok(_) => ("ok", None),
        Err(s) => {
            let code_str = match s.code() {
                tonic::Code::Unavailable => "unavailable",
                tonic::Code::Internal => "internal",
                tonic::Code::DeadlineExceeded => "timeout",
                _ => "internal",
            };
            (code_str, Some(s.message().to_string()))
        }
    };

    let insert = sqlx::query(
        r#"
        INSERT INTO ai_call_log
            (method, submission_id, user_id, latency_ms, status, model_version, error_message)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(method)
    .bind(submission_id)
    .bind(user_id)
    .bind(latency_ms)
    .bind(status)
    .bind(model_version)
    .bind(error_message)
    .execute(db)
    .await;

    if let Err(e) = insert {
        tracing::warn!(
            method,
            error = %e,
            "IA-D ai_call_log insert failed (best-effort, no propagation)"
        );
    }

    // Metric aussi (double-track).
    metrics::counter!(
        "skilluv_ai_calls_total",
        "method" => method.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "skilluv_ai_call_latency_ms",
        "method" => method.to_string(),
    )
    .record(latency_ms as f64);
}
