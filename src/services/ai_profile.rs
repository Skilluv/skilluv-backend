//! What somebody has actually done in the AI trades, and a single number for
//! it.
//!
//! ## Why nothing is stored
//!
//! The backlog asked for a `craft_score_ai` column. A stored score is wrong
//! within minutes of the next attestation and has no way of knowing it — and
//! when a proof is revoked, the column keeps the points unless somebody
//! remembers to recompute. Every count here derives from proofs that are
//! already immutable, the same rule the badge engine follows. The endpoint
//! caches for a few minutes; the database holds no duplicate of the truth.
//!
//! ## What the score is made of, and what it is not
//!
//! Each term is something a stranger can go and check: an attestation with a
//! basis, a benchmark somebody re-ran, a download count fetched from a hub.
//!
//! Two terms the backlog listed are absent, and their absence is the honest
//! answer rather than an omission:
//!
//!   * *paid missions* — there is no missions table. A term reading zero for
//!     everybody would suggest the platform measures something it does not.
//!   * *average review grade* — reviews record a verdict and a body, not a
//!     score. Inventing one from `approve`/`reject` would turn a binary into
//!     a grade nobody gave.
//!
//! ## Why revoked work scores nothing
//!
//! Every query filters `revoked_at IS NULL`. A score that survives the
//! revocation of what it rests on is the exact failure this platform sells
//! against.

use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

// ═══════════════════════════════════════════════════════════════════
// Weights
// ═══════════════════════════════════════════════════════════════════
//
// Ordered by what the work costs. A verified artefact is a week; a published
// model with a card is several; a paper is months. The ratios say that, and
// nothing here is tuned against data we do not have yet — these are a stated
// editorial position, not a measurement.

/// Any verified AI deliverable. Deliberately small: the specific attestations
/// below are where shipped work earns its weight, and counting both would pay
/// twice for one piece of work.
const PER_VERIFIED_ARTIFACT: i64 = 5;
const PER_MODEL_SHIPPED: i64 = 60;
const PER_DATASET_PUBLISHED: i64 = 40;
const PER_AGENT_SYSTEM: i64 = 50;
const PER_PAPER: i64 = 100;
/// Only reproduced benchmarks score. An unverified SOTA claim is the single
/// easiest thing to overstate in this domain, and paying for it would make
/// the score reward the claim rather than the result.
const PER_BENCHMARK_REPRODUCED: i64 = 150;
const PER_SAFETY_FINDING: i64 = 80;
const PER_FEATURED: i64 = 200;
/// Per year since the first verified AI artefact. Small, because time is not
/// work — it is only evidence that the work was not a single burst.
const PER_YEAR_ACTIVE: i64 = 30;

/// Bonus for total monthly downloads across every published model and
/// dataset, in steps rather than proportionally.
///
/// Capped, and the cap is the point: downloads measure reach, which depends
/// on the subject as much as on the craft. One model that goes around the
/// world should not outweigh a career.
fn downloads_bonus(downloads: i64) -> i64 {
    match downloads {
        d if d >= 100_000 => 400,
        d if d >= 10_000 => 200,
        d if d >= 1_000 => 75,
        d if d >= 100 => 25,
        _ => 0,
    }
}

/// Where a score places somebody. The names are the ones the trade uses.
fn tier_for(score: i64) -> &'static str {
    match score {
        s if s >= 3_500 => "researcher",
        s if s >= 1_500 => "senior",
        s if s >= 500 => "engineer",
        s if s >= 100 => "practitioner",
        _ => "apprentice",
    }
}

