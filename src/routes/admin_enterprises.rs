//! ADM-M4 — Admin management enterprises + type + type_config + agency_clients.
//!
//! - GET   /admin/enterprises                          — liste paginée (filtres type, verified)
//! - PATCH /admin/enterprises/{id}/type                — change enterprise_type (reset type_config)
//! - GET   /admin/enterprises/{id}/type-config         — lit type_config JSONB
//! - GET   /admin/enterprises/{id}/agency-clients      — liste clients (vide si non staffing)

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type AdminEnterprisesRow77 = (
    Uuid,
    String,
    String,
    Option<String>,
    bool,
    String,
    Value,
    chrono::DateTime<chrono::Utc>,
);
type AdminEnterprisesRow158 = (
    Uuid,
    String,
    String,
    Option<String>,
    bool,
    String,
    Value,
    chrono::DateTime<chrono::Utc>,
);
type AdminEnterprisesRow370 = (
    Uuid,
    String,
    Option<String>,
    Option<String>,
    bool,
    chrono::DateTime<chrono::Utc>,
);

pub fn admin_enterprise_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/enterprises", get(list_enterprises))
        .route("/admin/enterprises/{id}", get(get_enterprise))
        .route("/admin/enterprises/{id}/type", patch(patch_type))
        .route("/admin/enterprises/{id}/type-config", get(get_type_config))
        .route(
            "/admin/enterprises/{id}/agency-clients",
            get(list_agency_clients),
        )
}

const ALLOWED_TYPES: &[&str] = &["direct_hire", "staffing_agency", "remote_international"];

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
// GET /admin/enterprises
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    verified: Option<bool>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

