//! Accusing somebody of copying, and letting them answer.
//!
//! Four surfaces, and the middle one is the reason the other three exist: the
//! accused gets to reply before anybody decides.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::plagiarism_cases::{self, Case, FlagInput};

pub fn plagiarism_routes() -> Router<AppState> {
    Router::new()
        .route("/contests/submissions/{id}/flag", post(flag))
        .route("/contests/plagiarism/{id}", get(read))
        .route("/contests/plagiarism/{id}/respond", post(respond))
}

pub fn admin_plagiarism_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/plagiarism", get(queue))
        .route("/admin/plagiarism/{id}/decide", post(decide))
}

/// Somebody allowed to decide an accusation.
///
/// `plagiarism_reviewer` already exists (P25) and is exactly this job. An
/// admin too, because somebody has to be able to work the queue on a Sunday.
async fn require_reviewer(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["admin", "plagiarism_reviewer"],
    )
    .await
}

/// Raise a case against a contest entry.
///
/// Open to any authenticated member, not only to jurors: plagiarism is
/// usually spotted by the one person who recognises the original, and that is
/// rarely whoever happens to be judging.
#[utoipa::path(
    post, path = "/api/contests/submissions/{id}/flag", tag = "tournament",
    params(("id" = Uuid, Path, description = "Submission id")),
    request_body = FlagInput,
    responses(
        (status = 201, description = "Case opened", body = ApiResponse<Case>),
        (status = 400, description = "Too short, or no evidence link", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such submission", body = crate::api_response::ErrorResponse),
        (status = 409, description = "A case is already open on this entry", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn flag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(submission_id): Path<Uuid>,
    Json(input): Json<FlagInput>,
) -> Result<(axum::http::StatusCode, Json<ApiResponse<Case>>), AppError> {
    let case = plagiarism_cases::flag(&state.db, submission_id, auth.user_id, input).await?;

    // The accused is told, with the accusation in full. Being disqualified by
    // a process nobody told you about is the failure this whole table exists
    // to prevent.
    if let Err(err) = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(&state.db),
        crate::services::notify::Recipient::User(
            sqlx::query_scalar::<_, Uuid>("SELECT accused_id FROM plagiarism_cases WHERE id = $1")
                .bind(case.id)
                .fetch_one(&state.db)
                .await?,
        ),
        "moderation.plagiarism_case_opened",
    )
    .payload(serde_json::json!({
        "case_id": case.id,
        "respond_by": case.respond_by,
    }))
    .execute()
    .await
    {
        tracing::warn!(%err, case = %case.id, "the accused was not notified");
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ApiResponse::new(case)),
    ))
}

/// Read a case.
///
/// The accused and the reviewers, nobody else. An open accusation is not
/// public: it is an allegation, and publishing allegations before they are
/// decided is how a dismissed case still ruins somebody.
#[utoipa::path(
    get, path = "/api/contests/plagiarism/{id}", tag = "tournament",
    params(("id" = Uuid, Path, description = "Case id")),
    responses(
        (status = 200, body = ApiResponse<Case>),
        (status = 403, description = "Not the accused, and not a reviewer", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such case", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Case>>, AppError> {
    let accused: Option<Uuid> =
        sqlx::query_scalar("SELECT accused_id FROM plagiarism_cases WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let accused = accused.ok_or_else(|| AppError::NotFound("no such case".into()))?;

    if accused != auth.user_id {
        require_reviewer(&state, &auth).await?;
    }

    Ok(Json(ApiResponse::new(
        plagiarism_cases::by_id(&state.db, id).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RespondBody {
    pub response_md: String,
}

/// The accused answers.
#[utoipa::path(
    post, path = "/api/contests/plagiarism/{id}/respond", tag = "tournament",
    params(("id" = Uuid, Path, description = "Case id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = ApiResponse<Case>),
        (status = 400, description = "Empty answer", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not the accused", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already decided", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<ApiResponse<Case>>, AppError> {
    let case = plagiarism_cases::respond(&state.db, id, auth.user_id, &body.response_md).await?;
    Ok(Json(ApiResponse::new(case)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct QueueQuery {
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 200)]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// The open cases, oldest first.
#[utoipa::path(
    get, path = "/api/admin/plagiarism",
    operation_id = "plagiarismQueue",
    tag = "admin",
    params(QueueQuery),
    responses(
        (status = 200, body = ApiResponse<Vec<Case>>),
        (status = 403, description = "Not a reviewer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<Json<ApiResponse<Vec<Case>>>, AppError> {
    require_reviewer(&state, &auth).await?;
    if !(1..=200).contains(&q.limit) {
        return Err(AppError::Validation(
            "limit must be between 1 and 200".into(),
        ));
    }
    Ok(Json(ApiResponse::new(
        plagiarism_cases::open_cases(&state.db, q.limit).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DecideBody {
    /// True disqualifies the entry. False clears it.
    pub upheld: bool,
    /// At least eighty characters, whichever way it goes. An accusation
    /// dropped without a word leaves the accusation standing in everybody's
    /// memory.
    pub decision_md: String,
}

/// Decide a case.
#[utoipa::path(
    post, path = "/api/admin/plagiarism/{id}/decide", tag = "admin",
    params(("id" = Uuid, Path, description = "Case id")),
    request_body = DecideBody,
    responses(
        (status = 200, body = ApiResponse<Case>),
        (status = 400, description = "Decision too short", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not a reviewer", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already decided", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decide(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideBody>,
) -> Result<Json<ApiResponse<Case>>, AppError> {
    require_reviewer(&state, &auth).await?;

    let case =
        plagiarism_cases::decide(&state.db, id, auth.user_id, body.upheld, &body.decision_md)
            .await?;

    // Told either way. Being cleared matters as much as being disqualified,
    // and somebody who was accused and heard nothing assumes the worst.
    let accused: Uuid = sqlx::query_scalar("SELECT accused_id FROM plagiarism_cases WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    if let Err(err) = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(&state.db),
        crate::services::notify::Recipient::User(accused),
        "moderation.plagiarism_case_decided",
    )
    .payload(serde_json::json!({
        "case_id": id,
        "upheld": body.upheld,
    }))
    .execute()
    .await
    {
        tracing::warn!(%err, case = %id, "the accused was not told the outcome");
    }

    Ok(Json(ApiResponse::new(case)))
}
