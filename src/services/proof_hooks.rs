//! P19.1 — Orchestrateur des 3 engines proof-driven.
//!
//! Contrat : `recompute_all_for_user(db, user_id)` appelle en séquence :
//!   1. `capabilities_engine::recompute_capabilities_for_user` (auto-promotion
//!      challenger/mentor/pr_reviewer/…). Fait AVANT le rank car doyen
//!      dépend de la capability mentor (P18.5).
//!   2. `badge_engine::recompute_badges_for_user` (skill_patches, medals).
//!   3. `ranks::recompute_rank_for_user` (Apprenti→Doyen).
//!
//! Best-effort : chaque étape est encapsulée. Si une échoue, on log tracing::warn
//! et on continue — pas de rollback global. La cohérence viendra du prochain
//! recompute (idempotent).
//!
//! Retourne un rapport agrégé pour l'observabilité (metrics + admin dashboard).

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::{badge_engine, capabilities_engine, ranks};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProofRecomputeReport {
    pub user_id: Uuid,
    pub capabilities_granted: Vec<String>,
    pub capabilities_already_active: Vec<String>,
    pub badges_awarded: Vec<String>,
    pub badges_revoked: Vec<String>,
    pub badges_unchanged: usize,
    pub rank_previous: String,
    pub rank_computed: String,
    pub rank_promoted: bool,
    pub errors: Vec<String>,
}

pub async fn recompute_all_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<ProofRecomputeReport, AppError> {
    let mut errors = Vec::new();

    // 1. Capabilities
    let caps = match capabilities_engine::recompute_capabilities_for_user(db, user_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: capabilities recompute failed");
            errors.push(format!("capabilities: {e}"));
            capabilities_engine::RecomputeCapReport {
                granted: Vec::new(),
                already_active: Vec::new(),
            }
        }
    };

    // 2. Badges
    let badges = match badge_engine::recompute_badges_for_user(db, user_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: badges recompute failed");
            errors.push(format!("badges: {e}"));
            badge_engine::RecomputeReport {
                awarded: Vec::new(),
                revoked: Vec::new(),
                unchanged: 0,
            }
        }
    };

    // 3. Rank
    let (rank_prev, rank_new, rank_promoted) = match ranks::recompute_rank_for_user(db, user_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: rank recompute failed");
            errors.push(format!("rank: {e}"));
            (String::from("apprenti"), String::from("apprenti"), false)
        }
    };

    metrics::counter!(
        "skilluv_proof_hook_recompute_total",
        "result" => if errors.is_empty() { "ok" } else { "partial" },
    )
    .increment(1);

    Ok(ProofRecomputeReport {
        user_id,
        capabilities_granted: caps.granted,
        capabilities_already_active: caps.already_active,
        badges_awarded: badges.awarded,
        badges_revoked: badges.revoked,
        badges_unchanged: badges.unchanged,
        rank_previous: rank_prev,
        rank_computed: rank_new,
        rank_promoted,
        errors,
    })
}

/// P19.3 — Sweep : recompute pour tous les users ayant eu de l'activité
/// récente (deliverable verified OU attestation reçue dans la fenêtre).
/// Retourne la liste des user_ids traités.
pub async fn sweep_active_users(
    db: &PgPool,
    within_days: i32,
) -> Result<Vec<Uuid>, AppError> {
    let user_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT user_id FROM (
            SELECT user_id FROM deliverables
            WHERE verification_status = 'verified'
              AND verified_at >= NOW() - MAKE_INTERVAL(days => $1)
            UNION
            SELECT user_id FROM attestations
            WHERE revoked_at IS NULL
              AND issued_at >= NOW() - MAKE_INTERVAL(days => $1)
        ) t
        "#,
    )
    .bind(within_days)
    .fetch_all(db)
    .await?;

    let mut processed = Vec::with_capacity(user_ids.len());
    for uid in user_ids {
        // Best-effort par user — un échec n'arrête pas le sweep.
        match recompute_all_for_user(db, uid).await {
            Ok(_) => processed.push(uid),
            Err(e) => tracing::warn!(user_id = %uid, error = %e, "sweep skip"),
        }
    }
    Ok(processed)
}
