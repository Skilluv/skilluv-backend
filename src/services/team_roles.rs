//! P10.2 — Service pour gérer les slots de rôles multidisciplinaires sur teams.
//!
//! Trois opérations principales :
//! - `create_slot` : un membre de la team définit un rôle à pourvoir.
//! - `fill_slot` : un user prend le slot (auto-join la team si nécessaire),
//!   avec validation du skill prérequis best-effort.
//! - `leave_slot` : le user libère son slot (reste éventuellement dans la team
//!   sans rôle assigné).

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::TeamRoleSlot;

pub struct TeamRolesService;

/// Paramètres de création d'un slot. `required_skill_slug` optionnel — si set,
/// on résout le skill_id via `skill_nodes.slug`.
#[derive(Debug, Clone)]
pub struct CreateSlotParams<'a> {
    pub team_id: Uuid,
    pub role_slug: &'a str,
    pub role_display_name: Option<&'a str>,
    pub required_skill_slug: Option<&'a str>,
    pub min_proficiency_level: i16,
}

impl TeamRolesService {
    /// Liste tous les slots d'une team (ouverts + remplis).
    pub async fn list_slots(
        db: &PgPool,
        team_id: Uuid,
    ) -> Result<Vec<TeamRoleSlot>, AppError> {
        let rows = sqlx::query_as::<_, TeamRoleSlot>(
            "SELECT * FROM team_role_slots
             WHERE team_id = $1
             ORDER BY (filled_by_user_id IS NULL) DESC, role_slug",
        )
        .bind(team_id)
        .fetch_all(db)
        .await?;
        Ok(rows)
    }

    /// Slots ouverts (non-remplis) filtré par rôle — marketplace « teams cherchent musicien ».
    pub async fn find_open_slots_by_role(
        db: &PgPool,
        role_slug: &str,
        limit: i64,
    ) -> Result<Vec<TeamRoleSlot>, AppError> {
        let rows = sqlx::query_as::<_, TeamRoleSlot>(
            "SELECT * FROM team_role_slots
             WHERE role_slug = $1 AND filled_by_user_id IS NULL
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(role_slug)
        .bind(limit.clamp(1, 100))
        .fetch_all(db)
        .await?;
        Ok(rows)
    }

    /// Crée un slot. `required_skill_slug` résolu vers un `skill_id` si présent.
    pub async fn create_slot(
        db: &PgPool,
        params: CreateSlotParams<'_>,
    ) -> Result<TeamRoleSlot, AppError> {
        if params.role_slug.trim().is_empty() || params.role_slug.len() > 60 {
            return Err(AppError::Validation(
                "role_slug must be between 2 and 60 characters".into(),
            ));
        }

        let required_skill_id: Option<Uuid> = if let Some(slug) = params.required_skill_slug {
            let id: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1")
                    .bind(slug)
                    .fetch_optional(db)
                    .await?;
            if id.is_none() {
                return Err(AppError::Validation(format!(
                    "required_skill_slug '{slug}' not found in skill_nodes"
                )));
            }
            id
        } else {
            None
        };

        let row = sqlx::query_as::<_, TeamRoleSlot>(
            r#"
            INSERT INTO team_role_slots
                (team_id, role_slug, role_display_name, required_skill_id, min_proficiency_level)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(params.team_id)
        .bind(params.role_slug)
        .bind(params.role_display_name)
        .bind(required_skill_id)
        .bind(params.min_proficiency_level.clamp(1, 5))
        .fetch_one(db)
        .await?;

        Ok(row)
    }

    /// Un user prend un slot. Best-effort validation :
    /// - Le slot doit être vide.
    /// - Si le slot a un `required_skill_id`, le user doit avoir
    ///   `user_skills.proficiency_level >= min_proficiency_level`.
    /// - Un user ne peut pas déjà occuper un slot dans la même team.
    /// - Le user devient membre de la team (INSERT ON CONFLICT sur team_members).
    pub async fn fill_slot(
        db: &PgPool,
        slot_id: Uuid,
        user_id: Uuid,
    ) -> Result<TeamRoleSlot, AppError> {
        let mut tx = db.begin().await?;

        // 1. Récupérer le slot pour valider skill + team_id + statut
        let slot: TeamRoleSlot = sqlx::query_as::<_, TeamRoleSlot>(
            "SELECT * FROM team_role_slots WHERE id = $1 FOR UPDATE",
        )
        .bind(slot_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Slot not found".into()))?;

        if slot.filled_by_user_id.is_some() {
            return Err(AppError::Validation("Slot already filled".into()));
        }

        // 2. Validation skill (best-effort, non bloquante si skill_nodes indispo)
        if let Some(required_skill_id) = slot.required_skill_id {
            let level: Option<i16> = sqlx::query_scalar(
                "SELECT proficiency_level FROM user_skills
                 WHERE user_id = $1 AND skill_id = $2",
            )
            .bind(user_id)
            .bind(required_skill_id)
            .fetch_optional(&mut *tx)
            .await?;
            let has_level = level.unwrap_or(0) >= slot.min_proficiency_level;
            if !has_level {
                return Err(AppError::Validation(format!(
                    "User does not meet the minimum proficiency (required level {}, has {})",
                    slot.min_proficiency_level,
                    level.unwrap_or(0)
                )));
            }
        }

        // 3. UNIQUE partial index refusera si le user occupe déjà un slot ici
        let filled: TeamRoleSlot = sqlx::query_as::<_, TeamRoleSlot>(
            r#"
            UPDATE team_role_slots
            SET filled_by_user_id = $1, filled_at = NOW()
            WHERE id = $2 AND filled_by_user_id IS NULL
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(slot_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::Validation("Slot no longer available".into()))?;

        // 4. Auto-join team_members si pas déjà membre
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(slot.team_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(filled)
    }

    /// Le user libère son slot (mais reste dans team_members).
    pub async fn leave_slot(
        db: &PgPool,
        slot_id: Uuid,
        user_id: Uuid,
    ) -> Result<TeamRoleSlot, AppError> {
        let slot: TeamRoleSlot = sqlx::query_as::<_, TeamRoleSlot>(
            r#"
            UPDATE team_role_slots
            SET filled_by_user_id = NULL, filled_at = NULL
            WHERE id = $1 AND filled_by_user_id = $2
            RETURNING *
            "#,
        )
        .bind(slot_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation("You are not the current holder of this slot".into())
        })?;
        Ok(slot)
    }

    /// Delete un slot vide (nettoyage par le créateur de la team).
    /// Refuse si le slot est déjà rempli — utiliser leave_slot d'abord.
    pub async fn delete_slot(db: &PgPool, slot_id: Uuid) -> Result<(), AppError> {
        let res = sqlx::query(
            "DELETE FROM team_role_slots WHERE id = $1 AND filled_by_user_id IS NULL",
        )
        .bind(slot_id)
        .execute(db)
        .await?;
        if res.rows_affected() == 0 {
            return Err(AppError::Validation(
                "Slot not found or currently filled".into(),
            ));
        }
        Ok(())
    }
}
