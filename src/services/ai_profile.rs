//! The AI craft score (migration 0300).
//!
//! The second domain to score, and deliberately a second module rather than a
//! branch in [`crate::services::craft_score`] — which is what that module
//! says should happen. The formula, the tiers and the storage are shared and
//! keyed by domain; only the measuring differs, because it reads different
//! tables.
//!
//! ## What is stored and what is not
//!
//! The number and its tier, in `craft_scores`, so a recruiter listing can
//! sort without recomputing fourteen counts per row. Everything else —
//! including the breakdown — is derived on read, and the endpoint recomputes
//! rather than reading the stored figure: a profile page showing a score an
//! hour out of date is showing a revoked attestation still counting.
//!
//! ## What the score is made of, and what it is not
//!
//! Each term counts something a stranger can go and check: an attestation
//! with a basis, a benchmark somebody re-ran, a download figure fetched from
//! a hub. The weights live in `craft_score_weights` so the ratios can be
//! argued with in the admin panel instead of in a pull request.
//!
//! ## Why revoked work scores nothing
//!
//! Every count reads `countable_deliverables` or filters `revoked_at IS
//! NULL`. A score that survives the revocation of what it rests on is the
//! exact failure this platform sells against.

use bigdecimal::{BigDecimal, ToPrimitive};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore, Term};

/// The domain this module scores.
pub const DOMAIN: &str = "ai";

/// The same ceiling as every other domain, and deliberately.
///
/// Migration 0204 shares one set of tiers across all of them, because a tier
/// is a position on a scale and each scale is calibrated by its own weights.
/// A domain-specific cap breaks that as surely as domain-specific names
/// would: a ceiling of six thousand puts `principal`, which starts at seven,
/// permanently out of reach.
///
/// What makes AI score differently is the weights, which are lower in
/// aggregate. That is the whole mechanism.
pub const CAP: i32 = craft_score::CAP;

/// Everything the formula counts, in one round-trip.
///
/// One query rather than thirteen: this runs for every profile on a sweep,
/// and thirteen round-trips per person is the difference between a sweep that
/// finishes and one that does not.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_ai: i64,
    models_shipped: i64,
    datasets_published: i64,
    agent_systems_deployed: i64,
    papers_published: i64,
    benchmarks_reproduced: i64,
    safety_findings_validated: i64,
    missions_completed: i64,
    hub_downloads: i64,
    review_grid_average: Option<BigDecimal>,
    orientations_distinct: i64,
    years_active: i64,
    featured_times: i64,
}

