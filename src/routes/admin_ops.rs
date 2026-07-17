//! ADM-M5+ — Ops admin : sweep proof engine + gdpr-export admin-side.
//!
//! - POST /admin/proof-hooks/sweep?within_days=7   — recompute batch pour tous
//!   les users ayant eu de l'activité récente (wrapper `sweep_active_users`).
//! - POST /admin/users/{id}/gdpr-export             — déclenche l'export d'un
//!   user cible (background task) et envoie l'archive à son email.

use axum::extract::{Path, Query, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_ops_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/proof-hooks/sweep", post(admin_sweep_proof_hooks))
        .route(
            "/admin/users/{id}/gdpr-export",
            post(admin_trigger_gdpr_export),
        )
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/proof-hooks/sweep?within_days=7
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct SweepQuery {
    #[serde(default)]
    within_days: Option<i32>,
    #[serde(default)]
    dry_run: bool,
}

async fn admin_sweep_proof_hooks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SweepQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    let within = q.within_days.unwrap_or(7).clamp(1, 90);
    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();

    if dry {
        // Preview : combien de users seraient traités sans exécuter.
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM (
                SELECT DISTINCT user_id FROM deliverables
                WHERE verification_status = 'verified'
                  AND verified_at >= NOW() - MAKE_INTERVAL(days => $1)
                UNION
                SELECT user_id FROM attestations
                WHERE revoked_at IS NULL
                  AND issued_at >= NOW() - MAKE_INTERVAL(days => $1)
            ) t
            "#,
        )
        .bind(within)
        .fetch_one(&state.db)
        .await?;
        return Ok(Json(wrap(json!({
            "dry_run": true, "within_days": within, "would_process_count": count,
        }))));
    }

    let processed = crate::services::proof_hooks::sweep_active_users(&state.db, within).await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "proof_hooks.sweep",
            target_type: None,
            target_id: None,
            metadata: Some(json!({
                "within_days": within,
                "processed_count": processed.len(),
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "within_days": within,
        "processed_count": processed.len(),
        "user_ids": processed,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/users/{id}/gdpr-export
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct GdprExportBody {
    /// Raison obligatoire (audit trail).
    reason: String,
}

async fn admin_trigger_gdpr_export(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
    Json(body): Json<GdprExportBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation("reason must be at least 8 chars".into()));
    }

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(target_id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("user {target_id} not found")));
    }

    if crate::middleware::admin_destructive::is_admin_dry_run() {
        return Ok(Json(wrap(json!({
            "dry_run": true,
            "would_trigger_export_for_user": target_id,
        }))));
    }

    // Audit AVANT le spawn (best-effort mais synchrone).
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "user.admin_gdpr_export",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(json!({ "reason": body.reason })),
            headers: None,
        },
    )
    .await;

    let db = state.db.clone();
    let storage = state.storage.clone();
    let email = state.email.clone();
    tokio::spawn(async move {
        match crate::services::data_export::generate_export(db, storage, email, target_id).await {
            Ok(artifact) => tracing::info!(
                admin_target = %target_id, key = %artifact.key,
                "admin-triggered data export delivered"
            ),
            Err(err) => tracing::error!(
                admin_target = %target_id, error = %err,
                "admin-triggered data export failed"
            ),
        }
    });

    Ok(Json(wrap(json!({
        "status": "queued",
        "target_user_id": target_id,
        "message": "Export is being prepared; user will receive it by email within a few minutes.",
    }))))
}
