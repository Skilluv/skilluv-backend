//! Enterprise KYC — Phase 4.5.
//!
//! Threshold-based gating:
//!   - Level `none`  : ≤ 100 €/month spend (equivalent)
//!   - Level `basic` : ≤ 2 000 €/month
//!   - Level `full`  : > 2 000 €/month
//!
//! When an enterprise exceeds the current level's threshold, the next Stripe/PSP
//! checkout is refused with a 402-style error pointing to the upload flow.

use axum::extract::{Multipart, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub const KYC_BASIC_THRESHOLD_CENTS: i64 = 10_000; // 100 €/mo
pub const KYC_FULL_THRESHOLD_CENTS: i64 = 200_000; // 2 000 €/mo
pub const KYC_DOC_MAX_SIZE: usize = 10 * 1024 * 1024; // 10 MB per file
pub const KYC_ALLOWED_MIME: &[&str] = &["application/pdf", "image/jpeg", "image/png", "image/webp"];

pub fn enterprise_kyc_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/kyc", get(get_status))
        .route("/enterprise/kyc/documents", post(upload_document))
        .route("/admin/enterprise-kyc", get(admin_list))
        .route(
            "/admin/enterprise-kyc/{enterprise_id}/decide",
            post(admin_decide),
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

async fn current_enterprise_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

async fn get_status(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let row: (
        String,
        String,
        i64,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    ) = sqlx::query_as(
        r#"
            INSERT INTO enterprise_kyc (enterprise_id) VALUES ($1)
            ON CONFLICT (enterprise_id) DO UPDATE SET enterprise_id = enterprise_kyc.enterprise_id
            RETURNING level, status, monthly_spend_eur_cents, reviewed_at, rejection_reason
            "#,
    )
    .bind(enterprise_id)
    .fetch_one(&state.db)
    .await?;
    let docs: Vec<(Uuid, String, String, i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT id, kind, content_type, size_bytes, uploaded_at
        FROM enterprise_kyc_documents WHERE enterprise_id = $1 AND deleted_at IS NULL
        ORDER BY uploaded_at DESC
        "#,
    )
    .bind(enterprise_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<Value> = docs
        .into_iter()
        .map(|(id, kind, ct, sz, at)| {
            json!({
                "id": id,
                "kind": kind,
                "content_type": ct,
                "size_bytes": sz,
                "uploaded_at": at,
            })
        })
        .collect();
    Ok(Json(build_response(json!({
        "level": row.0,
        "status": row.1,
        "monthly_spend_eur_cents": row.2,
        "reviewed_at": row.3,
        "rejection_reason": row.4,
        "documents": items,
        "thresholds": {
            "basic_up_to_eur_cents": crate::routes::enterprise_kyc::KYC_BASIC_THRESHOLD_CENTS,
            "full_required_above_eur_cents": crate::routes::enterprise_kyc::KYC_FULL_THRESHOLD_CENTS,
        }
    }))))
}

async fn upload_document(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let mut kind: Option<String> = None;
    let mut data: Option<(Vec<u8>, String)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("multipart parse: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "kind" {
            kind = Some(
                field
                    .text()
                    .await
                    .map_err(|e| AppError::Validation(format!("multipart text: {e}")))?,
            );
        } else if name == "file" {
            let ct = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            if !KYC_ALLOWED_MIME.contains(&ct.as_str()) {
                return Err(AppError::Validation(format!(
                    "unsupported content type '{ct}'; allowed: {}",
                    KYC_ALLOWED_MIME.join(", ")
                )));
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::Validation(format!("multipart bytes: {e}")))?;
            if bytes.len() > KYC_DOC_MAX_SIZE {
                return Err(AppError::Validation(format!(
                    "file too large — max {} bytes",
                    KYC_DOC_MAX_SIZE
                )));
            }
            data = Some((bytes.to_vec(), ct));
        }
    }
    let kind = kind.ok_or(AppError::Validation("missing 'kind' field".into()))?;
    let (bytes, content_type) = data.ok_or(AppError::Validation("missing 'file' field".into()))?;
    let doc_id = Uuid::new_v4();
    let ext = match content_type.as_str() {
        "application/pdf" => "pdf",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "bin",
    };
    let storage_key = format!("kyc/{enterprise_id}/{doc_id}.{ext}");
    state
        .storage
        .upload_generic(&storage_key, &bytes, &content_type)
        .await?;
    sqlx::query(
        r#"
        INSERT INTO enterprise_kyc_documents (id, enterprise_id, kind, storage_key, content_type, size_bytes, uploaded_by_user_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(doc_id)
    .bind(enterprise_id)
    .bind(&kind)
    .bind(&storage_key)
    .bind(&content_type)
    .bind(bytes.len() as i64)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    // Move status to pending automatically.
    sqlx::query(
        "UPDATE enterprise_kyc SET status = 'pending', updated_at = NOW() WHERE enterprise_id = $1",
    )
    .bind(enterprise_id)
    .execute(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "document_id": doc_id }))))
}