/// The seven AI bases, as a SQL list. Written once so a basis added to the
/// schema and forgotten here shows up as a term that stops counting rather
/// than as a silent undercount spread across four queries.
const AI_BASES: &str = "'ai_model_shipped', 'ai_dataset_published', \
                        'ai_agent_system_deployed', 'ai_paper_published', \
                        'ai_benchmark_result', 'ai_safety_finding_validated', \
                        'featured_ai_researcher'";

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is a compile-time constant, never anything a caller
    // supplies: it reaches SQL as text because a bound array would need a
    // cast in seven places for no gain.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL AND basis IN ({AI_BASES}))
                AS attestations_ai,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ai_model_shipped')
                AS models_shipped,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ai_dataset_published')
                AS datasets_published,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ai_agent_system_deployed')
                AS agent_systems_deployed,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ai_paper_published')
                AS papers_published,

            -- Only reproduced benchmarks. An unverified record is the single
            -- easiest thing to overstate in this domain, and paying for it
            -- would make the score reward the claim rather than the result.
            (SELECT count(DISTINCT br.id)
               FROM benchmark_results br
               JOIN countable_deliverables d ON d.slice_id = br.slice_id
              WHERE d.user_id = $1 AND br.reproduced_at IS NOT NULL)
                AS benchmarks_reproduced,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ai_safety_finding_validated')
                AS safety_findings_validated,

            (SELECT count(*) FROM missions
              WHERE assigned_user_id = $1 AND status = 'closed'
                AND skill_domain = 'ai')
                AS missions_completed,

            -- Monthly downloads across published models and datasets. NULL
            -- stays out of the sum: a hub that measures nothing must not read
            -- as zero. Cast because summing bigints gives a NUMERIC, which
            -- fails to decode into an i64 at runtime rather than at compile
            -- time.
            (SELECT COALESCE(sum(COALESCE(ps.downloads_recent, ps.downloads_total)), 0)::BIGINT
               FROM published_artifact_stats ps
               JOIN countable_deliverables d ON d.slice_id = ps.slice_id
              WHERE d.user_id = $1
                AND ps.registry IN ('huggingface_models', 'huggingface_datasets',
                                    'kaggle_datasets'))
                AS hub_downloads,

            -- Only scorings made against an AI grid. Averaging every grid a
            -- person has ever been scored on would let a strong code reviewer
            -- lift an AI score, and the term claims to measure AI work.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN review_grids g ON g.id = rgs.grid_id
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'ai')
                AS review_grid_average,

            (SELECT count(DISTINCT o.id)
               FROM countable_deliverables d
               JOIN project_slices ps ON ps.id = d.slice_id
               JOIN orientations o ON o.id = ps.orientation_id
              WHERE d.user_id = $1 AND o.primary_domain = 'ai')
                AS orientations_distinct,

            -- Whole years, floored: somebody eleven months in has been active
            -- for zero, which stops the term paying out on the first day.
            (SELECT COALESCE(
                      floor(EXTRACT(EPOCH FROM (NOW() - min(d.created_at)))
                            / (365.25 * 86400)), 0)::BIGINT
               FROM countable_deliverables d
               LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
               LEFT JOIN project_slices ps ON ps.id = d.slice_id
               LEFT JOIN orientations o ON o.id = ps.orientation_id
              WHERE d.user_id = $1
                AND (ct.skill_domain = 'ai' OR o.primary_domain = 'ai'))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_ai_researcher')
                AS featured_times
        "#
    );

    Ok(sqlx::query_as::<_, Measurements>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_one(db)
        .await?)
}

/// The score, its tier and what it is made of. Nothing is written.
pub async fn compute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let weights = craft_score::weights_for(db, DOMAIN).await?;
    let m = measure(db, user_id).await?;

    let mut breakdown = Vec::new();
    let mut total: i64 = 0;

    for w in weights {
        let measured: f64 = match w.term.as_str() {
            "attestations_ai" => m.attestations_ai as f64,
            "models_shipped" => m.models_shipped as f64,
            "datasets_published" => m.datasets_published as f64,
            "agent_systems_deployed" => m.agent_systems_deployed as f64,
            "papers_published" => m.papers_published as f64,
            "benchmarks_reproduced" => m.benchmarks_reproduced as f64,
            "safety_findings_validated" => m.safety_findings_validated as f64,
            "missions_completed" => m.missions_completed as f64,
            "hub_downloads" => m.hub_downloads as f64,
            "review_grid_average" => match &m.review_grid_average {
                Some(avg) => avg.to_f64().unwrap_or(0.0),
                // Nobody has scored this person's AI work. Skipped rather
                // than counted as zero: an unscored average would subtract
                // the whole baseline from the total.
                None => continue,
            },
            "orientations_distinct" => m.orientations_distinct as f64,
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            unknown => {
                // Somebody added a row proposing a term. The answer to an
                // unimplemented proposal is silence in the total.
                tracing::warn!(
                    term = unknown,
                    "craft_score_weights names an AI term nothing knows how to count"
                );
                continue;
            }
        };

        if measured == 0.0 {
            continue;
        }

        let points = craft_score::points_for(
            &w.kind,
            w.weight.to_f64().unwrap_or(0.0),
            w.baseline.as_ref().and_then(|b| b.to_f64()),
            measured,
        );
        if points == 0 {
            continue;
        }

        total += points as i64;
        breakdown.push(Term {
            term: w.term,
            measured,
            points,
            explanation: w.explanation,
        });
    }

    let capped = total > CAP as i64;
    let score = total.min(CAP as i64) as i32;
    let (tier_slug, tier_name, tier_description, next_tier_at) =
        craft_score::resolve_tier(db, DOMAIN, score).await?;

    Ok(CraftScore {
        score,
        tier_slug,
        tier_name,
        tier_description,
        next_tier_at,
        breakdown,
        capped,
    })
}

