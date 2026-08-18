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
//!                     | "onboarding_bonjour_completed"
//!                     | "slice_merged_upstream" | "deliverable_featured"],
//!     "min_count":   integer (obligatoire, default 1),
//!     "skill_tag":   "react"      // filtre : deliverables/attestations sur ce skill
//!                                   // (via user_skills touchées)
//!     "display_category": "craft" // filtre par catégorie UX (P17.2)
//!     "skill_domain": "code"      // filtre : domaine du challenge derrière la preuve
//!     "distinct_over": "challenge_language" // compte des valeurs distinctes,
//!                                   // pas des preuves : "trois langages" et non
//!                                   // "trois livrables"
//!     "attestation_basis": "ai_model_shipped" // filtre : ce sur quoi
//!                                   // l'attestation se fonde. Rend comptable
//!                                   // ce qui était une appréciation.
//!     "mission_completed"           // proof_type : une mission cloturee.
//!                                   // Le domaine est porte par la mission
//!                                   // elle-meme, pas par un challenge.
//!     "manual": true              // le moteur n'attribue jamais : un opérateur
//!                                   // décide, et la raison est enregistrée
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
    /// Domain of the challenge behind the proof. A code badge should not be
    /// awarded for a design deliverable.
    #[serde(default)]
    skill_domain: Option<String>,
    /// Count distinct values of a dimension instead of counting proofs.
    /// "three languages" is not "three deliverables".
    #[serde(default)]
    distinct_over: Option<String>,
    /// What the attestation rests on — `ai_model_shipped`,
    /// `code_library_published`. Only meaningful alongside the
    /// `attestation_received` proof type.
    ///
    /// This is what makes "shipped a model" countable rather than a
    /// judgement: the basis is a recorded value, so the engine reads it
    /// instead of an operator deciding. It carries the domain in its own
    /// name, which is why `skill_domain` is not also applied to attestations
    /// — they link deliverables, not challenges, and there is no domain on
    /// them to filter by.
    #[serde(default)]
    attestation_basis: Option<String>,
    /// The engine never awards this one. Some distinctions are judgements —
    /// "shipped an audited contract to mainnet" is not a row count — and
    /// inventing a rule for them would award them to the wrong people.
    #[serde(default)]
    manual: bool,
}
fn one() -> i64 {
    1
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RuleRow {
    id: Uuid,
    slug: String,
    /// Selected for future rules that dispatch on the output type (e.g. XP
    /// vs badge). Not read today ; kept in the row to avoid a schema
    /// migration if we start needing it.
    #[allow(dead_code)]
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
    let want_merged_upstream = conds
        .proof_types
        .iter()
        .any(|t| t == "slice_merged_upstream");
    let want_mission_completed = conds.proof_types.iter().any(|t| t == "mission_completed");
    // A variant of the deliverable count rather than a source of its own:
    // being featured is a property of a deliverable, not a different proof.
    let featured_only = conds
        .proof_types
        .iter()
        .any(|t| t == "deliverable_featured");
    let want_deliverable = want_deliverable || featured_only;

    // Counting distinct values answers a different question, and answers it
    // on its own: "three languages" is satisfied by three deliverables in
    // three languages, not by thirty in one.
    if let Some(dimension) = conds.distinct_over.as_deref() {
        return count_distinct_dimension(db, user_id, conds, dimension).await;
    }

    let mut total: i64 = 0;
    let mut sources: Vec<Uuid> = Vec::new();

    if want_deliverable {
        // Counted and sampled separately. They used to be the same query
        // with `LIMIT 25`, which capped the count at twenty-five and made
        // every rule above that threshold unreachable — the badge existed,
        // the condition was met, and nothing ever fired.
        let matched: i64 = sqlx::query_scalar(
            r#"
            SELECT count(DISTINCT d.id)
            FROM deliverables d
            LEFT JOIN slice_skills ss ON ss.slice_id = d.slice_id
            LEFT JOIN skill_nodes sn  ON sn.id = ss.skill_id
            LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
            WHERE d.user_id = $1
              AND d.verification_status = 'verified'
              AND d.revoked_at IS NULL
              AND ($2::VARCHAR IS NULL OR sn.slug = $2)
              AND ($3::VARCHAR IS NULL OR sn.display_category = $3)
              AND ($4::VARCHAR IS NULL OR ct.skill_domain = $4)
              AND ($5::BOOLEAN IS FALSE OR d.featured)
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_tag.as_deref())
        .bind(conds.display_category.as_deref())
        .bind(conds.skill_domain.as_deref())
        .bind(featured_only)
        .fetch_one(db)
        .await?;

        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT d.id
            FROM deliverables d
            LEFT JOIN slice_skills ss ON ss.slice_id = d.slice_id
            LEFT JOIN skill_nodes sn  ON sn.id = ss.skill_id
            LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
            WHERE d.user_id = $1
              AND d.verification_status = 'verified'
              AND d.revoked_at IS NULL
              AND ($2::VARCHAR IS NULL OR sn.slug = $2)
              AND ($3::VARCHAR IS NULL OR sn.display_category = $3)
              AND ($4::VARCHAR IS NULL OR ct.skill_domain = $4)
              AND ($5::BOOLEAN IS FALSE OR d.featured)
            LIMIT 25
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_tag.as_deref())
        .bind(conds.display_category.as_deref())
        .bind(conds.skill_domain.as_deref())
        .bind(featured_only)
        .fetch_all(db)
        .await?;

        total += matched;
        sources.extend(ids);
    }

    if want_merged_upstream {
        // A slice merged upstream is the artefact this platform exists to
        // produce. `merged_at` is set by the GitHub ingestion, not by the
        // person claiming it.
        let matched: i64 = sqlx::query_scalar(
            r#"
            SELECT count(DISTINCT ps.id)
            FROM project_slices ps
            JOIN deliverables d ON d.slice_id = ps.id
            LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
            WHERE d.user_id = $1
              AND ps.merged_at IS NOT NULL
              AND d.revoked_at IS NULL
              AND ($2::VARCHAR IS NULL OR ct.skill_domain = $2)
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_domain.as_deref())
        .fetch_one(db)
        .await?;

        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT ps.id
            FROM project_slices ps
            JOIN deliverables d ON d.slice_id = ps.id
            LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
            WHERE d.user_id = $1
              AND ps.merged_at IS NOT NULL
              AND d.revoked_at IS NULL
              AND ($2::VARCHAR IS NULL OR ct.skill_domain = $2)
            LIMIT 25
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_domain.as_deref())
        .fetch_all(db)
        .await?;

        total += matched;
        sources.extend(ids);
    }

    if want_mission_completed {
        // A closed mission is money that changed hands for work somebody
        // accepted. `skill_domain` sits on the mission itself, so this is the
        // one proof type that does not have to reach through a challenge to
        // find out which domain it belongs to.
        let matched: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM missions
            WHERE assigned_user_id = $1
              AND status = 'closed'
              AND ($2::VARCHAR IS NULL OR skill_domain = $2)
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_domain.as_deref())
        .fetch_one(db)
        .await?;

        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM missions
            WHERE assigned_user_id = $1
              AND status = 'closed'
              AND ($2::VARCHAR IS NULL OR skill_domain = $2)
            ORDER BY id
            LIMIT 25
            "#,
        )
        .bind(user_id)
        .bind(conds.skill_domain.as_deref())
        .fetch_all(db)
        .await?;

        total += matched;
        sources.extend(ids);
    }

    if want_attestation {
        // Counted and sampled separately. They used to be one query with
        // `LIMIT 25`, and the count was the length of that page — so any rule
        // above twenty-five attestations was unreachable, the same bug 0177
        // fixed for deliverables.
        let matched: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM attestations
            WHERE user_id = $1 AND revoked_at IS NULL
              AND ($2::VARCHAR IS NULL OR basis = $2)
            "#,
        )
        .bind(user_id)
        .bind(conds.attestation_basis.as_deref())
        .fetch_one(db)
        .await?;

        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM attestations
            WHERE user_id = $1 AND revoked_at IS NULL
              AND ($2::VARCHAR IS NULL OR basis = $2)
            ORDER BY issued_at DESC
            LIMIT 25
            "#,
        )
        .bind(user_id)
        .bind(conds.attestation_basis.as_deref())
        .fetch_all(db)
        .await?;

        total += matched;
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

