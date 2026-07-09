use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn report_routes() -> Router<AppState> {
    Router::new()
        .route("/reports", post(create_report))
        .route("/reports/mine", get(my_reports))
        .route("/reports/{id}", delete(cancel_report))
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

#[derive(Debug, Deserialize)]
struct CreateReportRequest {
    target_type: String,
    target_id: Uuid,
    reason: String,
    details: Option<String>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct Report {
    id: Uuid,
    reporter_id: Uuid,
    target_type: String,
    target_id: Uuid,
    reason: String,
    details: Option<String>,
    status: String,
    admin_note: Option<String>,
    handled_by: Option<Uuid>,
    handled_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

// POST /api/reports
async fn create_report(
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

    if let Some(ref details) = body.details {
        if details.len() > 2000 {
            return Err(AppError::Validation(
                "Details must be at most 2000 characters".to_string(),
            ));
        }
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
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("idx_reports_unique_pending") {
                return AppError::Validation(
                    "You already have a pending report for this target".to_string(),
                );
            }
        }
        AppError::Database(e)
    })?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "report": report,
            "message": "Report submitted"
        }))),
    ))
}

// GET /api/reports/mine
async fn my_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let reports: Vec<Report> =
        sqlx::query_as("SELECT * FROM reports WHERE reporter_id = $1 ORDER BY created_at DESC")
            .bind(auth.user_id)
            .fetch_all(&state.db)
            .await?;

    Ok(Json(build_response(json!({ "reports": reports }))))
}

// DELETE /api/reports/:id — cancel a pending report
async fn cancel_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
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

    Ok(Json(build_response(json!({
        "message": "Report cancelled"
    }))))
}
