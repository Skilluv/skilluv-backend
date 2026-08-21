use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};
use crate::services::sandbox::{self, ExecutionResult, LanguageInfo};

pub fn sandbox_routes() -> Router<AppState> {
    Router::new()
        .route("/sandbox/execute", post(execute))
        .route("/sandbox/execute-async", post(execute_async))
        .route("/sandbox/result/{token}", get(get_result))
        .route("/sandbox/languages", get(list_languages))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExecuteRequest {
    /// Source code. Cap: 100 KB.
    #[schema(max_length = 10000)]
    pub source_code: String,
    /// Judge0 language slug (`rust`, `python`, `cpp`, …).
    #[schema(max_length = 10000)]
    pub language: String,
    #[schema(max_length = 10000)]
    pub stdin: Option<String>,
    /// If provided, Judge0 compares stdout to this and returns status 3
    /// (Accepted) or 4 (Wrong Answer).
    #[schema(max_length = 10000)]
    pub expected_output: Option<String>,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct ExecuteResponse {
    pub execution: ExecutionResult,
    /// One of `accepted`, `wrong_answer`, `time_limit_exceeded`,
    /// `compilation_error`, `runtime_error`, `internal_error`,
    /// `processing`, `unknown`.
    pub verdict: String,
    pub success: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AsyncExecuteResponse {
    pub token: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AsyncResultResponse {
    pub execution: ExecutionResult,
    pub verdict: String,
    pub success: bool,
    /// True while Judge0 still has the submission (status id 1 or 2).
    pub processing: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LanguagesResponse {
    /// Fully-supported languages (docs, tests, examples ready).
    pub tier1: Vec<LanguageInfo>,
    /// Judge0-supported but not officially curated.
    pub tier2: Vec<LanguageInfo>,
    pub total: usize,
}

/// Execute code synchronously via Judge0. Rate-limited to 20 exec/min
/// per user. Source cap: 100 KB.
#[utoipa::path(
    post,
    path = "/api/sandbox/execute",
    tag = "challenges",
    request_body = ExecuteRequest,
    responses(
        (status = 200, description = "Execution finished", body = ApiResponse<ExecuteResponse>),
        (status = 400, description = "Empty source or over 100 KB", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Rate limit hit", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn execute(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<ApiResponse<ExecuteResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(ExecuteResponse {
        execution: result,
        verdict: verdict.to_string(),
        success,
    })))
}

/// Enqueue a Judge0 submission and return a token. Poll
/// `/sandbox/result/{token}` for the result. No result-fetching
/// rate-limit — just the submission.
#[utoipa::path(
    post,
    path = "/api/sandbox/execute-async",
    tag = "challenges",
    request_body = ExecuteRequest,
    responses(
        (status = 200, description = "Submission queued", body = ApiResponse<AsyncExecuteResponse>),
        (status = 400, description = "Empty source", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn execute_async(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<ApiResponse<AsyncExecuteResponse>>, AppError> {
    if body.source_code.is_empty() {
        return Err(AppError::Validation(
            "source_code cannot be empty".to_string(),
        ));
    }

    let token = state
        .sandbox
        .execute_async(&body.source_code, &body.language, body.stdin.as_deref())
        .await?;

    Ok(Json(ApiResponse::new(AsyncExecuteResponse {
        token,
        message: "Submission queued. Poll /sandbox/result/{token} for results.".to_string(),
    })))
}

/// Fetch the result of an async submission. `processing: true` when
/// Judge0 still has it queued or running.
#[utoipa::path(
    get,
    path = "/api/sandbox/result/{token}",
    tag = "challenges",
    params(("token" = String, Path, description = "Judge0 submission token")),
    responses(
        (status = 200, description = "Current execution state", body = ApiResponse<AsyncResultResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_result(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(token): Path<String>,
) -> Result<Json<ApiResponse<AsyncResultResponse>>, AppError> {
    let result = state.sandbox.get_result(&token).await?;
    let (verdict, success) = classify_result(&result);

    let processing = result.status.id <= 2;

    Ok(Json(ApiResponse::new(AsyncResultResponse {
        execution: result,
        verdict: verdict.to_string(),
        success,
        processing,
    })))
}

/// List Judge0-supported languages the sandbox exposes, split by tier
/// (1 = officially curated, 2 = available but unpolished).
#[utoipa::path(
    get,
    path = "/api/sandbox/languages",
    tag = "challenges",
    responses(
        (status = 200, description = "Supported languages", body = ApiResponse<LanguagesResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "sandboxListLanguages",
)]
pub async fn list_languages(_auth: AuthUser) -> Json<ApiResponse<LanguagesResponse>> {
    let languages = sandbox::supported_languages();
    let tier1: Vec<LanguageInfo> = languages.iter().filter(|l| l.tier == 1).cloned().collect();
    let tier2: Vec<LanguageInfo> = languages.iter().filter(|l| l.tier == 2).cloned().collect();
    let total = languages.len();

    Json(ApiResponse::new(LanguagesResponse {
        tier1,
        tier2,
        total,
    }))
}
