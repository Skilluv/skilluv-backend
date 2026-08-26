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

/// SKI-43 — recompute, then notify through the durable channel only.
///
/// This is the signature every existing caller uses, and most of them are
/// service-layer functions holding nothing but a `PgPool`. The resulting
/// notifications are written to the `notifications` table, so the user
/// sees them on their next poll; the real-time push is skipped. Callers
/// that do have `AppState` should prefer
/// [`recompute_all_for_user_live`] to also light up WebSocket and mobile.
pub async fn recompute_all_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<ProofRecomputeReport, AppError> {
    recompute_inner(db, user_id, None).await
}

/// SKI-43 — recompute and notify through every channel.
///
/// Identical to [`recompute_all_for_user`] plus live delivery (Redis
/// unread counter, WebSocket, mobile push). Live delivery is best-effort:
/// its failures are logged, never propagated, so notification plumbing can
/// never fail a promotion that genuinely happened.
pub async fn recompute_all_for_user_live(
    db: &PgPool,
    redis: &mut redis::aio::ConnectionManager,
    ws: &crate::websocket::WsManager,
    user_id: Uuid,
) -> Result<ProofRecomputeReport, AppError> {
    let channel = crate::services::promotion_notify::LiveChannel { redis, ws };
    recompute_inner(db, user_id, Some(channel)).await
}

async fn recompute_inner(
    db: &PgPool,
    user_id: Uuid,
    live: Option<crate::services::promotion_notify::LiveChannel<'_>>,
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

    // 1bis. AI attestations, before the badges that count them.
    //
    // Ordered here for the same reason capabilities come before the rank: the
    // step that produces the proof runs before the step that reads it, so a
    // model shipped and its badge land in the same pass rather than one
    // recompute apart.
    match crate::services::ai_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "AI attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: AI attestations failed");
            errors.push(format!("ai_attestations: {e}"));
        }
    }

    // 1ter. Audio attestations, for the same reason and in the same place.
    //
    // Separate from the AI pass rather than folded into a loop over domains:
    // each generator knows which tables carry its evidence, and a shared loop
    // would have to know all of them.
    match crate::services::audio_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "audio attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: audio attestations failed");
            errors.push(format!("audio_attestations: {e}"));
        }
    }

    // 1quater. Quality attestations.
    //
    // Here for the reason the two above are, and with one extra: this
    // domain's signature basis waits on a fix confirmation that arrives long
    // after the deliverable was verified. Hooking verification alone would
    // have left every confirmed defect permanently unattested.
    match crate::services::quality_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "quality attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: quality attestations failed");
            errors.push(format!("quality_attestations: {e}"));
        }
    }

    // 1quinquies. Leadership attestations.
    //
    // The one with the most reasons to run here rather than at verification:
    // a redaction confirmation, a retrospective's last action item and a
    // cohort's conclusion all arrive well after the deliverable was
    // verified.
    match crate::services::leadership_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "leadership attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: leadership attestations failed");
            errors.push(format!("leadership_attestations: {e}"));
        }
    }

    // 1sexies. Communication attestations, for the same reason and in the
    // same place. The published address of an article or a talk recording
    // usually lands after the work is verified, so the generator is run from
    // here rather than from the verification.
    match crate::services::communication_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "communication attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: communication attestations failed");
            errors.push(format!("communication_attestations: {e}"));
        }
    }

    // 1septies. Education attestations. The one with the most reasons to
    // arrive late: the learner-data declaration, the cohort's conclusion and
    // the first adoption all happen after the work is verified.
    //
    // Seven blocks doing the same thing with a different function is where this
    // stops scaling. The eighth domain should turn them into a list of
    // generators rather than an eighth block and an eighth Latin ordinal —
    // each one is `fn(&PgPool, Uuid) -> Result<Vec<String>>`, which is a
    // shape a slice of function pointers can hold.
    match crate::services::education_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "education attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: education attestations failed");
            errors.push(format!("education_attestations: {e}"));
        }
    }

    // 1octies. Security attestations, and the sharpest version of the reason
    // all seven are here rather than at the point of verification: a finding is
    // confirmed in March and published in June, and the publication earns a
    // second basis on the same row. Hooking the confirmation alone would leave
    // every disclosure permanently unattested.
    match crate::services::security_attestations::issue_for_user(db, user_id).await {
        Ok(issued) if !issued.is_empty() => {
            tracing::info!(user_id = %user_id, ?issued, "security attestations issued");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "P19: security attestations failed");
            errors.push(format!("security_attestations: {e}"));
        }
    }

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
    let (rank_prev, rank_new, rank_promoted) =
        match ranks::recompute_rank_for_user(db, user_id).await {
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
        )
        .increment(1);
    }
    for slug in &badges.awarded {
        metrics::counter!(
            "skilluv_badges_awarded_total",
            "rule" => slug.clone(),
        )
        .increment(1);
    }
    for slug in &badges.revoked {
        metrics::counter!(
            "skilluv_badges_revoked_total",
            "rule" => slug.clone(),
        )
        .increment(1);
    }
    if rank_promoted {
        metrics::counter!(
            "skilluv_ranks_promoted_total",
            "rank" => rank_new.clone(),
        )
        .increment(1);
    }

    let report = ProofRecomputeReport {
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
    };

    // SKI-43 — celebrate at the psychological moment. Best-effort like
    // every other step here: a notification failure is logged into the
    // report's errors and never masks the promotion itself.
    let mut report = report;
    if let Err(e) = crate::services::promotion_notify::notify_from_report(db, &report, live).await {
        tracing::warn!(user_id = %user_id, error = %e, "SKI-43: promotion notifications failed");
        report.errors.push(format!("notifications: {e}"));
    }

    Ok(report)
}

/// P19.3 — Sweep : recompute pour tous les users ayant eu de l'activité
/// récente (deliverable verified OU attestation reçue dans la fenêtre).
/// Retourne la liste des user_ids traités.
pub async fn sweep_active_users(db: &PgPool, within_days: i32) -> Result<Vec<Uuid>, AppError> {
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
                    metrics::counter!("skilluv_proof_sweep_users_processed_total",)
                        .increment(processed.len() as u64);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "P19.3: proof sweep failed");
                }
            }
        }
    });
}