/// How many distinct values of a dimension this user's verified work covers.
///
/// The source proofs are the deliverables behind those values, capped like
/// everywhere else — enough to audit the award, not the whole history.
async fn count_distinct_dimension(
    db: &PgPool,
    user_id: Uuid,
    conds: &RuleConditions,
    dimension: &str,
) -> Result<(i64, Vec<Uuid>), AppError> {
    // Each dimension is a query, never an interpolated column name: a
    // dimension read from a JSONB field an operator can edit has no business
    // reaching SQL as an identifier.
    if dimension == "orientation" {
        return count_distinct_orientations(db, user_id, conds).await;
    }
    if dimension != "challenge_language" {
        return Err(AppError::Internal(format!(
            "badge rule asks to count distinct '{dimension}', which nothing implements"
        )));
    }

    let matched: i64 = sqlx::query_scalar(
        r#"
        SELECT count(DISTINCT ct.language)
        FROM deliverables d
        JOIN challenge_templates ct ON ct.id = d.challenge_id
        WHERE d.user_id = $1
          AND d.verification_status = 'verified'
          AND d.revoked_at IS NULL
          AND ct.language IS NOT NULL
          AND ($2::VARCHAR IS NULL OR ct.skill_domain = $2)
        "#,
    )
    .bind(user_id)
    .bind(conds.skill_domain.as_deref())
    .fetch_one(db)
    .await?;

    let sources: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ON (ct.language) d.id
        FROM deliverables d
        JOIN challenge_templates ct ON ct.id = d.challenge_id
        WHERE d.user_id = $1
          AND d.verification_status = 'verified'
          AND d.revoked_at IS NULL
          AND ct.language IS NOT NULL
          AND ($2::VARCHAR IS NULL OR ct.skill_domain = $2)
        ORDER BY ct.language, d.id
        LIMIT 25
        "#,
    )
    .bind(user_id)
    .bind(conds.skill_domain.as_deref())
    .fetch_all(db)
    .await?;

    Ok((matched, sources))
}

