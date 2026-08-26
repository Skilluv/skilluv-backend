//! The education craft score (migration 0525).
//!
//! The fifth domain to score. The formula, the tiers, the cap and the
//! assembly are shared — [`craft_score::assemble`] was extracted when audio
//! became the third — and what is domain-specific is the measuring, and only
//! the measuring.
//!
//! ## What the score counts, and the figure it refuses to count at face value
//!
//! Every term counts something a stranger can go and check: an attestation
//! with a basis, an adoption by a named trainer, a mission closed, a review
//! grid filled in. The exception is `learners_reached`, and this domain has
//! the hardest version of that problem on the platform.
//!
//! Almost all teaching happens somewhere with no API and no register. A
//! bootcamp instructor with eight years and two thousand alumni has a real
//! career and no machine-readable trace of it. Excluding that would make the
//! score describe only what happened here, which for this domain is almost
//! nothing; counting a typed figure at face value would make the score a
//! self-assessment.
//!
//! So: learners in cohorts *led on this platform* count in full, because they
//! are rows; enrolments declared from an outside platform count for half; and
//! the whole term is logarithmic, so no headcount can reach the ceiling
//! alone.
//!
//! ## What is deliberately absent
//!
//! Teaching hours. Migrations 0521, 0522 and 0525 each wrote out why, and the
//! short version is that at half a point an hour a typed four thousand would
//! have outweighed every other term put together.
//!
//! ## Why revoked work scores nothing
//!
//! Every count filters `revoked_at IS NULL` or reads `countable_deliverables`.
//! A score that survives the revocation of what it rests on is the exact
//! failure this platform sells against.

use bigdecimal::{BigDecimal, ToPrimitive};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore, Term};

/// The domain this module scores.
pub const DOMAIN: &str = "education";

/// The same ceiling as every other domain, and deliberately: migration 0204
/// shares one set of tiers across all of them, because a tier is a position on
/// a scale and each scale is calibrated by its own weights.
pub const CAP: i32 = craft_score::CAP;

/// What a declared enrolment figure is worth against a counted learner.
///
/// Applied before the logarithm, so it moves the term by a constant rather
/// than by a proportion.
const DECLARED_LEARNER_DISCOUNT: f64 = 0.5;

/// The five education bases, as a SQL list. Written once so a basis added to
/// the schema and forgotten here shows up as a term that stops counting
/// rather than as a silent undercount spread across five queries.
const EDUCATION_BASES: &str = "'education_cohort_delivered', \
                               'education_workshop_delivered', \
                               'education_curriculum_authored', \
                               'education_assessment_framework_published', \
                               'featured_educator'";

