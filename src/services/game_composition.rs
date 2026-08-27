//! What a game project is made of (W-03).
//!
//! A game is rarely one artefact. A finished one is code and art and a level
//! and sound, often from more than one person. This is the read model that
//! assembles that picture from the verified deliverables on a project's game
//! slices: which artefact subtypes shipped, who shipped them, whether it is a
//! full multi-craft game, and whether it was made by a team.
//!
//! The `game_multi_artefact_ship` and `game_team_ship` badges read the same
//! facts straight from SQL in the badge engine; this module is the shape a
//! route hands the front end for the "assembled game" view, so the two never
//! answer differently — both count verified game deliverables and nothing else.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The same threshold the badge engine uses, re-exported so the read model and
/// the badge agree on what "multi-artefact" means.
pub use crate::services::badge_engine::MULTI_ARTEFACT_MIN;

#[derive(Debug, Clone, Serialize)]
pub struct Composition {
    pub project_id: Uuid,
    /// The distinct game artefact subtypes that have shipped (verified), sorted.
    pub subtypes_shipped: Vec<String>,
    /// The distinct people whose verified game work is in the project.
    pub contributors: Vec<Uuid>,
    /// A full game built from more than one craft.
    pub is_multi_artefact: bool,
    /// Owned by a guild — made by a team.
    pub is_team: bool,
}

/// Assemble the composition of one project's shipped game work.
pub async fn composition_of(db: &PgPool, project_id: Uuid) -> Result<Composition, AppError> {
    let subtypes_shipped: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ps.game_artifact_subtype
          FROM deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
         WHERE ps.project_id = $1
           AND ps.slice_type = 'game_artifact'
           AND ps.game_artifact_subtype IS NOT NULL
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
         ORDER BY ps.game_artifact_subtype
        "#,
    )
    .bind(project_id)
    .fetch_all(db)
    .await?;

    let contributors: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT d.user_id
          FROM deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
         WHERE ps.project_id = $1
           AND ps.slice_type = 'game_artifact'
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
         ORDER BY d.user_id
        "#,
    )
    .bind(project_id)
    .fetch_all(db)
    .await?;

    let is_team: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM projects WHERE id = $1 AND owner_type = 'guild')",
    )
    .bind(project_id)
    .fetch_one(db)
    .await?;

    Ok(Composition {
        is_multi_artefact: subtypes_shipped.len() as i64 >= MULTI_ARTEFACT_MIN,
        is_team,
        subtypes_shipped,
        contributors,
        project_id,
    })
}