async fn list_enterprises(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    if let Some(t) = q.r#type.as_ref()
        && !ALLOWED_TYPES.contains(&t.as_str())
    {
        return Err(AppError::Validation(format!(
            "type invalid; allowed: {ALLOWED_TYPES:?}"
        )));
    }

    let rows: Vec<AdminEnterprisesRow77> = sqlx::query_as(
        r#"
            SELECT id, company_name, slug, industry, verified, enterprise_type,
                   type_config, created_at
            FROM enterprises
            WHERE ($1::text IS NULL OR enterprise_type = $1)
              AND ($2::bool IS NULL OR verified = $2)
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
    )
    .bind(q.r#type.as_ref())
    .bind(q.verified)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM enterprises
        WHERE ($1::text IS NULL OR enterprise_type = $1)
          AND ($2::bool IS NULL OR verified = $2)
        "#,
    )
    .bind(q.r#type.as_ref())
    .bind(q.verified)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, name, slug, industry, verified, etype, tconf, created)| {
                json!({
                    "id": id, "company_name": name, "slug": slug, "industry": industry,
                    "verified": verified, "enterprise_type": etype, "type_config": tconf,
                    "created_at": created.to_rfc3339(),
                })
            },
        )
        .collect();

    let total_pages = if per_page > 0 {
        (total + per_page - 1) / per_page
    } else {
        0
    };

    Ok(Json(json!({
        "data": items,
        "pagination": {
            "page": page, "per_page": per_page, "total": total, "total_pages": total_pages,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/enterprises/{id}
// ═══════════════════════════════════════════════════════════════════

async fn get_enterprise(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let row: Option<AdminEnterprisesRow158> = sqlx::query_as(
        r#"SELECT id, company_name, slug, industry, verified, enterprise_type,
                      type_config, created_at
               FROM enterprises WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let (eid, name, slug, industry, verified, etype, tconf, created) =
        row.ok_or_else(|| AppError::NotFound(format!("enterprise {id} not found")))?;

    Ok(Json(build_response(json!({
        "enterprise": {
            "id": eid, "company_name": name, "slug": slug, "industry": industry,
            "verified": verified, "enterprise_type": etype, "type_config": tconf,
            "created_at": created.to_rfc3339(),
        }
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// PATCH /admin/enterprises/{id}/type
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct PatchTypeBody {
    enterprise_type: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct DryRunQuery {
    #[serde(default)]
    dry_run: bool,
}

async fn patch_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
    Query(q): Query<DryRunQuery>,
    Json(body): Json<PatchTypeBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if !ALLOWED_TYPES.contains(&body.enterprise_type.as_str()) {
        return Err(AppError::Validation(format!(
            "enterprise_type invalid; allowed: {ALLOWED_TYPES:?}"
        )));
    }
    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
    }

    let ent: Option<(Uuid, String, Value, Option<String>)> = sqlx::query_as(
        r#"SELECT e.id, e.enterprise_type, e.type_config, u.country_iso2
           FROM enterprises e
           JOIN users u ON u.id = e.owner_id
           WHERE e.id = $1"#,
    )
    .bind(enterprise_id)
    .fetch_optional(&state.db)
    .await?;
    let (id, current_type, current_config, owner_country) =
        ent.ok_or_else(|| AppError::NotFound(format!("enterprise {enterprise_id} not found")))?;

    // remote_international : check pays éligibles si liste configurée.
    if body.enterprise_type == "remote_international"
        && let Ok(allowed) = std::env::var("SKILLUV_REMOTE_INTL_ORIGINS")
    {
        let list: Vec<&str> = allowed
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if !list.is_empty() {
            let owner_c = owner_country.as_deref().unwrap_or("");
            if !list.iter().any(|c| c.eq_ignore_ascii_case(owner_c)) {
                return Err(AppError::Validation(format!(
                    "owner country '{owner_c}' not in SKILLUV_REMOTE_INTL_ORIGINS allowlist"
                )));
            }
        }
    }

    let will_reset = current_type != body.enterprise_type;
    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();
    if dry {
        return Ok(Json(json!({
            "data": {
                "enterprise": {
                    "id": id, "enterprise_type": current_type, "type_config": current_config,
                },
            },
            "meta": {
                "request_id": Uuid::new_v4().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "dry_run_preview": {
                    "will_reset_type_config": will_reset,
                    "target_type": body.enterprise_type,
                }
            }
        })));
    }

    // Transaction atomique. Le trigger P24 (agency_clients) valide.
    let mut tx = state.db.begin().await?;
    let new_config = if will_reset {
        json!({})
    } else {
        current_config.clone()
    };
    let (etype, tconf): (String, Value) = sqlx::query_as(
        "UPDATE enterprises
         SET enterprise_type = $2, type_config = $3, updated_at = NOW()
         WHERE id = $1
         RETURNING enterprise_type, type_config",
    )
    .bind(id)
    .bind(&body.enterprise_type)
    .bind(&new_config)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) => AppError::Validation(format!(
            "enterprise type change rejected by DB constraint: {}",
            db_err.message()
        )),
        other => AppError::Database(other),
    })?;
    tx.commit().await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "enterprise.type_change",
            target_type: Some("enterprise"),
            target_id: Some(id),
            metadata: Some(json!({
                "before": { "enterprise_type": current_type, "type_config": current_config },
                "after":  { "enterprise_type": etype, "type_config": tconf },
                "reason": body.reason,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "enterprise": { "id": id, "enterprise_type": etype, "type_config": tconf }
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/enterprises/{id}/type-config
// ═══════════════════════════════════════════════════════════════════

async fn get_type_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let row: Option<(String, Value)> =
        sqlx::query_as("SELECT enterprise_type, type_config FROM enterprises WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (etype, tconf) =
        row.ok_or_else(|| AppError::NotFound(format!("enterprise {id} not found")))?;

    Ok(Json(build_response(json!({
        "enterprise_type": etype,
        "type_config": tconf,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/enterprises/{id}/agency-clients
// ═══════════════════════════════════════════════════════════════════

async fn list_agency_clients(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    // Non-404 : renvoie tableau vide si l'enterprise n'existe pas ou n'est pas staffing.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM enterprises WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("enterprise {id} not found")));
    }

    let rows: Vec<AdminEnterprisesRow370> = sqlx::query_as(
        r#"SELECT id, client_name, client_contact_email, notes, active, created_at
               FROM agency_clients WHERE enterprise_id = $1
               ORDER BY created_at DESC"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let clients: Vec<Value> = rows
        .into_iter()
        .map(|(cid, name, email, notes, active, created)| {
            json!({
                "id": cid, "client_name": name, "client_contact_email": email,
                "notes": notes, "active": active, "created_at": created.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "clients": clients }))))
}
