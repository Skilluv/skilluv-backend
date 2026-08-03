use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::{ApiResponse, SimpleMessage};
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn report_routes() -> Router<AppState> {
    Router::new()
        .route("/reports", post(create_report))
        .route("/reports/mine", get(my_reports))
        .route("/reports/{id}", delete(cancel_report))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReportRequest {
    /// One of `user`, `challenge`, `message`, `enterprise`.
    #[schema(pattern = r"^(user|challenge|message|enterprise)$", example = "user")]
    pub target_type: String,
    pub target_id: Uuid,
    /// One of `spam`, `harassment`, `inappropriate`, `cheating`,
    /// `fake_profile`, `other`.
    #[schema(
        pattern = r"^(spam|harassment|inappropriate|cheating|fake_profile|other)$",
        example = "harassment"
    )]
    pub reason: String,
    /// Free-text details, up to 2000 chars.
    #[schema(max_length = 2000)]
    pub details: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Report {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    /// `pending`, `handled`, `dismissed`.
    pub status: String,
    pub admin_note: Option<String>,
    pub handled_by: Option<Uuid>,
    pub handled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateReportResponse {
    pub report: Report,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyReportsResponse {
    pub reports: Vec<Report>,
}

/// Submit a moderation report. Deduplicated on `(reporter_id,
/// target_type, target_id)` while status is `pending` so a user can't
/// flood the queue with duplicates.
#[utoipa::path(
    post,
    path = "/api/reports",
    tag = "moderation",
    request_body = CreateReportRequest,
    responses(
        (status = 201, description = "Report created", body = ApiResponse<CreateReportResponse>),
        (status = 400, description = "Invalid target_type / reason / self-report / details too long / duplicate pending", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateReportRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate target_type
    let valid_types = ["user", "challenge", "message", "enterprise"];
    if !valid_types.contains(&body.target_type.as_str()) {
        return Err(AppError::Validation(format!(
            "target_type must be one of: {}",
            valid_types.join(", ")
        )));
    }

    // Validate reason
    let valid_reasons = [
        "spam",
        "harassment",
        "inappropriate",
        "cheating",
        "fake_profile",
        "other",
    ];
    if !valid_reasons.contains(&body.reason.as_str()) {
        return Err(AppError::Validation(format!(
            "reason must be one of: {}",
            valid_reasons.join(", ")
        )));
    }

    // Can't report yourself
    if body.target_type == "user" && body.target_id == auth.user_id {
        return Err(AppError::Validation(
            "You cannot report yourself".to_string(),
        ));
    }

    if let Some(ref details) = body.details
        && details.len() > 2000
    {
        return Err(AppError::Validation(
            "Details must be at most 2000 characters".to_string(),
        ));
    }

    let report: Report = sqlx::query_as(
        r#"
        INSERT INTO reports (reporter_id, target_type, target_id, reason, details)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(auth.user_id)
    .bind(&body.target_type)
    .bind(body.target_id)
    .bind(&body.reason)
    .bind(&body.details)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.constraint() == Some("idx_reports_unique_pending")
        {
            return AppError::Validation(
                "You already have a pending report for this target".to_string(),
            );
        }
        AppError::Database(e)
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(CreateReportResponse {
            report,
            message: "Report submitted".to_string(),
        })),
    ))
}

/// List every report the caller has ever filed. Ordered newest first.
#[utoipa::path(
    get,
    path = "/api/reports/mine",
    tag = "moderation",
    responses(
        (status = 200, description = "The caller's reports", body = ApiResponse<MyReportsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyReportsResponse>>, AppError> {
    let reports: Vec<Report> =
        sqlx::query_as("SELECT * FROM reports WHERE reporter_id = $1 ORDER BY created_at DESC")
            .bind(auth.user_id)
            .fetch_all(&state.db)
            .await?;

    Ok(Json(ApiResponse::new(MyReportsResponse { reports })))
}

/// Cancel a report that hasn't been handled yet. No-op with 404 if
/// the report is already processed or belongs to another user.
#[utoipa::path(
    delete,
    path = "/api/reports/{id}",
    tag = "moderation",
    params(("id" = Uuid, Path, description = "Report UUID")),
    responses(
        (status = 200, description = "Report cancelled", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Report not found or already processed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn cancel_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let result = sqlx::query(
        "DELETE FROM reports WHERE id = $1 AND reporter_id = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Report not found or already processed".to_string(),
        ));
    }

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Report cancelled",
    ))))
}