/// Compute and store, so a listing can sort without recomputing.
pub async fn recompute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let computed = compute(db, user_id).await?;
    craft_score::store(db, user_id, DOMAIN, computed.score, &computed.tier_slug).await?;
    Ok(computed)
}

/// Recompute everybody whose AI score is stale or was never computed.
///
/// Bounded per pass and oldest first, like the code sweep: one that tries the
/// whole table at once never reaches the end of the alphabet.
pub async fn sweep(db: &PgPool, batch: i64) -> Result<u64, AppError> {
    let stale: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.id FROM users u
           LEFT JOIN craft_scores cs
                  ON cs.user_id = u.id AND cs.skill_domain = $2
          WHERE u.is_banned = FALSE
            AND (cs.computed_at IS NULL
                 OR cs.computed_at < NOW() - INTERVAL '1 hour')
          ORDER BY cs.computed_at NULLS FIRST
          LIMIT $1",
    )
    .bind(batch)
    .bind(DOMAIN)
    .fetch_all(db)
    .await?;

    let mut done = 0u64;
    for user_id in stale {
        // One failure must not stop the sweep: the next pass picks it up, and
        // stopping would leave everybody after it stale forever.
        match recompute(db, user_id).await {
            Ok(_) => done += 1,
            Err(e) => {
                tracing::error!(user = %user_id, error = %e, "AI craft score recompute failed")
            }
        }
    }
    metrics::counter!("skilluv_ai_craft_score_recomputed_total").increment(done);
    Ok(done)
}

// ═══════════════════════════════════════════════════════════════════
// The public profile
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, ToSchema)]
pub struct AiProfile {
    pub username: String,
    pub craft_score: i32,
    /// `apprentice`, `contributor`, `engineer`, `senior`, `staff`,
    /// `principal` — the same six every domain uses, so somebody can compare
    /// their own two profiles.
    pub tier: String,
    pub tier_name: String,
    pub tier_description: String,
    pub next_tier_at: Option<i32>,
    /// What was counted and what each count was worth. A score with no
    /// explanation is a number somebody has to trust.
    pub breakdown: Vec<Term>,
    pub capped: bool,
    /// Trades this person has verified work in, by slug.
    pub orientations: Vec<String>,
    /// The same score, as the one envelope every domain shares (SKI-322): a
    /// client that reads `score` on any profile reads it on all eight. The
    /// flat fields above are the earlier shape, kept until the front moves off
    /// them.
    pub score: CraftScore,
}

/// Everything one person has to show in the AI trades.
///
/// Recomputes rather than reading the stored figure: the sweep runs hourly,
/// and a profile page showing an hour-old score is a page showing a revoked
/// attestation still counting.
pub async fn build(db: &PgPool, username: &str) -> Result<AiProfile, AppError> {
    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(db)
        .await?;
    let Some(user_id) = user_id else {
        return Err(AppError::NotFound(format!("user '{username}' not found")));
    };

    let score = recompute(db, user_id).await?;

    let orientations: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.slug
           FROM countable_deliverables d
           JOIN project_slices ps ON ps.id = d.slice_id
           JOIN orientations o ON o.id = ps.orientation_id
          WHERE d.user_id = $1 AND o.primary_domain = 'ai'
          ORDER BY o.slug",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(AiProfile {
        username: username.to_string(),
        craft_score: score.score,
        tier: score.tier_slug.clone(),
        tier_name: score.tier_name.clone(),
        tier_description: score.tier_description.clone(),
        next_tier_at: score.next_tier_at,
        breakdown: score.breakdown.clone(),
        capped: score.capped,
        orientations,
        score,
    })
}
