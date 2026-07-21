//! Service `attestations` — Killer feature de Skilluv (Phase P5 LAUNCH).
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections B.12, G.3, 6.3-6.5.
//!
//! Trois types :
//! - gesture : auto-issue quand proficiency_level d'un skill passe à 2
//! - skill : auto-issue quand level ≥ 4 + au moins un review par un sénior
//! - compagnonnage : manuel, signé par un steward de projet
//!
//! Anti-double-issue via UNIQUE indexes SQL sur linked_skill_node_ids.
//! Révocation propagée quand un deliverable sous-jacent est révoqué.

use base32::Alphabet;
use chrono::{DateTime, Utc};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;

/// Seuil de réputation pour être considéré "sénior" reviewer.
/// Décision G.3 : level 4 + au moins un review approve par un reviewer ≥ 0.7.
pub const SENIOR_REVIEWER_REPUTATION_THRESHOLD: f64 = 0.7;

pub struct AttestationsService;

/// Ligne attestations (calquée sur le schéma SQL).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Attestation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub attestation_type: String,
    pub title: String,
    pub description: String,
    pub icon: Option<String>,
    pub linked_deliverable_ids: Vec<Uuid>,
    pub linked_skill_node_ids: Vec<Uuid>,
    pub linked_project_ids: Vec<Uuid>,
    pub linked_reviewer_ids: Vec<Uuid>,
    pub issued_by_type: String,
    pub issued_by_org_id: Option<Uuid>,
    pub verification_code: String,
    pub public: bool,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by_user_id: Option<Uuid>,
    pub revoke_reason: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Paramètres pour créer une attestation compagnonnage (manuelle par steward).
#[derive(Debug, Clone, Deserialize)]
pub struct CompagnonnageParams {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: String,
    pub linked_deliverable_ids: Vec<Uuid>,
    pub linked_skill_node_ids: Vec<Uuid>,
}

impl AttestationsService {
    // ═══════════════════════════════════════════════════════════════════
    // Génération du verification_code
    // ═══════════════════════════════════════════════════════════════════

    /// Génère un code base32 de 10 caractères pour l'URL publique de vérification.
    /// 50 bits d'entropie (~10^15 combinaisons, quasi zéro collision).
    fn generate_verification_code() -> String {
        let mut bytes = [0u8; 8];
        rand_core::OsRng.fill_bytes(&mut bytes);
        let encoded = base32::encode(Alphabet::Rfc4648 { padding: false }, &bytes);
        encoded.chars().take(10).collect()
    }

    // ═══════════════════════════════════════════════════════════════════
    // Auto-issue déclenché après un level-up de skill
    // ═══════════════════════════════════════════════════════════════════

    /// Vérifie et émet les attestations éligibles suite à un level-up de skill.
    ///
    /// Appelé depuis DeliverablesService::propagate_skills et
    /// ReviewsService::propagate_skills, dans la transaction du verified.
    ///
    /// Retourne les IDs des attestations nouvellement créées (potentiellement
    /// 0, 1 ou 2 selon les seuils atteints).
    pub async fn check_and_issue_for_skill_levelup(
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        skill_id: Uuid,
        new_proficiency_level: i16,
    ) -> Result<Vec<Uuid>, AppError> {
        let mut issued_ids = Vec::new();

        // 1. Gesture : proficiency ≥ 2, un par skill
        if new_proficiency_level >= 2
            && let Some(id) = Self::try_issue_gesture(tx, user_id, skill_id).await?
        {
            issued_ids.push(id);
        }

        // 2. Skill : proficiency ≥ 4 + au moins une review par un sénior
        if new_proficiency_level >= 4
            && let Some(id) = Self::try_issue_skill(tx, user_id, skill_id).await?
        {
            issued_ids.push(id);
        }

        Ok(issued_ids)
    }

    async fn try_issue_gesture(
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        skill_id: Uuid,
    ) -> Result<Option<Uuid>, AppError> {
        // Anti-double : le UNIQUE index sur (user_id, type, linked_skill_node_ids)
        // where revoked_at IS NULL bloque déjà une seconde émission active.
        // On check d'abord pour éviter le ON CONFLICT bruyant.
        let already_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM attestations
                WHERE user_id = $1
                  AND attestation_type = 'gesture'
                  AND $2 = ANY(linked_skill_node_ids)
                  AND revoked_at IS NULL
            )",
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_one(&mut **tx)
        .await?;

        if already_exists {
            return Ok(None);
        }

        // Récupérer le skill pour construire title/description
        let (skill_slug, skill_display_name): (String, String) =
            sqlx::query_as("SELECT slug, display_name FROM skill_nodes WHERE id = $1")
                .bind(skill_id)
                .fetch_one(&mut **tx)
                .await?;

        // Top 3 preuves : deliverables du user qui touchent ce skill
        let top_proofs: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT d.id
            FROM deliverables d
            WHERE d.user_id = $1
              AND d.verification_status = 'verified'
              AND d.revoked_at IS NULL
              AND (
                  d.slice_id IN (SELECT slice_id FROM slice_skills WHERE skill_id = $2)
              )
            ORDER BY d.id
            LIMIT 3
            "#,
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_all(&mut **tx)
        .await?;