/// Everything the formula counts, in one round-trip.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_education: i64,
    cohorts_delivered: i64,
    workshops_delivered: i64,
    curriculum_adoptions: i64,
    assessment_frameworks_published: i64,
    missions_completed: i64,
    counted_learners: i64,
    declared_learners: i64,
    review_grid_average: Option<BigDecimal>,
    orientations_distinct: i64,
    years_active: i64,
    featured_times: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is a compile-time constant, never anything a caller
    // supplies: it reaches SQL as text because a bound array would need a cast
    // in five places for no gain.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis IN ({EDUCATION_BASES}))
                AS attestations_education,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'education_cohort_delivered')
                AS cohorts_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'education_workshop_delivered')
                AS workshops_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'education_assessment_framework_published')
                AS assessment_frameworks_published,

            -- Adoptions rather than curriculums: five trainers running one
            -- programme and one trainer running five are different
            -- achievements, and the adoption is the countable one.
            --
            -- Counted through the deliverable, so a curriculum whose
            -- verification was withdrawn stops paying out even though the
            -- adoption rows survive it.
            (SELECT count(*)
               FROM education_curriculum_adoptions a
               JOIN countable_deliverables d ON d.slice_id = a.curriculum_slice_id
              WHERE d.user_id = $1)
                AS curriculum_adoptions,

            (SELECT count(*) FROM missions
              WHERE assigned_user_id = $1 AND status = 'closed'
                AND skill_domain = 'education')
                AS missions_completed,

            -- Learners in cohorts this person actually led here. Rows, not a
            -- claim, and the whole reason a taught cohort is a cohort rather
            -- than a number on a report.
            (SELECT count(*)
               FROM cohort_members m
               JOIN cohorts c ON c.id = m.cohort_id
              WHERE c.led_by_user_id = $1 AND m.user_id <> $1)
                AS counted_learners,

            -- Enrolments declared on an outside course platform. Halved by
            -- `reach` below: this platform has no register of a class it did
            -- not host.
            (SELECT COALESCE(sum(p.reach_count), 0)::BIGINT
               FROM user_external_portfolios p
               JOIN portfolio_platforms pf ON pf.slug = p.platform
              WHERE p.user_id = $1 AND pf.skill_domain = 'education')
                AS declared_learners,

            -- Only scorings made against an education grid. Averaging every
            -- grid a person has ever been scored on would let a strong code
            -- reviewer lift this score, and the term claims to measure
            -- teaching.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN review_grids g ON g.id = rgs.grid_id
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'education')
                AS review_grid_average,

            (SELECT count(DISTINCT o.id)
               FROM countable_deliverables d
               JOIN project_slices ps ON ps.id = d.slice_id
               JOIN orientations o ON o.id = ps.orientation_id
              WHERE d.user_id = $1 AND o.primary_domain = 'education')
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
                AND (ct.skill_domain = 'education'
                     OR o.primary_domain = 'education'))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_educator')
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

    craft_score::assemble(db, DOMAIN, weights, |term| match term {
        "attestations_education" => Some(m.attestations_education as f64),
        "cohorts_delivered" => Some(m.cohorts_delivered as f64),
        "workshops_delivered" => Some(m.workshops_delivered as f64),
        "curriculum_adoptions" => Some(m.curriculum_adoptions as f64),
        "assessment_frameworks_published" => Some(m.assessment_frameworks_published as f64),
        "missions_completed" => Some(m.missions_completed as f64),
        "learners_reached" => Some(learners(m.counted_learners, m.declared_learners)),
        // Nobody has scored this person's teaching. Skipped rather than
        // counted as zero: an unscored average would subtract the whole
        // baseline from the total.
        "review_grid_average" => m.review_grid_average.as_ref().and_then(|a| a.to_f64()),
        "orientations_distinct" => Some(m.orientations_distinct as f64),
        "years_active" => Some(m.years_active as f64),
        "featured_times" => Some(m.featured_times as f64),
        unknown => {
            // Somebody added a row proposing a term. The answer to an
            // unimplemented proposal is silence in the total.
            tracing::warn!(
                term = unknown,
                "craft_score_weights names an education term nothing knows how to count"
            );
            None
        }
    })
    .await
}

/// The headcount the score uses, from the two kinds of number.
///
/// Declared enrolments are halved before the term's own logarithm is applied,
/// which moves them by a constant rather than by a proportion. Kept as its own
/// function so the trade-off is testable without a database.
fn learners(counted: i64, declared: i64) -> f64 {
    counted as f64 + (declared as f64 * DECLARED_LEARNER_DISCOUNT)
}

/// Compute and store, so a listing can sort without recomputing.
pub async fn recompute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let computed = compute(db, user_id).await?;
    craft_score::store(db, user_id, DOMAIN, computed.score, &computed.tier_slug).await?;
    Ok(computed)
}

/// Recompute everybody whose education score is stale or was never computed.
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
            Err(e) => tracing::error!(
                user = %user_id, error = %e,
                "education craft score recompute failed"
            ),
        }
    }
    metrics::counter!("skilluv_education_craft_score_recomputed_total").increment(done);
    Ok(done)
}

// ═══════════════════════════════════════════════════════════════════
// The public profile
// ═══════════════════════════════════════════════════════════════════

/// One cohort this person led, in aggregate.
///
/// No learner appears here, ever. The completion figure is a count over
/// `education_learner_outcomes`, and the rows it counts stay in that table:
/// a public profile is exactly the surface the learner-data rules of
/// migrations 0520, 0523 and 0524 exist to keep them off.
#[derive(Debug, Serialize, ToSchema)]
pub struct CohortSummary {
    pub cohort_id: Uuid,
    pub name: String,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub concluded_at: Option<chrono::DateTime<chrono::Utc>>,
    pub learners: i64,
    /// Learners with a recorded outcome who completed. Absent when nobody
    /// recorded anything, which is not the same as nobody finishing.
    pub completed: Option<i64>,
    pub outcomes_recorded: i64,
}

