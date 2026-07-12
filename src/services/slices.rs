//! Service `project_slices` — unité de travail réelle sur un projet curated.
//!
//! Phase P1 du refactor challenges (voir `docs/challenges-target-model-and-roadmap.md`
//! partie C phase 1 et partie G.1 pour le workflow "PR mergée → deliverable").
//!
//! Ce service généralise le pattern éprouvé de `oss_bounties`/`oss_bounty_claims`.
//! Une slice est claimable exclusivement par un user (soft-lock 7 jours), et son
//! statut suit le lifecycle : `draft` → `open` → `claimed` → `in_review` → `merged`
//! (ou `expired` si claim non honorée).

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::ProjectSlice;

/// Durée pendant laquelle un claim est exclusif (7 jours, aligné pattern bounties).
pub const CLAIM_DURATION_DAYS: i64 = 7;

/// Service métier pour les slices.
///
/// N'a pas d'état côté Rust — c'est un namespace de fonctions qui opèrent sur
/// le PgPool. Suit la convention des autres services du projet.
pub struct SlicesService;

/// Filtres pour lister les slices ouvertes.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub domain: Option<String>,
    pub difficulty: Option<i16>,
    pub project_id: Option<Uuid>,
    pub page: i64,
    pub per_page: i64,
}

impl SlicesService {
    // ═══════════════════════════════════════════════════════════════════
    // Lectures (list, get)
    // ═══════════════════════════════════════════════════════════════════

    /// Liste les slices `status='open'` avec filtres.
    ///
    /// Ordre : difficulty ASC puis created_at DESC (les plus faciles d'abord,
    /// puis les plus récentes) — cohérent avec l'expérience d'entrée d'un
    /// nouveau contributeur qui cherche des tâches accessibles.
    pub async fn list_open(
        db: &PgPool,
        filter: &ListFilter,
    ) -> Result<(Vec<ProjectSlice>, i64), AppError> {
        let per_page = filter.per_page.clamp(1, 100);
        let page = filter.page.max(1);
        let offset = (page - 1) * per_page;

        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE status = 'open'
              AND ($1::text IS NULL OR primary_domain = $1)
              AND ($2::smallint IS NULL OR difficulty = $2)
              AND ($3::uuid IS NULL OR project_id = $3)
            ORDER BY difficulty ASC, created_at DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(&filter.domain)
        .bind(filter.difficulty)
        .bind(filter.project_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(db)
        .await?;

        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM project_slices
            WHERE status = 'open'
              AND ($1::text IS NULL OR primary_domain = $1)
              AND ($2::smallint IS NULL OR difficulty = $2)
              AND ($3::uuid IS NULL OR project_id = $3)
            "#,
        )
        .bind(&filter.domain)
        .bind(filter.difficulty)
        .bind(filter.project_id)
        .fetch_one(db)
        .await?;

        Ok((slices, total))
    }

    /// Récupère une slice par son id (peu importe le status — utile pour affichage).
    pub async fn get(db: &PgPool, slice_id: Uuid) -> Result<ProjectSlice, AppError> {
        sqlx::query_as::<_, ProjectSlice>(
            "SELECT * FROM project_slices WHERE id = $1",
        )
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Slice not found".to_string()))
    }

