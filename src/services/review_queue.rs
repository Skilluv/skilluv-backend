//! Service `review_queue` — file d'attente de review humaine (Phase P2.2).
//!
//! Voir docs/challenges-target-model-and-roadmap.md partie H.2.
//!
//! Rôle : gérer le cycle de vie des `review_tasks` :
//!   open → claimed (soft-lock 2h) → completed
//!   open/claimed → escalated (SLA 72h)
//!
//! Le service ne fait PAS la finalisation du deliverable (approve → verified) ;
//! c'est le rôle de `ReviewsService::submit_verdict` qui appelle ensuite
//! `ReviewQueueService::mark_completed`.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;

/// Durée du soft-lock d'un claim par un reviewer (2h, alignée H.2).
pub const CLAIM_LOCK_HOURS: i64 = 2;

/// SLA avant escalade automatique vers admin (72h, décision W4).
pub const SLA_HOURS: i64 = 72;

pub struct ReviewQueueService;

/// Filtres pour lister la queue.
#[derive(Debug, Clone, Default)]
pub struct QueueFilter {
    pub primary_domain: Option<String>,
    pub max_seniority: SeniorityLevel,
    pub page: i64,
    pub per_page: i64,
}

/// Niveau de séniorité éligible du reviewer (utilisé pour filtrer les tasks).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SeniorityLevel {
    #[default]
    Any,
    Contribs,
    Impact,
}

impl SeniorityLevel {
    /// Retourne les valeurs de `required_seniority` que ce reviewer peut prendre.
    ///
    /// Un reviewer 'contribs' peut prendre 'any' + 'contribs' (mais pas 'impact').
    /// Un reviewer 'impact' peut prendre tout.
    pub fn eligible_task_seniorities(&self) -> Vec<&'static str> {
        match self {
            Self::Any => vec!["any"],
            Self::Contribs => vec!["any", "contribs"],
            Self::Impact => vec!["any", "contribs", "impact"],
        }
    }
}

/// Ligne de review_task (calquée sur le schéma SQL).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReviewTask {
    pub id: Uuid,
    pub task_type: String,
    pub deliverable_id: Option<Uuid>,
    pub slice_id: Option<Uuid>,
    pub status: String,
    pub claimed_by_user_id: Option<Uuid>,
    pub claimed_at: Option<chrono::DateTime<Utc>>,
    pub claim_expires_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub completed_review_id: Option<Uuid>,
    pub priority: i16,
    pub sla_deadline: chrono::DateTime<Utc>,
    pub escalated_at: Option<chrono::DateTime<Utc>>,
    pub primary_domain: String,
    pub required_seniority: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

impl ReviewQueueService {
    // ═══════════════════════════════════════════════════════════════════
    // Création de tasks (déclenchée par DeliverablesService ou soumission manuelle)
    // ═══════════════════════════════════════════════════════════════════