#[derive(Debug, Serialize, ToSchema, Default)]
pub struct AiProofCounts {
    /// Verified deliverables in the AI domain, whatever they produced.
    pub verified_artifacts: i64,
    pub models_shipped: i64,
    pub datasets_published: i64,
    pub agent_systems_deployed: i64,
    pub papers_published: i64,
    /// Benchmarks a reviewer re-ran and confirmed. Claims nobody checked are
    /// not counted here and score nothing.
    pub benchmarks_reproduced: i64,
    pub safety_findings_validated: i64,
    pub featured_times: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiProfile {
    pub username: String,
    pub craft_score: i64,
    /// `apprentice`, `practitioner`, `engineer`, `senior`, `researcher`.
    pub tier: &'static str,
    pub counts: AiProofCounts,
    /// Trades this person has verified work in, by slug.
    pub orientations: Vec<String>,
    /// Monthly downloads summed across published models and datasets. NULL is
    /// impossible here — no published artefact reads as zero, which is true.
    pub hub_downloads_recent: i64,
    pub hub_likes: i64,
    /// Whole years since the first verified AI artefact.
    pub years_active: i64,
}

/// Everything one person has to show in the AI trades.
///
/// Six queries rather than one join: the counts come from different tables
/// with different revocation rules, and folding them together produced a
/// query nobody could read and whose `count(DISTINCT …)` were easy to get
/// silently wrong.
pub async fn build(db: &PgPool, username: &str) -> Result<AiProfile, AppError> {
    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(db)
        .await?;
    let Some(user_id) = user_id else {
        return Err(AppError::NotFound(format!("user '{username}' not found")));
    };

    // A deliverable is AI work if its challenge says so, or if the slice it
    // came from belongs to an AI trade. Both, because the two paths into the
    // platform — a challenge and an ingested issue — carry the domain in
    // different places, and reading only one would lose half the work.
    let verified_artifacts: i64 = sqlx::query_scalar(
        r#"
        SELECT count(DISTINCT d.id)
          FROM deliverables d
          LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
          LEFT JOIN project_slices ps ON ps.id = d.slice_id
          LEFT JOIN orientations o ON o.id = ps.orientation_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND (ct.skill_domain = 'ai' OR o.primary_domain = 'ai')
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let mut counts = AiProofCounts {
        verified_artifacts,
        ..Default::default()
    };

    let by_basis: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT basis, count(*)
          FROM attestations
         WHERE user_id = $1
           AND revoked_at IS NULL
           AND basis IN ('ai_model_shipped', 'ai_dataset_published',
                         'ai_agent_system_deployed', 'ai_paper_published',
                         'ai_safety_finding_validated', 'featured_ai_researcher')
         GROUP BY basis
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    for (basis, n) in by_basis {
        match basis.as_str() {
            "ai_model_shipped" => counts.models_shipped = n,
            "ai_dataset_published" => counts.datasets_published = n,
            "ai_agent_system_deployed" => counts.agent_systems_deployed = n,
            "ai_paper_published" => counts.papers_published = n,
            "ai_safety_finding_validated" => counts.safety_findings_validated = n,
            "featured_ai_researcher" => counts.featured_times = n,
            // The IN clause above is the list. A value reaching here means
            // somebody widened one of the two without the other.
            other => tracing::warn!(basis = %other, "unhandled AI attestation basis"),
        }
    }

    counts.benchmarks_reproduced = sqlx::query_scalar(
        r#"
        SELECT count(DISTINCT br.id)
          FROM benchmark_results br
          JOIN deliverables d ON d.slice_id = br.slice_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND br.reproduced_at IS NOT NULL
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let (hub_downloads_recent, hub_likes): (i64, i64) = sqlx::query_as(
        r#"
        SELECT COALESCE(sum(st.downloads_recent), 0)::BIGINT,
               COALESCE(sum(st.likes_count), 0)::BIGINT
          FROM published_artifact_stats st
          JOIN deliverables d ON d.slice_id = st.slice_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND st.registry IN ('huggingface_models', 'huggingface_datasets',
                               'kaggle_datasets')
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let orientations: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT o.slug
          FROM deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
          JOIN orientations o ON o.id = ps.orientation_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND o.primary_domain = 'ai'
         ORDER BY o.slug
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // Whole years, floored. Somebody eleven months in has been active for
    // zero years, which is the honest reading and stops the term from paying
    // out on the first day.
    let years_active: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(
                 floor(EXTRACT(EPOCH FROM (NOW() - min(d.created_at)))
                       / (365.25 * 86400)),
                 0)::BIGINT
          FROM deliverables d
          LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
          LEFT JOIN project_slices ps ON ps.id = d.slice_id
          LEFT JOIN orientations o ON o.id = ps.orientation_id
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND (ct.skill_domain = 'ai' OR o.primary_domain = 'ai')
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let craft_score = counts.verified_artifacts * PER_VERIFIED_ARTIFACT
        + counts.models_shipped * PER_MODEL_SHIPPED
        + counts.datasets_published * PER_DATASET_PUBLISHED
        + counts.agent_systems_deployed * PER_AGENT_SYSTEM
        + counts.papers_published * PER_PAPER
        + counts.benchmarks_reproduced * PER_BENCHMARK_REPRODUCED
        + counts.safety_findings_validated * PER_SAFETY_FINDING
        + counts.featured_times * PER_FEATURED
        + years_active * PER_YEAR_ACTIVE
        + downloads_bonus(hub_downloads_recent);

    Ok(AiProfile {
        username: username.to_string(),
        craft_score,
        tier: tier_for(craft_score),
        counts,
        orientations,
        hub_downloads_recent,
        hub_likes,
        years_active,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_do_not_overlap_or_leave_a_gap() {
        assert_eq!(tier_for(0), "apprentice");
        assert_eq!(tier_for(99), "apprentice");
        assert_eq!(tier_for(100), "practitioner");
        assert_eq!(tier_for(499), "practitioner");
        assert_eq!(tier_for(500), "engineer");
        assert_eq!(tier_for(1_499), "engineer");
        assert_eq!(tier_for(1_500), "senior");
        assert_eq!(tier_for(3_499), "senior");
        assert_eq!(tier_for(3_500), "researcher");
    }

    #[test]
    fn reach_is_bounded() {
        // A model that goes around the world is worth noticing and must not
        // outweigh everything else somebody has done.
        assert_eq!(downloads_bonus(0), 0);
        assert_eq!(downloads_bonus(99), 0);
        assert_eq!(downloads_bonus(100), 25);
        assert_eq!(downloads_bonus(1_000), 75);
        assert_eq!(downloads_bonus(10_000), 200);
        assert_eq!(downloads_bonus(100_000), 400);
        assert_eq!(downloads_bonus(50_000_000), 400);
    }

    #[test]
    fn a_paper_outweighs_a_dataset_which_outweighs_an_artifact() {
        // The ordering is the editorial position; a change that inverts it
        // should have to change this test and say why.
        const { assert!(PER_PAPER > PER_MODEL_SHIPPED) };
        const { assert!(PER_MODEL_SHIPPED > PER_DATASET_PUBLISHED) };
        const { assert!(PER_DATASET_PUBLISHED > PER_VERIFIED_ARTIFACT) };
        const { assert!(PER_BENCHMARK_REPRODUCED > PER_PAPER) };
    }
}