async fn admin_list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT k.enterprise_id, e.company_name, k.level, k.status, k.monthly_spend_eur_cents, k.updated_at,
               (SELECT COUNT(*) FROM enterprise_kyc_documents d WHERE d.enterprise_id = k.enterprise_id AND d.deleted_at IS NULL)::BIGINT AS docs
        FROM enterprise_kyc k
        JOIN enterprises e ON e.id = k.enterprise_id
        WHERE k.status = 'pending'
        ORDER BY k.updated_at ASC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "enterprise_id": r.get::<Uuid, _>("enterprise_id"),
                "company_name": r.get::<String, _>("company_name"),
                "level": r.get::<String, _>("level"),
                "status": r.get::<String, _>("status"),
                "monthly_spend_eur_cents": r.get::<i64, _>("monthly_spend_eur_cents"),
                "documents_count": r.get::<i64, _>("docs"),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "queue": items }))))
}

#[derive(Deserialize)]
struct DecideBody {
    action: String,         // "approve" | "reject"
    level: Option<String>,  // "basic" | "full" when approving
    reason: Option<String>, // rejection reason
}

async fn admin_decide(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
    Json(body): Json<DecideBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    // BE-F : on clone les champs `level`/`reason` avant `match` pour pouvoir
    // les reprendre dans l'audit log après la mutation.
    let audit_level = body.level.clone();
    let audit_reason = body.reason.clone();
    match body.action.as_str() {
        "approve" => {
            let level = body.level.unwrap_or_else(|| "basic".into());
            if !matches!(level.as_str(), "basic" | "full") {
                return Err(AppError::Validation("level must be basic or full".into()));
            }
            sqlx::query(
                r#"
                UPDATE enterprise_kyc
                SET level = $1, status = 'approved', reviewed_by_user_id = $2, reviewed_at = NOW(),
                    rejection_reason = NULL, updated_at = NOW()
                WHERE enterprise_id = $3
                "#,
            )
            .bind(&level)
            .bind(auth.user_id)
            .bind(enterprise_id)
            .execute(&state.db)
            .await?;
            metrics::counter!("skilluv_kyc_approved_total", "level" => level).increment(1);
        }
        "reject" => {
            let reason = body.reason.unwrap_or_default();
            sqlx::query(
                r#"
                UPDATE enterprise_kyc
                SET status = 'rejected', reviewed_by_user_id = $1, reviewed_at = NOW(),
                    rejection_reason = $2, updated_at = NOW()
                WHERE enterprise_id = $3
                "#,
            )
            .bind(auth.user_id)
            .bind(&reason)
            .bind(enterprise_id)
            .execute(&state.db)
            .await?;
            metrics::counter!("skilluv_kyc_rejected_total").increment(1);
        }
        _ => {
            return Err(AppError::Validation(
                "action must be approve or reject".into(),
            ));
        }
    }

    // BE-F — audit log unifié.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "kyc_decide",
            target_type: Some("enterprise"),
            target_id: Some(enterprise_id),
            metadata: Some(json!({
                "action": body.action,
                "level": audit_level,
                "reason": audit_reason,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(
        json!({ "decided": true, "action": body.action }),
    )))
}
