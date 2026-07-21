//! Service `seasons` — Phase P6.
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections B.13, 9.4.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub struct SeasonsService;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Season {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub theme: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
    pub retrospective_report_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSeasonParams {
    pub slug: String,
    pub name: String,
    pub theme: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

impl SeasonsService {
    pub async fn list_all(db: &PgPool) -> Result<Vec<Season>, AppError> {
        let seasons = sqlx::query_as::<_, Season>("SELECT * FROM seasons ORDER BY starts_at DESC")
            .fetch_all(db)
            .await?;
        Ok(seasons)
    }

    pub async fn get_by_slug(db: &PgPool, slug: &str) -> Result<Season, AppError> {
        sqlx::query_as::<_, Season>("SELECT * FROM seasons WHERE slug = $1")
            .bind(slug)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Season '{slug}' not found")))
    }

    /// Retourne la saison actuellement `status='active'` (au plus une à la fois).
    pub async fn get_current(db: &PgPool) -> Result<Option<Season>, AppError> {
        let season = sqlx::query_as::<_, Season>(
            "SELECT * FROM seasons
             WHERE status = 'active'
               AND starts_at <= NOW()
               AND ends_at >= NOW()
             ORDER BY starts_at DESC
             LIMIT 1",
        )
        .fetch_optional(db)
        .await?;
        Ok(season)
    }

    pub async fn create(db: &PgPool, params: CreateSeasonParams) -> Result<Season, AppError> {
        if params.ends_at <= params.starts_at {
            return Err(AppError::Validation(
                "ends_at must be strictly after starts_at".to_string(),
            ));
        }

        let season = sqlx::query_as::<_, Season>(
            r#"
            INSERT INTO seasons (slug, name, theme, starts_at, ends_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(&params.slug)
        .bind(&params.name)
        .bind(&params.theme)
        .bind(params.starts_at)
        .bind(params.ends_at)
        .fetch_one(db)
        .await?;
        Ok(season)
    }

    /// Bascule status → 'active' (et repasse les anciennes actives à 'completed').
    pub async fn activate(db: &PgPool, slug: &str) -> Result<Season, AppError> {
        let mut tx = db.begin().await?;

        sqlx::query(
            "UPDATE seasons SET status = 'completed'
             WHERE status = 'active' AND slug != $1",
        )
        .bind(slug)
        .execute(&mut *tx)
        .await?;

        let season = sqlx::query_as::<_, Season>(
            "UPDATE seasons SET status = 'active'
             WHERE slug = $1
             RETURNING *",
        )
        .bind(slug)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Season '{slug}' not found")))?;

        tx.commit().await?;
        Ok(season)
    }

    /// Assigne un projet à une saison (M2M project_seasons).
    pub async fn assign_project(
        db: &PgPool,
        season_id: Uuid,
        project_id: Uuid,
        focus_type: &str,
    ) -> Result<(), AppError> {
        if !matches!(focus_type, "primary" | "featured" | "sponsor") {
            return Err(AppError::Validation(
                "focus_type must be one of primary|featured|sponsor".to_string(),
            ));
        }
        sqlx::query(
            "INSERT INTO project_seasons (project_id, season_id, focus_type)
             VALUES ($1, $2, $3)
             ON CONFLICT (project_id, season_id) DO UPDATE SET focus_type = EXCLUDED.focus_type",
        )
        .bind(project_id)
        .bind(season_id)
        .bind(focus_type)
        .execute(db)
        .await?;
        Ok(())
    }

    /// Liste des projets rattachés à une saison, avec focus_type.
    pub async fn list_projects_in_season(
        db: &PgPool,
        season_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, AppError> {
        let rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT project_id, focus_type FROM project_seasons WHERE season_id = $1",
        )
        .bind(season_id)
        .fetch_all(db)
        .await?;
        Ok(rows)
    }
}
