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

/// P15.3 — Vue enrichie d'un slot pour le marketplace public.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct MarketplaceSlot {
    pub slot_id: Uuid,
    pub role_slug: String,
    pub role_display_name: Option<String>,
    pub min_proficiency_level: i16,
    pub required_skill_id: Option<Uuid>,
    pub required_skill_slug: Option<String>,
    pub team_id: Uuid,
    pub team_name: String,
    pub challenge_id: Uuid,
    pub challenge_title: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

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

    /// P15.3 — Marketplace : liste globale des slots ouverts, enrichis avec
    /// team.name + challenge_template.title, filtrable par rôle et skill.
    pub async fn marketplace_open_slots(
        db: &PgPool,
        role_filter: Option<&str>,
        skill_slug_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<MarketplaceSlot>, AppError> {
        let rows = sqlx::query_as::<_, MarketplaceSlot>(
            r#"
            SELECT
                s.id                        AS slot_id,
                s.role_slug                 AS role_slug,
                s.role_display_name         AS role_display_name,
                s.min_proficiency_level     AS min_proficiency_level,
                s.required_skill_id         AS required_skill_id,
                sn.slug                     AS required_skill_slug,
                t.id                        AS team_id,
                t.name                      AS team_name,
                ct.id                       AS challenge_id,
                ct.title                    AS challenge_title,
                s.created_at                AS created_at
            FROM team_role_slots s
            JOIN challenge_teams t   ON t.id = s.team_id
            JOIN challenge_templates ct ON ct.id = t.challenge_id
            LEFT JOIN skill_nodes sn ON sn.id = s.required_skill_id
            WHERE s.filled_by_user_id IS NULL
              AND ($1::VARCHAR IS NULL OR s.role_slug = $1)
              AND ($2::VARCHAR IS NULL OR sn.slug = $2)
            ORDER BY s.created_at DESC
            LIMIT $3
            "#,
        )
        .bind(role_filter)
        .bind(skill_slug_filter)
        .bind(limit.clamp(1, 100))
        .fetch_all(db)
        .await?;
        Ok(rows)
    }

    /// P15.3 — Notifie les users éligibles (skill + proficiency match) qu'un
    /// slot vient d'être ouvert. Insère une ligne dans `notifications` par user.
    /// Best-effort : push mobile via `push_to_user_mobile` (silencieux si absent).
    /// Retourne le nombre de users notifiés.
    pub async fn notify_eligible_users_for_slot(
        db: &PgPool,
        slot_id: Uuid,
    ) -> Result<usize, AppError> {
        let slot: Option<TeamRoleSlot> = sqlx::query_as::<_, TeamRoleSlot>(
            "SELECT * FROM team_role_slots WHERE id = $1",
        )
        .bind(slot_id)
        .fetch_optional(db)
        .await?;
        let Some(slot) = slot else {
            return Ok(0);
        };
        let Some(required_skill_id) = slot.required_skill_id else {
            // Pas de skill requis → pas de targeting précis, on ne spamme pas.
            return Ok(0);
        };

        let team_row: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT t.name, t.challenge_id
             FROM challenge_teams t WHERE t.id = $1",
        )
        .bind(slot.team_id)
        .fetch_optional(db)
        .await?;
        let (team_name, challenge_id) =
            team_row.unwrap_or_else(|| ("(team)".into(), Uuid::nil()));
        let challenge_title: String = sqlx::query_scalar(
            "SELECT title FROM challenge_templates WHERE id = $1",
        )
        .bind(challenge_id)
        .fetch_optional(db)
        .await?
        .unwrap_or_else(|| "(challenge)".into());

        let user_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM user_skills
             WHERE skill_id = $1 AND proficiency_level >= $2
             LIMIT 500",
        )
        .bind(required_skill_id)
        .bind(slot.min_proficiency_level)
        .fetch_all(db)
        .await?;

        let title = format!("New team slot: {}", slot.role_slug);
        let body = format!(
            "\"{team_name}\" is looking for a {} on challenge {challenge_title}",
            slot.role_display_name.clone().unwrap_or_else(|| slot.role_slug.clone())
        );
        let data = serde_json::json!({
            "kind": "team_slot_open",
            "slot_id": slot.id,
            "team_id": slot.team_id,
            "role_slug": slot.role_slug,
        });

        let mut notified = 0usize;
        for uid in &user_ids {
            let inserted = sqlx::query(
                "INSERT INTO notifications (user_id, notification_type, title, body, data)
                 VALUES ($1, 'team_slot_open', $2, $3, $4)",
            )
            .bind(uid)
            .bind(&title)
            .bind(&body)
            .bind(&data)
            .execute(db)
            .await;
            if inserted.is_ok() {
                notified += 1;
                let msg = crate::services::mobile_push::MobilePushMessage {
                    title: &title,
                    body: &body,
                    data: Some(data.clone()),
                };
                let _ = crate::services::mobile_push::push_to_user_mobile(db, *uid, msg).await;
            }
        }

        metrics::counter!("skilluv_team_slot_notifications_total")
            .increment(notified as u64);
        Ok(notified)
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
