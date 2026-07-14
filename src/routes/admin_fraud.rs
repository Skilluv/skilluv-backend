//! P14.5 — Admin fraud dashboard.
//!
//! Endpoints admin pour la modération anti-fraude :
//! - GET /api/admin/fraud/queue : deliverables flaggés + users suspects.
//! - POST /api/admin/fraud/deliverables/{id}/mark-valid : lève le flag.
//! - POST /api/admin/fraud/deliverables/{id}/revoke : marque revoked.
//! - POST /api/admin/fraud/users/{id}/mark-valid : lève le suspected_multi_account.
//! - POST /api/admin/fraud/scan-deliverable/{id} : (re-)lance un scan de similarité.
//! - POST /api/admin/fraud/detect-multi-accounts : lance le job de détection.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{fingerprint, plagiarism};

pub fn admin_fraud_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/fraud/queue", get(fraud_queue))
        .route(
            "/admin/fraud/deliverables/{id}/mark-valid",
            post(mark_deliverable_valid),
        )
        .route("/admin/fraud/deliverables/{id}/revoke", post(revoke_deliverable))
        .route("/admin/fraud/users/{id}/mark-valid", post(mark_user_valid))
        .route(
            "/admin/fraud/scan-deliverable/{id}",
            post(scan_deliverable_endpoint),
        )
        .route(
            "/admin/fraud/detect-multi-accounts",
            post(detect_multi_accounts_endpoint),
        )
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

fn require_admin(auth: &AuthUser) -> Result<(), AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/fraud/queue
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct QueueQuery {
    threshold: Option<String>,
    limit: Option<i64>,
}

async fn fraud_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;

    let threshold: BigDecimal = q
        .threshold
        .as_deref()
        .and_then(|s| BigDecimal::try_from(s.parse::<f64>().ok()?).ok())
        .unwrap_or_else(|| BigDecimal::try_from(0.9f64).unwrap());
    let limit = q.limit.unwrap_or(50);

    let plag = plagiarism::list_flagged(&state.db, threshold, limit).await?;

    let suspects: Vec<(Uuid, chrono::DateTime<chrono::Utc>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, suspected_multi_account_at, suspected_multi_account_reason
        FROM users
        WHERE suspected_multi_account = TRUE
        ORDER BY suspected_multi_account_at DESC NULLS LAST
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "flagged_deliverables": plag.into_iter().map(|(id, score, similar)| json!({
            "deliverable_id": id,
            "plagiarism_score": score,
            "similar_to": similar,
        })).collect::<Vec<_>>(),
        "suspected_users": suspects.into_iter().map(|(id, at, reason)| json!({
            "user_id": id,
            "flagged_at": at,
            "reason": reason,
        })).collect::<Vec<_>>(),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/deliverables/{id}/mark-valid
// ═══════════════════════════════════════════════════════════════════

async fn mark_deliverable_valid(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    let res = sqlx::query(
        "UPDATE deliverables
         SET plagiarism_score = NULL,
             plagiarism_similar_to = NULL,
             plagiarism_scanned_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("deliverable not found".into()));
    }
    Ok(Json(build_response(json!({ "marked_valid": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/deliverables/{id}/revoke
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct RevokeBody {
    #[serde(default)]
    reason: Option<String>,
}

async fn revoke_deliverable(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RevokeBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    let res = sqlx::query(
        "UPDATE deliverables
         SET revoked_at = NOW(),
             revocation_reason = COALESCE($1, 'admin_fraud_revoke')
         WHERE id = $2",
    )
    .bind(body.reason.as_deref())
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("deliverable not found".into()));
    }
    metrics::counter!("skilluv_fraud_deliverables_revoked_total").increment(1);
    Ok(Json(build_response(json!({ "revoked": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/users/{id}/mark-valid
// ═══════════════════════════════════════════════════════════════════

async fn mark_user_valid(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    let res = sqlx::query(
        "UPDATE users
         SET suspected_multi_account = FALSE,
             suspected_multi_account_reason = 'cleared_by_admin'
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("user not found".into()));
    }
    Ok(Json(build_response(json!({ "marked_valid": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/scan-deliverable/{id}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ScanQuery {
    threshold: Option<f32>,
    window_days: Option<i32>,
}

async fn scan_deliverable_endpoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ScanQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    let threshold = q.threshold.unwrap_or(0.9);
    let window = q.window_days.unwrap_or(30);
    let res = plagiarism::scan_deliverable(&state.db, id, threshold, window).await?;
    Ok(Json(build_response(json!({
        "deliverable_id": id,
        "best_match_id": res.best_match_id,
        "best_score": res.best_score,
        "compared_count": res.compared_count,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/detect-multi-accounts
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct DetectBody {
    #[serde(default)]
    window_hours: Option<i32>,
    #[serde(default)]
    min_group_size: Option<i32>,
}

async fn detect_multi_accounts_endpoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DetectBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    let groups = fingerprint::detect_multi_accounts(
        &state.db,
        body.window_hours.unwrap_or(24),
        body.min_group_size.unwrap_or(3),
    )
    .await?;
    let total_users: usize = groups.iter().map(|g| g.user_ids.len()).sum();
    Ok(Json(build_response(json!({
        "groups_detected": groups.len(),
        "users_flagged": total_users,
        "groups": groups,
    }))))
}
