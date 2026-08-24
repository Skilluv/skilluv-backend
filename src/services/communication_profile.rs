//! The communication craft score (migration 0508).
//!
//! The fourth domain to score. The formula, the tiers, the cap and the
//! assembly are shared — [`craft_score::assemble`] was extracted when audio
//! became the third — and what is domain-specific is the measuring, and only
//! the measuring.
//!
//! ## What the score counts, and the one figure it discounts
//!
//! Every term counts something a stranger can go and check: an attestation
//! with a basis, a mission closed, a review grid filled in, a language a
//! translation was carried into. The exception is `audience_reach`, and it is
//! the sharpest case of it on the platform.
//!
//! Audience figures come from two places. Per-artefact ones are fetched from
//! platforms with an API — DEV, Hashnode, YouTube, Zenodo — and are as good as
//! the platform. Per-account ones come from `user_external_portfolios`, where
//! Medium, a personal blog and Apple Podcasts have no API at all and the
//! number is what the person read on their own dashboard.
//!
//! Excluding the second would erase the recorded career of most technical
//! bloggers, who publish on their own domain. Counting it at face value would
//! make the score a self-assessment. Halving it, before a logarithm, is the
//! compromise migration 0415 established for audio: a declared million lands
//! where a verified half million would.
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
pub const DOMAIN: &str = "communication";

/// The same ceiling as every other domain, and deliberately: migration 0204
/// shares one set of tiers across all of them, because a tier is a position on
/// a scale and each scale is calibrated by its own weights.
pub const CAP: i32 = craft_score::CAP;

/// What a declared audience figure is worth against a fetched one.
///
/// Applied before the logarithm, so it moves the term by a constant rather
/// than by a proportion.
const DECLARED_REACH_DISCOUNT: f64 = 0.5;

/// The six communication bases, as a SQL list. Written once so a basis added
/// to the schema and forgotten here shows up as a term that stops counting
/// rather than as a silent undercount spread across five queries.
const COMMUNICATION_BASES: &str = "'communication_docs_contribution', \
                                   'communication_talk_delivered', \
                                   'communication_content_published', \
                                   'communication_translation_validated', \
                                   'communication_research_published', \
                                   'featured_communicator'";

