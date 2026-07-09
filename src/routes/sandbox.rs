use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};
use crate::services::sandbox::{self, ExecutionResult};

pub fn sandbox_routes() -> Router<AppState> {
    Router::new()
        .route("/sandbox/execute", post(execute))
        .route("/sandbox/execute-async", post(execute_async))
        .route("/sandbox/result/{token}", get(get_result))
        .route("/sandbox/languages", get(list_languages))
}

#[derive(Debug, Deserialize)]
struct ExecuteRequest {
    source_code: String,
    language: String,
    stdin: Option<String>,
    expected_output: Option<String>,
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

/// Judge0 status IDs:
/// 1 = In Queue, 2 = Processing, 3 = Accepted,
/// 4 = Wrong Answer, 5 = Time Limit Exceeded,
/// 6 = Compilation Error, 7-12 = Runtime errors, 13 = Internal Error
fn classify_result(result: &ExecutionResult) -> (&'static str, bool) {
    match result.status.id {
        3 => ("accepted", true),
        4 => ("wrong_answer", false),
        5 => ("time_limit_exceeded", false),
        6 => ("compilation_error", false),
        7..=12 => ("runtime_error", false),
        13 => ("internal_error", false),
        1 | 2 => ("processing", false),
        _ => ("unknown", false),
    }
}

// POST /api/sandbox/execute — synchronous execution
async fn execute(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Rate limit: 20 executions per minute per user
    RateLimiter::check(
        &mut state.redis.clone(),
        "sandbox",
        &auth.user_id.to_string(),
        20,
        60,
    )
    .await?;

    if body.source_code.is_empty() {
        return Err(AppError::Validation(
            "source_code cannot be empty".to_string(),
        ));
    }

    if body.source_code.len() > 100_000 {
        return Err(AppError::Validation(
            "source_code exceeds maximum size (100KB)".to_string(),
        ));
    }

    let result = state
        .sandbox
        .execute(
            &body.source_code,
            &body.language,
            body.stdin.as_deref(),
            body.expected_output.as_deref(),
            None,
            None,
        )
        .await?;

    let (verdict, success) = classify_result(&result);

    Ok(Json(build_response(json!({
        "execution": result,
        "verdict": verdict,
        "success": success,
    }))))
}

// POST /api/sandbox/execute-async — async execution (returns token)
async fn execute_async(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if body.source_code.is_empty() {
        return Err(AppError::Validation(
            "source_code cannot be empty".to_string(),
        ));
    }

    let token = state
        .sandbox
        .execute_async(&body.source_code, &body.language, body.stdin.as_deref())
        .await?;

    Ok(Json(build_response(json!({
        "token": token,
        "message": "Submission queued. Poll /sandbox/result/{token} for results."
    }))))
}

// GET /api/sandbox/result/:token — poll result
async fn get_result(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = state.sandbox.get_result(&token).await?;
    let (verdict, success) = classify_result(&result);

    let processing = result.status.id <= 2;

    Ok(Json(build_response(json!({
        "execution": result,
        "verdict": verdict,
        "success": success,
        "processing": processing,
    }))))
}

// GET /api/sandbox/languages — list supported languages
async fn list_languages(_auth: AuthUser) -> Json<serde_json::Value> {
    let languages = sandbox::supported_languages();
    let tier1: Vec<_> = languages.iter().filter(|l| l.tier == 1).collect();
    let tier2: Vec<_> = languages.iter().filter(|l| l.tier == 2).collect();

    Json(build_response(json!({
        "tier1": tier1,
        "tier2": tier2,
        "total": languages.len(),
    })))
}