    /// Liste les slices claimed par un user (`in_progress` côté user).
    pub async fn list_claimed_by(
        db: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE claimed_by_user_id = $1
              AND status IN ('claimed', 'in_review')
            ORDER BY claim_expires_at ASC NULLS LAST, claimed_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;

        Ok(slices)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Mutations : claim, unclaim
    // ═══════════════════════════════════════════════════════════════════

    /// Claim une slice pour un user. Soft-lock exclusif pendant `CLAIM_DURATION_DAYS`.
    ///
    /// Erreurs :
    /// - `NotFound` si la slice n'existe pas ou n'est pas `open`
    /// - `Validation` si le user a déjà `max_concurrent_claims` slices actives
    ///   (Phase P1 : pas de limite. À réintroduire si besoin en Phase P3+)
    ///
    /// Retourne la slice mise à jour avec `claim_expires_at` calculé.
    pub async fn claim(
        db: &PgPool,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let expires_at = Utc::now() + Duration::days(CLAIM_DURATION_DAYS);

        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
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
        .bind(user_id)
        .bind(expires_at)
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "Slice is not available for claim (either not found, or already claimed / closed)"
                    .to_string(),
            )
        })?;

        Ok(slice)
    }

    /// Un user relâche sa slice. Elle retourne au pool `open`.
    ///
    /// Erreurs :
    /// - `Validation` si la slice n'est pas claimée par ce user
    pub async fn unclaim(
        db: &PgPool,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'open',
                claimed_by_user_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND claimed_by_user_id = $2
              AND status = 'claimed'
            RETURNING *
            "#,
        )
        .bind(slice_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "You can only unclaim your own claimed slices".to_string(),
            )
        })?;

        Ok(slice)
    }

    // ═══════════════════════════════════════════════════════════════════
    // P10.1 : claim en team (persistent) — alternative au claim solo user
    // ═══════════════════════════════════════════════════════════════════

    /// Claim une slice pour une team persistente. XOR avec le claim solo user.
    ///
    /// L'appelant doit être membre de la team (validation faite côté route).
    /// Erreurs : `Validation` si slice pas `open` ou team déjà claim ailleurs.
    pub async fn claim_as_team(
        db: &PgPool,
        slice_id: Uuid,
        team_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let expires_at = Utc::now() + Duration::days(CLAIM_DURATION_DAYS);

        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'claimed',
                claimed_by_team_id = $1,
                claimed_by_user_id = NULL,
                claimed_at = NOW(),
                claim_expires_at = $2,
                updated_at = NOW()
            WHERE id = $3
              AND status = 'open'
            RETURNING *
            "#,
        )
        .bind(team_id)
        .bind(expires_at)
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "Slice is not available for team claim (not found or already claimed / closed)"
                    .to_string(),
            )
        })?;

        Ok(slice)
    }

    /// Un membre de la team relâche le claim collectif de la slice.
    pub async fn unclaim_by_team(
        db: &PgPool,
        slice_id: Uuid,
        team_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'open',
                claimed_by_team_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND claimed_by_team_id = $2
              AND status = 'claimed'
            RETURNING *
            "#,
        )
        .bind(slice_id)
        .bind(team_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "This team does not currently claim this slice".to_string(),
            )
        })?;

        Ok(slice)
    }

    /// Slices claimed par une team (dashboard team).
    pub async fn list_claimed_by_team(
        db: &PgPool,
        team_id: Uuid,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE claimed_by_team_id = $1
              AND status IN ('claimed', 'in_review')
            ORDER BY claim_expires_at ASC NULLS LAST, claimed_at DESC
            "#,
        )
        .bind(team_id)
        .fetch_all(db)
        .await?;

        Ok(slices)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Maintenance : expire les claims dépassés (appelé par cron)
    // ═══════════════════════════════════════════════════════════════════

    /// Retourne au pool `open` les claims dont `claim_expires_at` est dépassé.
    ///
    /// Appelé par un cron (à définir en Phase P1 ou plus tard) toutes les heures.
    /// Retourne le nombre de slices remises au pool.
    ///
    /// Note (workflow W1) : la reco de la session 2026-07-09 mentionne une notif
    /// à J+5 puis prolongation manuelle par steward possible. Ce service ne gère
    /// que le hard expire à J+7 ; la notif J+5 est un cron séparé (Phase P2+).
    pub async fn expire_stale_claims(db: &PgPool) -> Result<u64, AppError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE project_slices
            SET status = 'open',
                claimed_by_user_id = NULL,
                claimed_by_team_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE status = 'claimed'
              AND claim_expires_at IS NOT NULL
              AND claim_expires_at < $1
            "#,
        )
        .bind(now)
        .execute(db)
        .await?;

        Ok(result.rows_affected())
    }

    /// Slices proches d'expirer (utile pour envoyer une notif J+5 au user
    /// et à son steward, workflow W1 reco session 2026-07-09).
    pub async fn find_expiring_within(
        db: &PgPool,
        within: Duration,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let deadline: DateTime<Utc> = Utc::now() + within;

        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE status = 'claimed'
              AND claim_expires_at IS NOT NULL
              AND claim_expires_at BETWEEN NOW() AND $1
            ORDER BY claim_expires_at ASC
            "#,
        )
        .bind(deadline)
        .fetch_all(db)
        .await?;

        Ok(slices)
    }
}