/// Everything the formula counts, in one round-trip.
///
/// One query rather than thirteen: this runs for every profile on a sweep, and
/// thirteen round-trips per person is the difference between a sweep that
/// finishes and one that does not.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_communication: i64,
    docs_contributions: i64,
    talks_delivered: i64,
    content_published: i64,
    translations_validated: i64,
    research_published: i64,
    missions_completed: i64,
    verified_reach: i64,
    declared_reach: i64,
    review_grid_average: Option<BigDecimal>,
    orientations_distinct: i64,
    target_languages_distinct: i64,
    years_active: i64,
    featured_times: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is a compile-time constant, never anything a caller
    // supplies: it reaches SQL as text because a bound array would need a cast
    // in six places for no gain.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis IN ({COMMUNICATION_BASES}))
                AS attestations_communication,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'communication_docs_contribution')
                AS docs_contributions,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'communication_talk_delivered')
                AS talks_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'communication_content_published')
                AS content_published,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'communication_translation_validated')
                AS translations_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'communication_research_published')
                AS research_published,

            (SELECT count(*) FROM missions
              WHERE assigned_user_id = $1 AND status = 'closed'
                AND skill_domain = 'communication')
                AS missions_completed,

            -- Readers and viewers, from the two places they come from and
            -- kept apart. Per-artefact figures fetched from a platform with an
            -- API count in full; per-account figures the person typed are
            -- halved by `reach` below.
            --
            -- NULL stays out of both sums: a platform that publishes nothing
            -- must not read as zero.
            ((SELECT COALESCE(sum(s.views_count), 0)::BIGINT
                FROM published_artifact_stats s
                JOIN publication_registries r ON r.slug = s.registry
                JOIN countable_deliverables d ON d.slice_id = s.slice_id
               WHERE d.user_id = $1 AND r.skill_domain = 'communication')
             + (SELECT COALESCE(sum(p.reach_count), 0)::BIGINT
                  FROM user_external_portfolios p
                  JOIN portfolio_platforms pf ON pf.slug = p.platform
                 WHERE p.user_id = $1 AND pf.skill_domain = 'communication'
                   AND NOT p.figures_are_declared))
                AS verified_reach,

            (SELECT COALESCE(sum(p.reach_count), 0)::BIGINT
               FROM user_external_portfolios p
               JOIN portfolio_platforms pf ON pf.slug = p.platform
              WHERE p.user_id = $1 AND pf.skill_domain = 'communication'
                AND p.figures_are_declared)
                AS declared_reach,

            -- Only scorings made against a communication grid. Averaging every
            -- grid a person has ever been scored on would let a strong code
            -- reviewer lift this score, and the term claims to measure
            -- communication work.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN review_grids g ON g.id = rgs.grid_id
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'communication')
                AS review_grid_average,

            (SELECT count(DISTINCT o.id)
               FROM countable_deliverables d
               JOIN project_slices ps ON ps.id = d.slice_id
               JOIN orientations o ON o.id = ps.orientation_id
              WHERE d.user_id = $1 AND o.primary_domain = 'communication')
                AS orientations_distinct,

            -- Languages carried into, on translations that were actually
            -- validated. Counting the targets of unreviewed translations would
            -- let somebody claim five languages by typing five tags.
            (SELECT count(DISTINCT lang)
               FROM countable_deliverables d
               JOIN project_slices ps ON ps.id = d.slice_id
               JOIN attestations a
                 ON d.id = ANY (a.linked_deliverable_ids)
                AND a.basis = 'communication_translation_validated'
                AND a.revoked_at IS NULL
               CROSS JOIN LATERAL unnest(ps.communication_target_languages) AS lang
              WHERE d.user_id = $1)
                AS target_languages_distinct,

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
                AND (ct.skill_domain = 'communication'
                     OR o.primary_domain = 'communication'))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_communicator')
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
        "attestations_communication" => Some(m.attestations_communication as f64),
        "docs_contributions" => Some(m.docs_contributions as f64),
        "talks_delivered" => Some(m.talks_delivered as f64),
        "content_published" => Some(m.content_published as f64),
        "translations_validated" => Some(m.translations_validated as f64),
        "research_published" => Some(m.research_published as f64),
        "missions_completed" => Some(m.missions_completed as f64),
        "audience_reach" => Some(reach(m.verified_reach, m.declared_reach)),
        // Nobody has scored this person's communication work. Skipped rather
        // than counted as zero: an unscored average would subtract the whole
        // baseline from the total.
        "review_grid_average" => m.review_grid_average.as_ref().and_then(|a| a.to_f64()),
        "orientations_distinct" => Some(m.orientations_distinct as f64),
        "target_languages_distinct" => Some(m.target_languages_distinct as f64),
        "years_active" => Some(m.years_active as f64),
        "featured_times" => Some(m.featured_times as f64),
        unknown => {
            // Somebody added a row proposing a term. The answer to an
            // unimplemented proposal is silence in the total.
            tracing::warn!(
                term = unknown,
                "craft_score_weights names a communication term nothing knows how to count"
            );
            None
        }
    })
    .await
}

/// The audience figure the score uses, from the two kinds of number.
///
/// Declared figures are halved before the term's own logarithm is applied,
/// which moves them by a constant rather than by a proportion. Kept as its own
/// function so the trade-off is testable without a database.
fn reach(verified: i64, declared: i64) -> f64 {
    verified as f64 + (declared as f64 * DECLARED_REACH_DISCOUNT)
}

/// Compute and store, so a listing can sort without recomputing.
pub async fn recompute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let computed = compute(db, user_id).await?;
    craft_score::store(db, user_id, DOMAIN, computed.score, &computed.tier_slug).await?;
    Ok(computed)
}

/// Recompute everybody whose communication score is stale or was never
/// computed.
///
/// Bounded per pass and oldest first, like the other sweeps: one that tries
/// the whole table at once never reaches the end of the alphabet.
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
                "communication craft score recompute failed"
            ),
        }
    }
    metrics::counter!("skilluv_communication_craft_score_recomputed_total").increment(done);
    Ok(done)
}

// ═══════════════════════════════════════════════════════════════════
// The public profile
// ═══════════════════════════════════════════════════════════════════

