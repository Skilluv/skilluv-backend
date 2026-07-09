//! Service `stewards` — gouvernance humaine des projets (Phase P6).
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections B.13, 9.5, 9.7.
//!
//! Le steward est le pivot humain de tout le pipeline (voir H.3). Sans stewards
//! actifs, un projet ne peut pas ingérer de slices, valider les drafts, arbitrer
//! les disputes, signer les attestations compagnonnage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub struct StewardsService;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectSteward {
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub appointed_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub appointed_by_user_id: Option<Uuid>,
}

impl StewardsService {
    /// Rôles valides pour un steward.
    pub const VALID_ROLES: &'static [&'static str] = &[
        "lead_steward",
        "co_steward",
        "domain_lead_code",
        "domain_lead_design",
        "domain_lead_sec",
        "domain_lead_game",
        "mediator",
    ];

    /// Nomme un user comme steward d'un projet.
    ///
    /// Idempotent : si le user occupe déjà ce rôle actif, no-op.
    pub async fn add(
        db: &PgPool,
        project_id: Uuid,
        user_id: Uuid,
        role: &str,
        appointed_by: Uuid,
    ) -> Result<ProjectSteward, AppError> {
        if !Self::VALID_ROLES.contains(&role) {
            return Err(AppError::Validation(format!(
                "Invalid steward role '{role}'; valid: {:?}",
                Self::VALID_ROLES
            )));
        }

        let steward = sqlx::query_as::<_, ProjectSteward>(
            r#"
            INSERT INTO project_stewards
                (project_id, user_id, role, appointed_by_user_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (project_id, user_id, role) DO UPDATE SET
                ended_at = NULL,
                appointed_at = CASE
                    WHEN project_stewards.ended_at IS NOT NULL THEN NOW()
                    ELSE project_stewards.appointed_at
                END,
                appointed_by_user_id = EXCLUDED.appointed_by_user_id
            RETURNING *
            "#,
        )
        .bind(project_id)
        .bind(user_id)
        .bind(role)
        .bind(appointed_by)
        .fetch_one(db)
        .await?;
        Ok(steward)
    }

    /// Retire un user d'un rôle steward (set ended_at).
    pub async fn remove(
        db: &PgPool,
        project_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE project_stewards
             SET ended_at = NOW()
             WHERE project_id = $1 AND user_id = $2 AND role = $3
               AND ended_at IS NULL",
        )
        .bind(project_id)
        .bind(user_id)
        .bind(role)
        .execute(db)
        .await?;
        Ok(())
    }

    /// Liste les stewards actifs d'un projet.
    pub async fn list_project_stewards(
        db: &PgPool,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSteward>, AppError> {
        let stewards = sqlx::query_as::<_, ProjectSteward>(
            "SELECT * FROM project_stewards
             WHERE project_id = $1 AND ended_at IS NULL
             ORDER BY appointed_at ASC",
        )
        .bind(project_id)
        .fetch_all(db)
        .await?;
        Ok(stewards)
    }

    /// Liste les projets où un user est steward actif.
    pub async fn list_user_stewardships(
        db: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<ProjectSteward>, AppError> {
        let stewardships = sqlx::query_as::<_, ProjectSteward>(
            "SELECT * FROM project_stewards
             WHERE user_id = $1 AND ended_at IS NULL
             ORDER BY appointed_at DESC",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;
        Ok(stewardships)
    }

    /// Helper : le user est-il steward actif de ce projet, peu importe le rôle ?
    ///
    /// Utilisé par les endpoints qui exigent le steward flag (ex: signature
    /// compagnonnage, publication d'une slice draft).
    pub async fn is_steward(
        db: &PgPool,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, AppError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM project_stewards
                WHERE project_id = $1 AND user_id = $2 AND ended_at IS NULL
            )",
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        Ok(exists)
    }
}
