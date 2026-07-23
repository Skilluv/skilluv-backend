//! P17.3 — Rules engine "proof-driven" pour badges.
//!
//! Le principe : à partir des `badge_rules` (JSONB conditions) et des preuves
//! immuables déjà en DB (`deliverables` verified, `attestations`), on calcule
//! quels badges un user mérite. Pas de compteur pré-agrégé — tout se dérive.
//!
//! Grammar des conditions JSONB supportée en v1 :
//!
//!   {
//!     "proof_types": ["deliverable_verified" | "attestation_received"
//!                     | "onboarding_bonjour_completed"],
//!     "min_count":   integer (obligatoire, default 1),
//!     "skill_tag":   "react"      // filtre : deliverables/attestations sur ce skill
//!                                   // (via user_skills touchées)
//!     "display_category": "craft" // filtre par catégorie UX (P17.2)
//!   }
//!
//! Le proof_type `onboarding_bonjour_completed` compte la ligne
//! `onboarding_bonjour_skilluv` du user si son `completed_at IS NOT NULL`.
//! Utilise pour ancrer la rule "Bonjour Skilluv" (1re contribution mergee).
//!
//! Grammar volontairement simple ; extensible en P17.4/5 (within_days,
//! quality thresholds, guild membership, etc.).
//!
//! Contrat de `recompute_badges_for_user` :
//!   - Pour chaque rule non-deprecated, évalue.
//!   - Si conditions remplies et le user n'a pas encore ce badge (par rule_id) :
//!     INSERT user_badges avec source_proofs = les preuves qui ont matché.
//!   - Si conditions plus remplies (preuve source révoquée) et user_badge existe
//!     non-révoqué : UPDATE revoked_at = NOW(), revoked_reason = 'conditions_no_longer_met'.

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Deserialize, Default, Clone)]
struct RuleConditions {
    #[serde(default)]
    proof_types: Vec<String>,
    #[serde(default = "one")]
    min_count: i64,
    #[serde(default)]
    skill_tag: Option<String>,
    #[serde(default)]
    display_category: Option<String>,
}
fn one() -> i64 {
    1
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RuleRow {
    id: Uuid,
    slug: String,
    output_type: String,
    conditions: serde_json::Value,
    rarity: String,
}

#[derive(Debug, Clone)]
pub struct RecomputeReport {
    pub awarded: Vec<String>, // slugs
    pub revoked: Vec<String>,
    pub unchanged: usize,
}

/// Compte les preuves qui matchent une rule pour un user donné.
/// Retourne (count, source_proof_ids limités à 25 pour la traçabilité).
async fn count_matching_proofs(
    db: &PgPool,
    user_id: Uuid,
    conds: &RuleConditions,
) -> Result<(i64, Vec<Uuid>), AppError> {
    let want_deliverable = conds.proof_types.is_empty()
        || conds
            .proof_types
            .iter()
            .any(|t| t == "deliverable_verified");
    let want_attestation = conds
        .proof_types
        .iter()
        .any(|t| t == "attestation_received");
    let want_onboarding_bonjour = conds
        .proof_types
        .iter()
        .any(|t| t == "onboarding_bonjour_completed");

    let mut total: i64 = 0;
    let mut sources: Vec<Uuid> = Vec::new();

    if want_deliverable {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT d.id
            FROM deliverables d
            LEFT JOIN slice_skills ss ON ss.slice_id = d.slice_id
            LEFT JOIN skill_nodes sn  ON sn.id = ss.skill_id
            WHERE d.user_id = $1
              AND d.verification_status = 'verified'
              AND ($2::VARCHAR IS NULL OR sn.slug = $2)
              AND ($3::VARCHAR IS NULL OR sn.display_category = $3)
            LIMIT 25
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_tag.as_deref())
        .bind(conds.display_category.as_deref())
        .fetch_all(db)
        .await?;
        total += ids.len() as i64;
        sources.extend(ids);
    }

    if want_attestation {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM attestations
            WHERE user_id = $1 AND revoked_at IS NULL
            LIMIT 25
            "#,
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;
        total += ids.len() as i64;
        sources.extend(ids);
    }

    if want_onboarding_bonjour {
        // La table a une PK sur user_id -> au plus 1 ligne. On compte 1 si
        // completed_at set, 0 sinon. La "source_proof" est l'user_id lui-meme
        // (pas d'id dedie car on utilise l'user_id comme PK).
        let count: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT 1::BIGINT FROM onboarding_bonjour_skilluv
            WHERE user_id = $1 AND completed_at IS NOT NULL
            "#,
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        if count.is_some() {
            total += 1;
            sources.push(user_id);
        }
    }

    Ok((total, sources))
}