/// How many distinct trades this user's verified work covers.
///
/// Reads the orientation on the slice, which migration 0186 added. Work that
/// carries none is not counted: an unlabelled issue is honestly untyped, and
/// counting it as a trade would credit somebody with a speciality nobody
/// recorded.
async fn count_distinct_orientations(
    db: &PgPool,
    user_id: Uuid,
    conds: &RuleConditions,
) -> Result<(i64, Vec<Uuid>), AppError> {
    let matched: i64 = sqlx::query_scalar(
        r#"
        SELECT count(DISTINCT ps.orientation_id)
          FROM deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
          LEFT JOIN orientations o ON o.id = ps.orientation_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND ps.orientation_id IS NOT NULL
           AND ($2::VARCHAR IS NULL OR o.primary_domain = $2)
        "#,
    )
    .bind(user_id)
    .bind(conds.skill_domain.as_deref())
    .fetch_one(db)
    .await?;

    let sources: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ON (ps.orientation_id) d.id
          FROM deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
          LEFT JOIN orientations o ON o.id = ps.orientation_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND ps.orientation_id IS NOT NULL
           AND ($2::VARCHAR IS NULL OR o.primary_domain = $2)
         ORDER BY ps.orientation_id, d.id
         LIMIT 25
        "#,
    )
    .bind(user_id)
    .bind(conds.skill_domain.as_deref())
    .fetch_all(db)
    .await?;

    Ok((matched, sources))
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

        // A manual rule is skipped entirely rather than evaluated to zero:
        // evaluating it would revoke a badge an operator granted on purpose.
        if conds.manual {
            unchanged += 1;
            continue;
        }

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