/// One piece of published work, with enough to open it.
#[derive(Debug, Serialize, ToSchema)]
pub struct CommunicationHighlight {
    pub slice_id: Uuid,
    pub title: String,
    pub subtype: String,
    /// Where it lives publicly, or the pull request that carried it.
    pub url: Option<String>,
    /// Languages a translation was carried into. Empty for everything else.
    pub target_languages: Vec<String>,
    /// Readers or viewers, where the platform publishes the figure. Absent
    /// where it does not, which is not the same as zero.
    pub views: Option<i64>,
    /// Reactions, claps, comments.
    pub engagement: Option<i32>,
}

/// One language this person has been credited with translating into.
#[derive(Debug, Serialize, ToSchema)]
pub struct LanguageRow {
    pub language: String,
    /// How many validated translations into it.
    pub validated: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommunicationProfile {
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
    /// Languages they have validated translations into, most first.
    pub languages: Vec<LanguageRow>,
    /// Verified work worth reading first.
    pub highlights: Vec<CommunicationHighlight>,
}

/// Everything one person has to show in the communication trades.
///
/// Recomputes rather than reading the stored figure: the sweep runs hourly,
/// and a profile page showing an hour-old score is a page showing a revoked
/// attestation still counting.
pub async fn build(db: &PgPool, username: &str) -> Result<CommunicationProfile, AppError> {
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
          WHERE d.user_id = $1 AND o.primary_domain = 'communication'
          ORDER BY o.slug",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let languages = languages_for(db, user_id).await?;
    let highlights = highlights_for(db, user_id).await?;

    Ok(CommunicationProfile {
        username: username.to_string(),
        craft_score: score.score,
        tier: score.tier_slug,
        tier_name: score.tier_name,
        tier_description: score.tier_description,
        next_tier_at: score.next_tier_at,
        breakdown: score.breakdown,
        capped: score.capped,
        orientations,
        languages,
        highlights,
    })
}

/// Languages this person has validated translations into.
///
/// Only validated ones, for the same reason the score counts only those: the
/// target languages of an unreviewed translation are five tags somebody typed.
async fn languages_for(db: &PgPool, user_id: Uuid) -> Result<Vec<LanguageRow>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT lang, count(DISTINCT ps.id)
          FROM countable_deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
          JOIN attestations a
            ON d.id = ANY (a.linked_deliverable_ids)
           AND a.basis = 'communication_translation_validated'
           AND a.revoked_at IS NULL
          CROSS JOIN LATERAL unnest(ps.communication_target_languages) AS lang
         WHERE d.user_id = $1
         GROUP BY lang
         ORDER BY count(DISTINCT ps.id) DESC, lang
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(language, validated)| LanguageRow {
            language,
            validated,
        })
        .collect())
}

/// Verified communication work, most recent first, capped.
///
/// Only verified and unrevoked, for the same reason the score counts nothing
/// else: a profile listing work whose verification was withdrawn is a profile
/// making a claim the platform has retracted.
async fn highlights_for(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<CommunicationHighlight>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        slice_id: Uuid,
        title: String,
        subtype: String,
        url: Option<String>,
        target_languages: Vec<String>,
        views: Option<i64>,
        engagement: Option<i32>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT ps.id AS slice_id,
               ps.title,
               ps.communication_subtype AS subtype,
               COALESCE(ps.published_artifact_url, ps.pr_url) AS url,
               ps.communication_target_languages AS target_languages,
               (SELECT sum(s.views_count)::BIGINT FROM published_artifact_stats s
                 WHERE s.slice_id = ps.id) AS views,
               (SELECT sum(s.engagement_count)::INT FROM published_artifact_stats s
                 WHERE s.slice_id = ps.id) AS engagement
          FROM countable_deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
         WHERE d.user_id = $1 AND ps.slice_type = 'communication_artifact'
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
        .map(|r| CommunicationHighlight {
            slice_id: r.slice_id,
            title: r.title,
            subtype: r.subtype,
            url: r.url,
            target_languages: r.target_languages,
            views: r.views,
            engagement: r.engagement,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_audience_counts_for_half() {
        assert_eq!(reach(1_000, 0), 1_000.0);
        assert_eq!(reach(0, 1_000), 500.0);
        assert_eq!(reach(400, 200), 500.0);
    }

    #[test]
    fn nobody_with_no_audience_is_penalised() {
        // Zero is skipped by `assemble` rather than scored, which matters
        // because most technical bloggers publish on a domain of their own
        // that reports nothing.
        assert_eq!(reach(0, 0), 0.0);
    }
}