        let code = Self::generate_verification_code();
        let title = format!("Sait {}", &skill_display_name);
        let description = format!(
            "A démontré compétence sur \"{}\" ({}) via ses contributions vérifiées.",
            skill_display_name, skill_slug
        );

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO attestations (
                user_id, attestation_type, title, description,
                linked_skill_node_ids, linked_deliverable_ids,
                verification_code
            )
            VALUES ($1, 'gesture', $2, $3, ARRAY[$4], $5, $6)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(&title)
        .bind(&description)
        .bind(skill_id)
        .bind(&top_proofs)
        .bind(&code)
        .fetch_one(&mut **tx)
        .await?;

        Ok(Some(id))
    }

    async fn try_issue_skill(
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        skill_id: Uuid,
    ) -> Result<Option<Uuid>, AppError> {
        let already_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM attestations
                WHERE user_id = $1
                  AND attestation_type = 'skill'
                  AND $2 = ANY(linked_skill_node_ids)
                  AND revoked_at IS NULL
            )",
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_one(&mut **tx)
        .await?;

        if already_exists {
            return Ok(None);
        }

        // Vérifier qu'au moins un deliverable du user (touchant ce skill) a été
        // reviewé par un reviewer sénior (reputation ≥ 0.7).
        let senior_reviewer_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT r.reviewer_user_id
            FROM reviews r
            JOIN deliverables d ON d.id = r.deliverable_id
            LEFT JOIN review_metrics rm ON rm.reviewer_user_id = r.reviewer_user_id
            WHERE d.user_id = $1
              AND d.verification_status = 'verified'
              AND d.revoked_at IS NULL
              AND r.verdict = 'approve'
              AND (
                  d.slice_id IN (SELECT slice_id FROM slice_skills WHERE skill_id = $2)
              )
              AND COALESCE(rm.reputation_score, 0.5) >= $3
            "#,
        )
        .bind(user_id)
        .bind(skill_id)
        .bind(SENIOR_REVIEWER_REPUTATION_THRESHOLD)
        .fetch_all(&mut **tx)
        .await?;

        if senior_reviewer_ids.is_empty() {
            // Pas encore de review sénior : on n'émet pas la skill attestation.
            // Elle sera émise plus tard quand un sénior aura reviewé une contribution.
            return Ok(None);
        }

        let (_, skill_display_name): (String, String) =
            sqlx::query_as("SELECT slug, display_name FROM skill_nodes WHERE id = $1")
                .bind(skill_id)
                .fetch_one(&mut **tx)
                .await?;

        let top_proofs: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT d.id
            FROM deliverables d
            WHERE d.user_id = $1
              AND d.verification_status = 'verified'
              AND d.revoked_at IS NULL
              AND d.slice_id IN (SELECT slice_id FROM slice_skills WHERE skill_id = $2)
            ORDER BY d.id
            LIMIT 5
            "#,
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_all(&mut **tx)
        .await?;

        let code = Self::generate_verification_code();
        let title = format!("Maîtrise de {}", &skill_display_name);
        let description = format!(
            "A démontré maîtrise avancée (niveau 4+) sur \"{}\", validée par {} reviewer(s) sénior(s).",
            skill_display_name,
            senior_reviewer_ids.len()
        );

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO attestations (
                user_id, attestation_type, title, description,
                linked_skill_node_ids, linked_deliverable_ids, linked_reviewer_ids,
                verification_code
            )
            VALUES ($1, 'skill', $2, $3, ARRAY[$4], $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(&title)
        .bind(&description)
        .bind(skill_id)
        .bind(&top_proofs)
        .bind(&senior_reviewer_ids)
        .bind(&code)
        .fetch_one(&mut **tx)
        .await?;

        Ok(Some(id))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Compagnonnage (manuel par steward)
    // ═══════════════════════════════════════════════════════════════════

    /// Pré-check : le user est-il éligible à une attestation compagnonnage
    /// sur un projet donné ?
    ///
    /// Critères (G.3) :
    /// 1. Le user a ≥ 5 deliverables verifiés sur ce projet
    /// 2. Ces deliverables ont couvert ≥ 3 skills différents avec weight ≥ 3
    /// 3. Le projet est en lifecycle_status 'mature' ou 'graduated'
    /// 4. Pas déjà d'attestation compagnonnage active sur ce projet pour ce user
    pub async fn check_compagnonnage_eligibility(
        db: &PgPool,
        user_id: Uuid,
        project_id: Uuid,
    ) -> Result<bool, AppError> {
        let counts: (i64, i64, Option<String>) = sqlx::query_as(
            r#"
            SELECT
                (SELECT COUNT(*) FROM deliverables d
                 JOIN project_slices ps ON ps.id = d.slice_id
                 WHERE d.user_id = $1 AND ps.project_id = $2
                   AND d.verification_status = 'verified' AND d.revoked_at IS NULL),
                (SELECT COUNT(DISTINCT ss.skill_id) FROM deliverables d
                 JOIN project_slices ps ON ps.id = d.slice_id
                 JOIN slice_skills ss ON ss.slice_id = ps.id AND ss.weight >= 3
                 WHERE d.user_id = $1 AND ps.project_id = $2
                   AND d.verification_status = 'verified' AND d.revoked_at IS NULL),
                (SELECT lifecycle_status FROM projects WHERE id = $2)
            "#,
        )
        .bind(user_id)
        .bind(project_id)
        .fetch_one(db)
        .await?;

        let (deliverable_count, distinct_skills_count, lifecycle) = counts;

        if deliverable_count < 5 {
            return Ok(false);
        }
        if distinct_skills_count < 3 {
            return Ok(false);
        }
        let Some(lc) = lifecycle else {
            return Ok(false);
        };
        if !matches!(lc.as_str(), "mature" | "graduated") {
            return Ok(false);
        }

        // Pas déjà d'attestation compagnonnage active
        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM attestations
                WHERE user_id = $1 AND attestation_type = 'compagnonnage'
                  AND $2 = ANY(linked_project_ids) AND revoked_at IS NULL
            )",
        )
        .bind(user_id)
        .bind(project_id)
        .fetch_one(db)
        .await?;

        Ok(!already)
    }

    /// Émission manuelle d'un compagnonnage par un steward.
    ///
    /// Retourne l'id de l'attestation créée. Vérifie l'éligibilité en amont.
    pub async fn issue_compagnonnage(
        db: &PgPool,
        steward_user_id: Uuid,
        params: CompagnonnageParams,
    ) -> Result<Uuid, AppError> {
        // Pré-check éligibilité
        if !Self::check_compagnonnage_eligibility(db, params.user_id, params.project_id).await? {
            return Err(AppError::Validation(
                "User is not eligible for compagnonnage attestation on this project".to_string(),
            ));
        }

        let code = Self::generate_verification_code();
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO attestations (
                user_id, attestation_type, title, description,
                linked_project_ids, linked_deliverable_ids, linked_skill_node_ids,
                linked_reviewer_ids, verification_code
            )
            VALUES ($1, 'compagnonnage', $2, $3, ARRAY[$4], $5, $6, ARRAY[$7], $8)
            RETURNING id
            "#,
        )
        .bind(params.user_id)
        .bind(&params.title)
        .bind(&params.description)
        .bind(params.project_id)
        .bind(&params.linked_deliverable_ids)
        .bind(&params.linked_skill_node_ids)
        .bind(steward_user_id)
        .bind(&code)
        .fetch_one(db)
        .await?;
        Ok(id)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Lecture publique
    // ═══════════════════════════════════════════════════════════════════

    /// Portfolio public d'attestations d'un user.
    pub async fn list_public_by_user(
        db: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<Attestation>, AppError> {
        let attestations = sqlx::query_as::<_, Attestation>(
            r#"
            SELECT * FROM attestations
            WHERE user_id = $1
              AND public = TRUE
              AND revoked_at IS NULL
            ORDER BY issued_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;
        Ok(attestations)
    }

    /// Vérification publique par code (URL /attestations/verify/{code}).
    /// Retourne Some(attestation) si trouvée + valide, None sinon.
    pub async fn verify_by_code(db: &PgPool, code: &str) -> Result<Option<Attestation>, AppError> {
        let attestation = sqlx::query_as::<_, Attestation>(
            "SELECT * FROM attestations WHERE verification_code = $1 AND public = TRUE",
        )
        .bind(code)
        .fetch_optional(db)
        .await?;
        Ok(attestation)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Révocation
    // ═══════════════════════════════════════════════════════════════════

    /// Révoque une attestation. Audit trail préservé.
    ///
    /// Appelée :
    /// - Manuellement par admin
    /// - Automatiquement quand un deliverable sous-jacent est révoqué
    ///   (voir `revoke_attestations_depending_on_deliverable`)
    pub async fn revoke(
        db: &PgPool,
        attestation_id: Uuid,
        revoked_by: Option<Uuid>,
        reason: String,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE attestations
             SET revoked_at = NOW(),
                 revoked_by_user_id = $1,
                 revoke_reason = $2
             WHERE id = $3 AND revoked_at IS NULL",
        )
        .bind(revoked_by)
        .bind(reason)
        .bind(attestation_id)
        .execute(db)
        .await?;
        Ok(())
    }

    /// Cascade révocation : quand un deliverable est révoqué, on révoque toutes
    /// les attestations qui listaient ce deliverable dans leurs preuves.
    pub async fn revoke_attestations_depending_on_deliverable(
        tx: &mut Transaction<'_, Postgres>,
        deliverable_id: Uuid,
    ) -> Result<u64, AppError> {
        let result = sqlx::query(
            r#"
            UPDATE attestations
            SET revoked_at = NOW(),
                revoke_reason = 'underlying_deliverable_revoked'
            WHERE $1 = ANY(linked_deliverable_ids) AND revoked_at IS NULL
            "#,
        )
        .bind(deliverable_id)
        .execute(&mut **tx)
        .await?;
        Ok(result.rows_affected())
    }
}
