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

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::{badge_engine, capabilities_engine, ranks};

/// P19.3 — Sweep interval par défaut (7 jours = 604 800 secondes).
const DEFAULT_SWEEP_INTERVAL_SECS: u64 = 60 * 60 * 24 * 7;
/// Fenêtre de "user actif" (30 jours) — évite de recomputer tout le monde.
const DEFAULT_SWEEP_WINDOW_DAYS: i32 = 30;

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

    // P19.4 — Metrics granulaires.
    metrics::counter!(
        "skilluv_proof_hook_recompute_total",
        "result" => if errors.is_empty() { "ok" } else { "partial" },
    )
    .increment(1);
    for slug in &caps.granted {
        metrics::counter!(
            "skilluv_capabilities_granted_total",
            "capability" => slug.clone(),
        ).increment(1);
    }
    for slug in &badges.awarded {
        metrics::counter!(
            "skilluv_badges_awarded_total",
            "rule" => slug.clone(),
        ).increment(1);
    }
    for slug in &badges.revoked {
        metrics::counter!(
            "skilluv_badges_revoked_total",
            "rule" => slug.clone(),
        ).increment(1);
    }
    if rank_promoted {
        metrics::counter!(
            "skilluv_ranks_promoted_total",
            "rank" => rank_new.clone(),
        ).increment(1);
    }

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

/// P19.3 — Task de fond : sweep hebdomadaire des users actifs.
///
/// Contrôlée par env :
///   - `SKILLUV_PROOF_SWEEP_ENABLED=1` pour activer (default OFF en dev).
///   - `SKILLUV_PROOF_SWEEP_INTERVAL_SECS` (default 604800 = 7 jours).
///   - `SKILLUV_PROOF_SWEEP_WINDOW_DAYS` (default 30).
///
/// Le sweep sert de filet de sécurité : les hooks inline (P19.2) attrapent
/// 99 % des cas ; ce job rattrape les evolutions de seuils (nouvelles rules
/// ajoutées, capabilities engine mis à jour), ou les hooks qui auraient
/// échoué silencieusement.
pub fn start_proof_sweep_task(db: PgPool) {
    if std::env::var("SKILLUV_PROOF_SWEEP_ENABLED").as_deref() != Ok("1") {
        tracing::info!("P19.3: proof sweep disabled (set SKILLUV_PROOF_SWEEP_ENABLED=1)");
        return;
    }
    let interval_secs = std::env::var("SKILLUV_PROOF_SWEEP_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SWEEP_INTERVAL_SECS);
    let window_days = std::env::var("SKILLUV_PROOF_SWEEP_WINDOW_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SWEEP_WINDOW_DAYS);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            match sweep_active_users(&db, window_days).await {
                Ok(processed) => {
                    tracing::info!(
                        count = processed.len(),
                        window_days,
                        "P19.3: proof sweep completed"
                    );
                    metrics::counter!(
                        "skilluv_proof_sweep_users_processed_total",
                    )
                    .increment(processed.len() as u64);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "P19.3: proof sweep failed");
                }
            }
        }
    });
}