    /// Crée une task `verify_deliverable` liée à un deliverable pending.
    ///
    /// Appelée depuis DeliverablesService après insertion d'un deliverable
    /// verifiable_by='human_review' ou verification_status='pending'.
    ///
    /// Priorité par défaut 3. La priorité peut être ajustée par le steward
    /// via un endpoint futur (Phase P3+).
    pub async fn create_task_for_deliverable(
        tx: &mut Transaction<'_, Postgres>,
        deliverable_id: Uuid,
        primary_domain: &str,
        priority: i16,
        required_seniority: &str,
    ) -> Result<Uuid, AppError> {
        let sla = Utc::now() + Duration::hours(SLA_HOURS);

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO review_tasks
                (task_type, deliverable_id, primary_domain, priority,
                 required_seniority, sla_deadline)
            VALUES ('verify_deliverable', $1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(deliverable_id)
        .bind(primary_domain)
        .bind(priority)
        .bind(required_seniority)
        .bind(sla)
        .fetch_one(&mut **tx)
        .await?;

        Ok(id)
    }

    /// Crée une task `verify_slice_claim` pour arbitrer un mismatch author/claimed_by.
    ///
    /// Appelée depuis DeliverablesService::insert_deliverable_pending_manual_review.
    /// Priorité 4 (plus urgent qu'une review normale, la slice est bloquée en in_review).
    pub async fn create_task_for_slice_claim(
        tx: &mut Transaction<'_, Postgres>,
        deliverable_id: Uuid,
        slice_id: Uuid,
        primary_domain: &str,
    ) -> Result<Uuid, AppError> {
        let sla = Utc::now() + Duration::hours(SLA_HOURS);

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO review_tasks
                (task_type, deliverable_id, slice_id, primary_domain, priority,
                 required_seniority, sla_deadline)
            VALUES ('verify_slice_claim', $1, $2, $3, 4, 'contribs', $4)
            RETURNING id
            "#,
        )
        .bind(deliverable_id)
        .bind(slice_id)
        .bind(primary_domain)
        .bind(sla)
        .fetch_one(&mut **tx)
        .await?;

        Ok(id)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Lecture de la queue
    // ═══════════════════════════════════════════════════════════════════

    /// Liste les tasks `status='open'` éligibles pour ce niveau de séniorité.
    ///
    /// Trié par priority DESC puis created_at ASC (FIFO parmi égaux).
    pub async fn list_open(db: &PgPool, filter: &QueueFilter) -> Result<Vec<ReviewTask>, AppError> {
        let per_page = filter.per_page.clamp(1, 50);
        let page = filter.page.max(1);
        let offset = (page - 1) * per_page;

        let eligible = filter.max_seniority.eligible_task_seniorities();

        let tasks = sqlx::query_as::<_, ReviewTask>(
            r#"
            SELECT * FROM review_tasks
            WHERE status = 'open'
              AND required_seniority = ANY($1)
              AND ($2::text IS NULL OR primary_domain = $2)
            ORDER BY priority DESC, created_at ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(&eligible)
        .bind(&filter.primary_domain)
        .bind(per_page)
        .bind(offset)
        .fetch_all(db)
        .await?;

        Ok(tasks)
    }

    /// Récupère une task par id.
    pub async fn get(db: &PgPool, task_id: Uuid) -> Result<ReviewTask, AppError> {
        sqlx::query_as::<_, ReviewTask>("SELECT * FROM review_tasks WHERE id = $1")
            .bind(task_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Review task not found".to_string()))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Claim et complete
    // ═══════════════════════════════════════════════════════════════════

    /// Le reviewer claim une task (soft-lock 2h).
    ///
    /// Erreurs :
    /// - `NotFound` si la task n'existe pas ou n'est pas 'open'
    /// - `Forbidden` si le niveau de séniorité du reviewer est insuffisant
    ///   (à vérifier en amont, ce service ne connaît pas la phase du user)
    pub async fn claim(
        db: &PgPool,
        task_id: Uuid,
        reviewer_user_id: Uuid,
    ) -> Result<ReviewTask, AppError> {
        let expires_at = Utc::now() + Duration::hours(CLAIM_LOCK_HOURS);

        let task = sqlx::query_as::<_, ReviewTask>(
            r#"
            UPDATE review_tasks
            SET status = 'claimed',
                claimed_by_user_id = $1,
                claimed_at = NOW(),
                claim_expires_at = $2,
                updated_at = NOW()
            WHERE id = $3
              AND status = 'open'
            RETURNING *
            "#,
        )
        .bind(reviewer_user_id)
        .bind(expires_at)
        .bind(task_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "Task is not available for claim (already claimed, completed, or missing)"
                    .to_string(),
            )
        })?;

        Ok(task)
    }

    /// Marque une task comme completed. Appelée après verdict soumis.
    ///
    /// Appelée dans la même transaction que l'insertion du verdict dans `reviews`.
    pub async fn mark_completed(
        tx: &mut Transaction<'_, Postgres>,
        task_id: Uuid,
        review_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE review_tasks
            SET status = 'completed',
                completed_at = NOW(),
                completed_review_id = $1,
                updated_at = NOW()
            WHERE id = $2
              AND status IN ('claimed', 'open')
            "#,
        )
        .bind(review_id)
        .bind(task_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // Cron : expire stale claims et escalate SLA
    // ═══════════════════════════════════════════════════════════════════

    /// Retourne au pool `open` les claims dont le lock 2h est dépassé sans complete.
    /// À appeler toutes les 15 minutes.
    pub async fn expire_stale_claims(db: &PgPool) -> Result<u64, AppError> {
        let result = sqlx::query(
            r#"
            UPDATE review_tasks
            SET status = 'open',
                claimed_by_user_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE status = 'claimed'
              AND claim_expires_at IS NOT NULL
              AND claim_expires_at < NOW()
            "#,
        )
        .execute(db)
        .await?;
        Ok(result.rows_affected())
    }

    /// Escalade automatique des tasks dont le SLA 72h est dépassé.
    /// À appeler toutes les heures.
    pub async fn escalate_stale_sla(db: &PgPool) -> Result<u64, AppError> {
        let result = sqlx::query(
            r#"
            UPDATE review_tasks
            SET status = 'escalated',
                escalated_at = NOW(),
                updated_at = NOW()
            WHERE status IN ('open', 'claimed')
              AND sla_deadline < NOW()
            "#,
        )
        .execute(db)
        .await?;
        Ok(result.rows_affected())
    }

    /// Résout le domain d'une task pour un deliverable donné.
    ///
    /// Regarde la slice ou le challenge attaché. Utilisé par
    /// `create_task_for_deliverable` quand on ne connaît pas le domain à l'appel.
    pub async fn resolve_deliverable_domain(
        tx: &mut Transaction<'_, Postgres>,
        deliverable_id: Uuid,
    ) -> Result<String, AppError> {
        let row: Option<(Option<Uuid>, Option<Uuid>)> =
            sqlx::query_as("SELECT slice_id, challenge_id FROM deliverables WHERE id = $1")
                .bind(deliverable_id)
                .fetch_optional(&mut **tx)
                .await?;

        let Some((slice_id, challenge_id)) = row else {
            return Err(AppError::NotFound("Deliverable not found".to_string()));
        };

        if let Some(sid) = slice_id {
            let domain: Option<String> =
                sqlx::query_scalar("SELECT primary_domain FROM project_slices WHERE id = $1")
                    .bind(sid)
                    .fetch_optional(&mut **tx)
                    .await?;
            if let Some(d) = domain {
                return Ok(d);
            }
        }

        if let Some(cid) = challenge_id {
            let domain: Option<String> =
                sqlx::query_scalar("SELECT skill_domain FROM challenge_templates WHERE id = $1")
                    .bind(cid)
                    .fetch_optional(&mut **tx)
                    .await?;
            if let Some(d) = domain {
                return Ok(d);
            }
        }

        // Fallback safe
        Ok("code".to_string())
    }
}