/// Dérive la rareté effective en fonction du count matched si la rule est en 'auto'.
fn resolve_rarity(rule_rarity: &str, matched: i64) -> String {
    if rule_rarity != "auto" {
        return rule_rarity.to_string();
    }
    match matched {
        0..=4 => "common",
        5..=14 => "rare",
        15..=49 => "epic",
        _ => "legendary",
    }
    .to_string()
}

pub async fn recompute_badges_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<RecomputeReport, AppError> {
    // Récupère le badge_id "generic" pour l'INSERT (contrainte FK badges).
    // Legacy : chaque user_badge doit référencer un badge existant. Pour les
    // nouvelles rules qui n'ont pas de badge legacy, on utilise un badge
    // sentinel "proof_engine" (auto-créé au besoin).
    let sentinel_badge_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO badges (slug, name, description, icon, category, condition_type, condition_value)
        VALUES ('_proof_engine', 'Proof Engine badge', 'Managed by badge_rules', '_', 'special', 'derived', 0)
        ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        "#,
    )
    .fetch_one(db)
    .await?;

    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT id, slug, output_type, conditions, rarity
         FROM badge_rules WHERE deprecated_at IS NULL",
    )
    .fetch_all(db)
    .await?;

    let mut awarded = Vec::new();
    let mut revoked = Vec::new();
    let mut unchanged = 0usize;

    for rule in rules {
        let conds: RuleConditions =
            serde_json::from_value(rule.conditions.clone()).unwrap_or_default();
        let (count, sources) = count_matching_proofs(db, user_id, &conds).await?;
        let meets = count >= conds.min_count;
        let has: Option<(bool,)> = sqlx::query_as(
            "SELECT revoked_at IS NULL FROM user_badges
             WHERE user_id = $1 AND rule_id = $2 LIMIT 1",
        )
        .bind(user_id)
        .bind(rule.id)
        .fetch_optional(db)
        .await?;

        match (meets, has) {
            (true, Some((true,))) => unchanged += 1,
            (true, Some((false,))) => {
                sqlx::query(
                    "UPDATE user_badges
                     SET revoked_at = NULL, revoked_reason = NULL,
                         source_proofs = $3
                     WHERE user_id = $1 AND rule_id = $2",
                )
                .bind(user_id)
                .bind(rule.id)
                .bind(&sources)
                .execute(db)
                .await?;
                awarded.push(rule.slug.clone());
            }
            (true, None) => {
                let rarity = resolve_rarity(&rule.rarity, count);
                sqlx::query(
                    "INSERT INTO user_badges
                         (user_id, badge_id, rule_id, source_proofs, rarity)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(user_id)
                .bind(sentinel_badge_id)
                .bind(rule.id)
                .bind(&sources)
                .bind(&rarity)
                .execute(db)
                .await?;
                awarded.push(rule.slug.clone());
            }
            (false, Some((true,))) => {
                sqlx::query(
                    "UPDATE user_badges
                     SET revoked_at = NOW(),
                         revoked_reason = 'conditions_no_longer_met'
                     WHERE user_id = $1 AND rule_id = $2 AND revoked_at IS NULL",
                )
                .bind(user_id)
                .bind(rule.id)
                .execute(db)
                .await?;
                revoked.push(rule.slug.clone());
            }
            (false, _) => {}
        }
    }

    Ok(RecomputeReport {
        awarded,
        revoked,
        unchanged,
    })
}
