//! ADM-M5+ — Ops admin : sweep proof engine + gdpr-export admin-side.
//!
//! - POST /admin/proof-hooks/sweep?within_days=7   — recompute batch pour tous
//!   les users ayant eu de l'activité récente (wrapper `sweep_active_users`).
//! - POST /admin/users/{id}/gdpr-export             — déclenche l'export d'un
//!   user cible (background task) et envoie l'archive à son email.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type AdminOpsRow286 = (
    Uuid,
    String,
    String,
    String,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
    Value,
    bool,
    bool,
    chrono::DateTime<chrono::Utc>,
);

pub fn admin_ops_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/proof-hooks/sweep", post(admin_sweep_proof_hooks))
        .route(
            "/admin/users/{id}/gdpr-export",
            post(admin_trigger_gdpr_export),
        )
        // MVP.md Annexe A #8 — CRUD event (Hacktoberfest, Skilluv Fest).
        .route(
            "/admin/badge-events",
            get(admin_list_badge_events).post(admin_create_badge_event),
        )
        // MVP.md §2.2 ligne 125 — recompute capabilities seul (scope réduit).
        .route(
            "/admin/users/{id}/recompute-capabilities",
            post(admin_recompute_capabilities),
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
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
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

// ═══════════════════════════════════════════════════════════════════
// POST /admin/users/{id}/recompute-capabilities
// ═══════════════════════════════════════════════════════════════════

async fn admin_recompute_capabilities(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(target_id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("user {target_id} not found")));
    }

    if crate::middleware::admin_destructive::is_admin_dry_run() {
        let cap_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(target_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
        return Ok(Json(wrap(json!({
            "dry_run": true,
            "current_active_count": cap_count,
        }))));
    }

    let report =
        crate::services::capabilities_engine::recompute_capabilities_for_user(&state.db, target_id)
            .await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "user.recompute_capabilities",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(json!({
                "granted": report.granted.clone(),
                "already_active": report.already_active.clone(),
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "granted": report.granted,
        "already_active": report.already_active,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/badge-events — liste paginée (filtres is_active + is_partner).
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ListEventsQuery {
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    is_partner: Option<bool>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

async fn admin_list_badge_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListEventsQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(30).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows: Vec<AdminOpsRow286> = sqlx::query_as(
        r#"SELECT id, slug, name, description, starts_at, ends_at,
                  visual_theme, is_partner, is_active, created_at
           FROM events
           WHERE ($1::bool IS NULL OR is_active = $1)
             AND ($2::bool IS NULL OR is_partner = $2)
           ORDER BY starts_at DESC
           LIMIT $3 OFFSET $4"#,
    )
    .bind(q.is_active)
    .bind(q.is_partner)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM events
           WHERE ($1::bool IS NULL OR is_active = $1)
             AND ($2::bool IS NULL OR is_partner = $2)"#,
    )
    .bind(q.is_active)
    .bind(q.is_partner)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, slug, name, desc, starts, ends, theme, partner, active, created)| {
                json!({
                    "id": id, "slug": slug, "name": name, "description": desc,
                    "starts_at": starts.to_rfc3339(),
                    "ends_at": ends.map(|t| t.to_rfc3339()),
                    "visual_theme": theme, "is_partner": partner, "is_active": active,
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
// POST /admin/badge-events — création d'un event (mig 0093).
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreateEventBody {
    slug: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    starts_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    ends_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    visual_theme: Option<Value>,
    #[serde(default)]
    is_partner: Option<bool>,
}

async fn admin_create_badge_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateEventBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    let slug_len = body.slug.len();
    if !(3..=60).contains(&slug_len)
        || !body
            .slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::Validation(
            "slug must be 3..=60 chars matching ^[a-z0-9-]+$".into(),
        ));
    }
    if body.name.trim().is_empty() || body.name.len() > 120 {
        return Err(AppError::Validation("name must be 1..=120 chars".into()));
    }
    if let Some(end) = body.ends_at
        && end < body.starts_at
    {
        return Err(AppError::Validation("ends_at must be >= starts_at".into()));
    }

    if crate::middleware::admin_destructive::is_admin_dry_run() {
        return Ok(Json(wrap(json!({
            "dry_run": true,
            "would_create": { "slug": body.slug, "name": body.name },
        }))));
    }

    let visual = body.visual_theme.clone().unwrap_or_else(|| json!({}));
    let partner = body.is_partner.unwrap_or(false);

    let (id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO events
                (slug, name, description, starts_at, ends_at, visual_theme, is_partner, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id"#,
    )
    .bind(&body.slug)
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.starts_at)
    .bind(body.ends_at)
    .bind(&visual)
    .bind(partner)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.constraint() == Some("events_slug_key") => {
            AppError::Validation(format!("event slug '{}' already exists", body.slug))
        }
        _ => AppError::Database(e),
    })?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "badge_event.create",
            target_type: Some("event"),
            target_id: Some(id),
            metadata: Some(json!({
                "slug": body.slug, "name": body.name,
                "starts_at": body.starts_at.to_rfc3339(),
                "ends_at": body.ends_at.map(|t| t.to_rfc3339()),
                "is_partner": partner,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "event": {
            "id": id, "slug": body.slug, "name": body.name,
            "starts_at": body.starts_at.to_rfc3339(),
            "ends_at": body.ends_at.map(|t| t.to_rfc3339()),
            "visual_theme": visual, "is_partner": partner,
        }
    }))))
}
