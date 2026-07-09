//! Service `reviews` — soumission de verdict par un reviewer humain (Phase P2.2).
//!
//! Voir docs/challenges-target-model-and-roadmap.md partie G.3 (attestations)
//! et H.2 (workflow reviewer).
//!
//! Rôle : soumettre un verdict sur un deliverable en attente, avec propagation
//! des side-effects (verified → fragments + skills, rejected → status figé).
//! Toute la logique est transactionnelle pour garantir cohérence.

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::UserSkill;
use crate::services::ReviewQueueService;

/// Fragments récompensés au reviewer selon verdict (décision Q4).
///
/// - `approve` sur un deliverable qui reste valide 30j → 10 fragments
///   (attribution différée pour l'instant : on donne 10 dès le vote,
///   révoqué si le deliverable est révoqué dans les 30j — Phase P3+)
/// - `reject` avec confirmation par un 2e reviewer ou une correction ultérieure
///   → 15 fragments (attribution différée — Phase P3+)
/// - `request_changes` menant à un cycle réussi → 5 fragments (Phase P3+)
/// - `abstain` → 0 fragments
///
/// En P2.2 : on attribue seulement l'award immédiat sur approve (10 fragments).
pub const REVIEWER_FRAGMENTS_APPROVE: i32 = 10;
pub const REVIEWER_FRAGMENTS_REJECT: i32 = 0; // attribué a posteriori (Phase P3+)
pub const REVIEWER_FRAGMENTS_REQUEST_CHANGES: i32 = 0; // attribué a posteriori
pub const REVIEWER_FRAGMENTS_ABSTAIN: i32 = 0;

pub struct ReviewsService;

/// Verdict rendu par un reviewer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approve,
    RequestChanges,
    Reject,
    Abstain,
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::RequestChanges => "request_changes",
            Self::Reject => "reject",
            Self::Abstain => "abstain",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "approve" => Some(Self::Approve),
            "request_changes" => Some(Self::RequestChanges),
            "reject" => Some(Self::Reject),
            "abstain" => Some(Self::Abstain),
            _ => None,
        }
    }

    pub fn reviewer_fragments(&self) -> i32 {
        match self {
            Self::Approve => REVIEWER_FRAGMENTS_APPROVE,
            Self::RequestChanges => REVIEWER_FRAGMENTS_REQUEST_CHANGES,
            Self::Reject => REVIEWER_FRAGMENTS_REJECT,
            Self::Abstain => REVIEWER_FRAGMENTS_ABSTAIN,
        }
    }
}

/// Paramètres pour soumettre un verdict.
#[derive(Debug, Clone)]
pub struct SubmitParams {
    pub deliverable_id: Uuid,
    pub reviewer_user_id: Uuid,
    pub verdict: Verdict,
    pub body: String,
    pub time_spent_seconds: Option<i32>,
}

/// Résultat de la soumission.
#[derive(Debug, Serialize)]
pub struct SubmitOutcome {
    pub review_id: Uuid,
    /// Nouveau statut du deliverable après le verdict.
    pub new_deliverable_status: String,
    /// Si applicable, fragments attribués au reviewer immédiatement.
    pub reviewer_fragments_awarded: i32,
}

impl ReviewsService {
    // ═══════════════════════════════════════════════════════════════════
    // Point d'entrée : submit_verdict
    // ═══════════════════════════════════════════════════════════════════