/// One published education artefact.
#[derive(Debug, Serialize, ToSchema)]
pub struct EducationHighlight {
    pub slice_id: Uuid,
    pub title: String,
    pub subtype: String,
    pub target_audience: Option<String>,
    pub url: Option<String>,
    /// How many other trainers have run it. Only meaningful for a curriculum.
    pub adoptions: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EducationProfile {
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
    /// Cohorts led here, most recent first, in aggregate.
    pub cohorts: Vec<CohortSummary>,
    /// Verified work worth reading first.
    pub highlights: Vec<EducationHighlight>,
}

/// Everything one person has to show in the education trades.
///
/// Recomputes rather than reading the stored figure: the sweep runs hourly,
/// and a profile page showing an hour-old score is a page showing a revoked
/// attestation still counting.
pub async fn build(db: &PgPool, username: &str) -> Result<EducationProfile, AppError> {
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
          WHERE d.user_id = $1 AND o.primary_domain = 'education'
          ORDER BY o.slug",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let cohorts = cohorts_for(db, user_id).await?;
    let highlights = highlights_for(db, user_id).await?;

    Ok(EducationProfile {
        username: username.to_string(),
        craft_score: score.score,
        tier: score.tier_slug,
        tier_name: score.tier_name,
        tier_description: score.tier_description,
        next_tier_at: score.next_tier_at,
        breakdown: score.breakdown,
        capped: score.capped,
        orientations,
        cohorts,
        highlights,
    })
}

/// Cohorts this person led, in aggregate and with nobody named.
async fn cohorts_for(db: &PgPool, user_id: Uuid) -> Result<Vec<CohortSummary>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        cohort_id: Uuid,
        name: String,
        starts_at: chrono::DateTime<chrono::Utc>,
        concluded_at: Option<chrono::DateTime<chrono::Utc>>,
        learners: i64,
        completed: Option<i64>,
        outcomes_recorded: i64,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT c.id AS cohort_id, c.name, c.starts_at, c.concluded_at,
               (SELECT count(*) FROM cohort_members m
                 WHERE m.cohort_id = c.id AND m.user_id <> $1) AS learners,
               (SELECT count(*) FROM education_learner_outcomes o
                 WHERE o.cohort_id = c.id) AS outcomes_recorded,
               -- NULL when nobody recorded anything, zero when they did and
               -- nobody finished. "Nobody finished" and "nobody wrote it
               -- down" are different facts, and a profile must not print the
               -- first for the second.
               --
               -- Counted from the membership since migration 0532: finishing
               -- is a fact about participation, and both domains that run
               -- cohorts used to record it separately. The outcome rows still
               -- decide whether anything was measured at all, which is why
               -- they are what the NULL turns on.
               (SELECT CASE WHEN NOT EXISTS (
                                   SELECT 1 FROM education_learner_outcomes o
                                    WHERE o.cohort_id = c.id)
                            THEN NULL
                            ELSE (SELECT count(*) FROM cohort_members m
                                   WHERE m.cohort_id = c.id
                                     AND m.user_id <> $1
                                     AND m.graduated_at IS NOT NULL)
                       END) AS completed
          FROM cohorts c
         WHERE c.led_by_user_id = $1
         ORDER BY c.starts_at DESC
         LIMIT 20
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| CohortSummary {
            cohort_id: r.cohort_id,
            name: r.name,
            starts_at: r.starts_at,
            concluded_at: r.concluded_at,
            learners: r.learners,
            completed: r.completed,
            outcomes_recorded: r.outcomes_recorded,
        })
        .collect())
}

/// Verified education work, most recent first, capped.
async fn highlights_for(db: &PgPool, user_id: Uuid) -> Result<Vec<EducationHighlight>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        slice_id: Uuid,
        title: String,
        subtype: String,
        target_audience: Option<String>,
        url: Option<String>,
        adoptions: i64,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT ps.id AS slice_id,
               ps.title,
               ps.education_subtype AS subtype,
               ps.education_target_audience AS target_audience,
               COALESCE(ps.published_artifact_url, ps.pr_url) AS url,
               (SELECT count(*) FROM education_curriculum_adoptions a
                 WHERE a.curriculum_slice_id = ps.id) AS adoptions
          FROM countable_deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
         WHERE d.user_id = $1 AND ps.slice_type = 'education_artifact'
         GROUP BY ps.id
         ORDER BY max(d.created_at) DESC
         LIMIT 12
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| EducationHighlight {
            slice_id: r.slice_id,
            title: r.title,
            subtype: r.subtype,
            target_audience: r.target_audience,
            url: r.url,
            adoptions: r.adoptions,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_headcount_counts_for_half() {
        assert_eq!(learners(20, 0), 20.0);
        assert_eq!(learners(0, 2_000), 1_000.0);
        assert_eq!(learners(20, 40), 40.0);
    }

    #[test]
    fn nobody_who_has_taught_nobody_is_penalised() {
        // Zero is skipped by `assemble` rather than scored, which matters
        // because a curriculum designer may never have led a cohort and is
        // not worse at their trade for it.
        assert_eq!(learners(0, 0), 0.0);
    }
}
