//! ADM-M5 — Enrichissement admin sur users : proof recompute + rank override + orientations peek.
//!
//! - POST /admin/users/{id}/recompute-proofs   — wrap proof_hooks::recompute_all_for_user
//! - POST /admin/users/{id}/rank-override      — force un rank + audit + rank_overrides row
//! - GET  /users/{id}/orientations             — admin-scoped read (via admin_gate)

use axum::extract::{Path, Query, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_user_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/users/{id}/recompute-proofs",
            post(admin_recompute_proofs),
        )
        .route("/admin/users/{id}/rank-override", post(admin_rank_override))
    // Note: `GET /users/{id}/orientations` était monté ici en admin-scoped
    // à l'origine. Déplacé dans `orientations.rs` en route publique respectant
    // la privacy (BACKEND-GAPS FE-M1). Admin peut consommer la même route.
}

const ALLOWED_RANKS: &[&str] = &["apprenti", "ranger", "artisan", "maitre", "doyen"];

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct DryRunQuery {
    #[serde(default)]
    dry_run: bool,
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/users/{id}/recompute-proofs
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct RecomputeBody {
    #[serde(default)]
    scope: Option<String>, // "capabilities|badges|ranks|all", accepté mais actuellement no-op (recompute complet)
    #[serde(default)]
    reason: Option<String>,
}

async fn admin_recompute_proofs(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
    Query(q): Query<DryRunQuery>,
    Json(body): Json<RecomputeBody>,
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

    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();
    if dry {
        // Preview : snapshot état actuel (rank + capabilities + badges).
        let current_rank: Option<String> =
            sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
                .bind(target_id)
                .fetch_optional(&state.db)
                .await?;
        let cap_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(target_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
        let badge_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_badges WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(target_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
        return Ok(Json(build_response(json!({
            "dry_run": true,
            "current_state": {
                "rank": current_rank,
                "capabilities_active_count": cap_count,
                "badges_active_count": badge_count,
            },
            "would_recompute": body.scope.clone().unwrap_or_else(|| "all".into()),
        }))));
    }

    // Verrou row-level (SELECT FOR UPDATE dans une tx) empêche double-write concurrent.
    let mut tx = state.db.begin().await?;
    let _lock: (Uuid,) = sqlx::query_as("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(target_id)
        .fetch_one(&mut *tx)
        .await?;

    let report = crate::services::proof_hooks::recompute_all_for_user(&state.db, target_id).await?;
    tx.commit().await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "user.recompute_proofs",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(json!({
                "scope": body.scope,
                "reason": body.reason,
                "capabilities_granted": report.capabilities_granted.clone(),
                "badges_awarded": report.badges_awarded.clone(),
                "badges_revoked": report.badges_revoked.clone(),
                "rank_before": report.rank_previous.clone(),
                "rank_after": report.rank_computed.clone(),
                "errors": report.errors.clone(),
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "recomputed": {
            "capabilities_added": report.capabilities_granted,
            "capabilities_removed": Vec::<String>::new(),
            "badges_added": report.badges_awarded,
            "badges_removed": report.badges_revoked,
            "rank_before": report.rank_previous,
            "rank_after": report.rank_computed,
            "errors": report.errors,
        }
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/users/{id}/rank-override
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct RankOverrideBody {
    new_rank: String,
    reason: String,
}

async fn admin_rank_override(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
    Query(q): Query<DryRunQuery>,
    Json(body): Json<RankOverrideBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if !ALLOWED_RANKS.contains(&body.new_rank.as_str()) {
        return Err(AppError::Validation(format!(
            "new_rank invalid; allowed: {ALLOWED_RANKS:?}"
        )));
    }
    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
    }

    let current: Option<(String,)> =
        sqlx::query_as("SELECT rank FROM user_ranks WHERE user_id = $1")
            .bind(target_id)
            .fetch_optional(&state.db)
            .await?;
    let old_rank = current.map(|(r,)| r).unwrap_or_else(|| "apprenti".into());

    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();
    if dry {
        // Delta leaderboard estimé : nombre de users au new_rank (pour situer).
        let peers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_ranks WHERE rank = $1")
            .bind(&body.new_rank)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
        return Ok(Json(build_response(json!({
            "dry_run": true,
            "would_override": {
                "user_id": target_id,
                "old_rank": old_rank,
                "new_rank": body.new_rank,
                "peers_at_new_rank": peers,
            }
        }))));
    }

    let mut tx = state.db.begin().await?;

    // 1. Insert row historique dans rank_overrides.
    let (override_id,): (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO rank_overrides (user_id, admin_id, old_rank, new_rank, reason)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(target_id)
    .bind(auth.user_id)
    .bind(&old_rank)
    .bind(&body.new_rank)
    .bind(&body.reason)
    .fetch_one(&mut *tx)
    .await?;

    // 2. Écrit user_ranks (upsert : le user peut ne pas encore avoir de row).
    sqlx::query(
        r#"
        INSERT INTO user_ranks (user_id, rank, achieved_at, previous_rank)
        VALUES ($1, $2, NOW(), $3)
        ON CONFLICT (user_id) DO UPDATE SET
            rank = EXCLUDED.rank,
            previous_rank = user_ranks.rank,
            achieved_at = NOW()
        "#,
    )
    .bind(target_id)
    .bind(&body.new_rank)
    .bind(&old_rank)
    .execute(&mut *tx)
    .await?;

    // 3. Trace dans user_rank_history.
    sqlx::query(
        r#"
        INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(target_id)
    .bind(&old_rank)
    .bind(&body.new_rank)
    .bind(format!("admin override: {}", body.reason))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "user.rank_override",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(json!({
                "before": { "rank": old_rank },
                "after":  { "rank": body.new_rank },
                "reason": body.reason,
                "override_id": override_id,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "user_id": target_id,
        "old_rank": old_rank,
        "new_rank": body.new_rank,
        "override_id": override_id,
    }))))
}

// (peek_user_orientations déplacé vers routes/orientations.rs en route publique.)
