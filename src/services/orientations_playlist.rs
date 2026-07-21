//! P16.5 — Playlist d'onboarding pour une orientation.
//!
//! Quand un user vient de choisir une orientation (P16.3), on veut lui
//! proposer immédiatement une "prochaine étape" pertinente : un mini-parcours
//! composé de :
//!   - 3 challenges training tagués sur les domaines couverts par l'orientation
//!   - des open team-role-slots dont le `required_skill_id` correspond à
//!     un skill core de l'orientation
//!
//! On ne recommande jamais de challenge sur lequel le user a déjà un
//! deliverable verified (pas de redite). On priorise les templates récents
//! `is_training=TRUE` et `status='published'`.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlaylistChallenge {
    pub id: Uuid,
    pub title: String,
    pub skill_domain: String,
    pub difficulty: i16,
    pub is_training: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlaylistTeamSlot {
    pub slot_id: Uuid,
    pub team_id: Uuid,
    pub team_name: String,
    pub role_slug: String,
    pub role_display_name: Option<String>,
    pub skill_slug: String,
    pub min_proficiency_level: i16,
}

#[derive(Debug, Serialize)]
pub struct Playlist {
    pub orientation_slug: String,
    pub training_challenges: Vec<PlaylistChallenge>,
    pub open_team_slots: Vec<PlaylistTeamSlot>,
}

pub async fn playlist_for(
    db: &PgPool,
    user_id: Uuid,
    orientation_slug: &str,
) -> Result<Playlist, AppError> {
    // 1. Résoudre l'orientation + ses domaines (primary + secondary) + skills core.
    let ori: Option<(Uuid, String, Vec<String>)> = sqlx::query_as(
        "SELECT id, primary_domain, secondary_domains FROM orientations WHERE slug = $1",
    )
    .bind(orientation_slug)
    .fetch_optional(db)
    .await?;

    let (ori_id, primary_domain, secondary_domains) = ori
        .ok_or_else(|| AppError::NotFound(format!("orientation '{orientation_slug}' not found")))?;

    let mut all_domains = vec![primary_domain];
    all_domains.extend(secondary_domains);

    // 2. Skills core de l'orientation (pour les slots).
    let core_skill_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT skill_id FROM orientation_skill_map
         WHERE orientation_id = $1 AND is_core = TRUE",
    )
    .bind(ori_id)
    .fetch_all(db)
    .await?;

    // 3. 3 training challenges tagués sur ces domaines, non déjà verified par l'user.
    let training_challenges: Vec<PlaylistChallenge> = sqlx::query_as(
        r#"
        SELECT ct.id, ct.title, ct.skill_domain, ct.difficulty, ct.is_training
        FROM challenge_templates ct
        WHERE ct.is_training = TRUE
          AND ct.status = 'published'
          AND ct.skill_domain = ANY($1::TEXT[])
          AND NOT EXISTS (
              SELECT 1 FROM deliverables d
              WHERE d.challenge_id = ct.id
                AND d.user_id = $2
                AND d.verification_status = 'verified'
          )
        ORDER BY ct.difficulty ASC, ct.created_at DESC
        LIMIT 3
        "#,
    )
    .bind(&all_domains)
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // 4. Open team slots exigeant un skill core.
    let open_slots: Vec<PlaylistTeamSlot> = if core_skill_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            r#"
            SELECT s.id          AS slot_id,
                   t.id          AS team_id,
                   t.name        AS team_name,
                   s.role_slug   AS role_slug,
                   s.role_display_name AS role_display_name,
                   sn.slug       AS skill_slug,
                   s.min_proficiency_level AS min_proficiency_level
            FROM team_role_slots s
            JOIN challenge_teams t ON t.id = s.team_id
            JOIN skill_nodes sn    ON sn.id = s.required_skill_id
            WHERE s.filled_by_user_id IS NULL
              AND s.required_skill_id = ANY($1::UUID[])
              AND t.created_by <> $2
            ORDER BY s.created_at DESC
            LIMIT 5
            "#,
        )
        .bind(&core_skill_ids)
        .bind(user_id)
        .fetch_all(db)
        .await?
    };

    Ok(Playlist {
        orientation_slug: orientation_slug.to_string(),
        training_challenges,
        open_team_slots: open_slots,
    })
}