    /// Soumet un verdict de reviewer sur un deliverable.
    ///
    /// Workflow :
    /// 1. INSERT dans reviews (UNIQUE deliverable_id + reviewer_user_id empêche
    ///    la double-review)
    /// 2. Selon verdict :
    ///    - approve → deliverable.verification_status = 'verified'
    ///                + side-effects (fragments, skills, slice → merged si applicable)
    ///    - request_changes → deliverable reste 'pending', body devient feedback
    ///    - reject → deliverable.verification_status = 'rejected'
    ///    - abstain → deliverable reste pending, task retourne à open
    /// 3. Mark associated review_task as 'completed'
    /// 4. Award reviewer fragments selon verdict
    ///
    /// Transactionnel.
    pub async fn submit_verdict(
        db: &PgPool,
        params: SubmitParams,
    ) -> Result<SubmitOutcome, AppError> {
        let mut tx = db.begin().await?;

        // 1. Vérifier que le deliverable est bien en attente de review
        let (verification_status, slice_id, deliverable_user_id, fragments_reward):
            (String, Option<Uuid>, Uuid, i32) = sqlx::query_as(
            r#"
            SELECT d.verification_status,
                   d.slice_id,
                   d.user_id,
                   COALESCE(ps.fragments_reward, 0)
            FROM deliverables d
            LEFT JOIN project_slices ps ON ps.id = d.slice_id
            WHERE d.id = $1
            "#,
        )
        .bind(params.deliverable_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Deliverable not found".to_string()))?;

        if !matches!(
            verification_status.as_str(),
            "pending" | "pending_manual_review" | "pending_admin_escalation"
        ) {
            return Err(AppError::Validation(format!(
                "Cannot review deliverable in status '{verification_status}'"
            )));
        }

        // 2. INSERT dans reviews (UNIQUE constraint empêche la double review)
        let reviewer_fragments = params.verdict.reviewer_fragments();
        let review_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO reviews
                (deliverable_id, reviewer_user_id, verdict, body,
                 time_spent_seconds, fragments_awarded)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(params.deliverable_id)
        .bind(params.reviewer_user_id)
        .bind(params.verdict.as_str())
        .bind(&params.body)
        .bind(params.time_spent_seconds)
        .bind(reviewer_fragments)
        .fetch_one(&mut *tx)
        .await?;

        // 3. Update deliverable status selon verdict
        let new_status = match params.verdict {
            Verdict::Approve => "verified",
            Verdict::RequestChanges => "pending",
            Verdict::Reject => "rejected",
            Verdict::Abstain => &verification_status, // pas de changement
        };

        if new_status != verification_status {
            sqlx::query(
                r#"
                UPDATE deliverables
                SET verification_status = $1,
                    verified_at = CASE WHEN $1 = 'verified' THEN NOW() ELSE verified_at END,
                    verified_by_user_id = CASE WHEN $1 = 'verified' THEN $2 ELSE verified_by_user_id END,
                    verification_notes = $3
                WHERE id = $4
                "#,
            )
            .bind(new_status)
            .bind(params.reviewer_user_id)
            .bind(&params.body)
            .bind(params.deliverable_id)
            .execute(&mut *tx)
            .await?;
        }

        // 4. Si approve : propagation des side-effects (comme le workflow G.1)
        if params.verdict == Verdict::Approve {
            Self::apply_verified_side_effects(
                &mut tx,
                params.deliverable_id,
                slice_id,
                deliverable_user_id,
                fragments_reward,
            )
            .await?;
        }

        // 5. Marquer la review_task associée comme completed
        Self::mark_associated_task_completed(&mut tx, params.deliverable_id, review_id).await?;

        // 6. Bonus reviewer fragments immédiat (si applicable en P2.2)
        if reviewer_fragments > 0 {
            sqlx::query(
                "UPDATE users
                 SET total_fragments = total_fragments + $1, updated_at = NOW()
                 WHERE id = $2",
            )
            .bind(reviewer_fragments)
            .bind(params.reviewer_user_id)
            .execute(&mut *tx)
            .await?;
        }

        // 7. Bump les compteurs review_metrics
        Self::bump_metrics_counter(&mut tx, params.reviewer_user_id, params.verdict).await?;

        tx.commit().await?;

        Ok(SubmitOutcome {
            review_id,
            new_deliverable_status: new_status.to_string(),
            reviewer_fragments_awarded: reviewer_fragments,
        })
    }

    // ═══════════════════════════════════════════════════════════════════
    // Propagation des side-effects (approve)
    // ═══════════════════════════════════════════════════════════════════

    /// Applique les side-effects d'un deliverable qui passe verified via review :
    /// - fragments à l'auteur du deliverable
    /// - propagation des skills (même formule que DeliverablesService::propagate_skills)
    /// - slice → merged si liée
    async fn apply_verified_side_effects(
        tx: &mut Transaction<'_, Postgres>,
        deliverable_id: Uuid,
        slice_id: Option<Uuid>,
        user_id: Uuid,
        fragments_reward: i32,
    ) -> Result<(), AppError> {
        // Set fragments_awarded on the deliverable itself
        sqlx::query(
            "UPDATE deliverables SET fragments_awarded = $1 WHERE id = $2",
        )
        .bind(fragments_reward)
        .bind(deliverable_id)
        .execute(&mut **tx)
        .await?;

        // Fragments à l'auteur
        if fragments_reward > 0 {
            sqlx::query(
                "UPDATE users
                 SET total_fragments = total_fragments + $1, updated_at = NOW()
                 WHERE id = $2",
            )
            .bind(fragments_reward)
            .bind(user_id)
            .execute(&mut **tx)
            .await?;
        }

        // Propagation skills si slice attachée
        if let Some(sid) = slice_id {
            Self::propagate_skills(tx, sid, user_id).await?;

            // Slice → merged
            sqlx::query(
                "UPDATE project_slices
                 SET status = 'merged', closed_at = NOW(), updated_at = NOW()
                 WHERE id = $1 AND status IN ('in_review', 'claimed')",
            )
            .bind(sid)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    /// Miroir de DeliverablesService::propagate_skills.
    ///
    /// Duplication assumée en P2.2 : on refactor en un helper commun en Phase P3
    /// une fois qu'on est sûr des invariants.
    async fn propagate_skills(
        tx: &mut Transaction<'_, Postgres>,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let slice_skills: Vec<(Uuid, i16)> = sqlx::query_as(
            "SELECT skill_id, weight FROM slice_skills WHERE slice_id = $1",
        )
        .bind(slice_id)
        .fetch_all(&mut **tx)
        .await?;

        for (skill_id, weight) in slice_skills {
            sqlx::query(
                r#"
                INSERT INTO user_skills (
                    user_id, skill_id, proven_count, weighted_proven_count,
                    proficiency_level, first_proven_at, last_proven_at
                )
                VALUES ($1, $2, 1, $3, 1, NOW(), NOW())
                ON CONFLICT (user_id, skill_id) DO UPDATE SET
                    proven_count = user_skills.proven_count + 1,
                    weighted_proven_count = user_skills.weighted_proven_count + $3,
                    last_proven_at = NOW(),
                    first_proven_at = COALESCE(user_skills.first_proven_at, NOW())
                "#,
            )
            .bind(user_id)
            .bind(skill_id)
            .bind(weight as i32)
            .execute(&mut **tx)
            .await?;

            let wpc: i32 = sqlx::query_scalar(
                "SELECT weighted_proven_count FROM user_skills
                 WHERE user_id = $1 AND skill_id = $2",
            )
            .bind(user_id)
            .bind(skill_id)
            .fetch_one(&mut **tx)
            .await?;

            let new_level = UserSkill::proficiency_level_for(wpc);

            sqlx::query(
                "UPDATE user_skills SET proficiency_level = $1
                 WHERE user_id = $2 AND skill_id = $3",
            )
            .bind(new_level)
            .bind(user_id)
            .bind(skill_id)
            .execute(&mut **tx)
            .await?;

            // P5 auto-issue attestations (idempotent via UNIQUE index)
            let _issued = crate::services::AttestationsService::check_and_issue_for_skill_levelup(
                tx, user_id, skill_id, new_level,
            )
            .await?;
        }
        Ok(())
    }

    async fn mark_associated_task_completed(
        tx: &mut Transaction<'_, Postgres>,
        deliverable_id: Uuid,
        review_id: Uuid,
    ) -> Result<(), AppError> {
        // Une task en 'claimed' ou 'open' liée à ce deliverable → completed
        let task_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM review_tasks
            WHERE deliverable_id = $1
              AND status IN ('open', 'claimed')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(deliverable_id)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(tid) = task_id {
            ReviewQueueService::mark_completed(tx, tid, review_id).await?;
        }
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // review_metrics
    // ═══════════════════════════════════════════════════════════════════

    /// Incrémente les compteurs bruts de review_metrics pour un reviewer.
    /// Les scores dérivés (accuracy, rejection_relevance, reputation) sont
    /// recalculés par un job nightly (à écrire en Phase P3+).
    async fn bump_metrics_counter(
        tx: &mut Transaction<'_, Postgres>,
        reviewer_user_id: Uuid,
        verdict: Verdict,
    ) -> Result<(), AppError> {
        let (approve_delta, reject_delta, changes_delta, abstain_delta) = match verdict {
            Verdict::Approve => (1, 0, 0, 0),
            Verdict::Reject => (0, 1, 0, 0),
            Verdict::RequestChanges => (0, 0, 1, 0),
            Verdict::Abstain => (0, 0, 0, 1),
        };

        sqlx::query(
            r#"
            INSERT INTO review_metrics (
                reviewer_user_id, total_reviews,
                approved_count, rejected_count, request_changes_count, abstain_count
            )
            VALUES ($1, 1, $2, $3, $4, $5)
            ON CONFLICT (reviewer_user_id) DO UPDATE SET
                total_reviews = review_metrics.total_reviews + 1,
                approved_count = review_metrics.approved_count + $2,
                rejected_count = review_metrics.rejected_count + $3,
                request_changes_count = review_metrics.request_changes_count + $4,
                abstain_count = review_metrics.abstain_count + $5,
                last_recomputed_at = NOW()
            "#,
        )
        .bind(reviewer_user_id)
        .bind(approve_delta)
        .bind(reject_delta)
        .bind(changes_delta)
        .bind(abstain_delta)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}
