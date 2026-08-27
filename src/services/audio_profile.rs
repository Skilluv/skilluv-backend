//! The audio craft score (migration 0514).
//!
//! The third domain to score. The formula, the tiers, the cap and the
//! assembly are shared — [`craft_score::assemble`] was extracted when this
//! module was written, because three copies of the same loop is three places
//! for the cap or the skip-on-zero to drift, and the drift would be invisible:
//! a domain's score is only ever compared to itself.
//!
//! What is domain-specific is the measuring, and only the measuring.
//!
//! ## What the score counts, and one thing it deliberately does not
//!
//! Every term counts something a stranger can go and check: an attestation
//! with a basis, a mission closed, a review grid filled in. The exception is
//! `portfolio_reach`, which counts plays on platforms this codebase cannot
//! query — SoundCloud and Bandcamp publish no usable API — and where the
//! figures are often the person's own word.
//!
//! That is why declared figures are counted at a discount rather than
//! excluded. Excluding them would erase the entire recorded career of every
//! musician who works outside the two platforms with an API, which is most of
//! them. Counting them at face value would make the score a self-assessment.
//! Halving them, on a logarithmic scale, is the compromise that keeps a real
//! audience visible without letting a typed number outweigh a verified one.
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
pub const DOMAIN: &str = "audio";

/// The same ceiling as every other domain, and deliberately: migration 0204
/// shares one set of tiers across all of them, because a tier is a position on
/// a scale and each scale is calibrated by its own weights.
pub const CAP: i32 = craft_score::CAP;

/// What a declared audience figure is worth against a fetched one.
///
/// Applied before the logarithm, so it moves the term by a constant rather
/// than by a proportion — a declared million lands where a verified half
/// million would.
const DECLARED_REACH_DISCOUNT: f64 = 0.5;

/// Everything the formula counts, in one round-trip.
///
/// One query rather than thirteen: this runs for every profile on a sweep, and
/// thirteen round-trips per person is the difference between a sweep that
/// finishes and one that does not.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_audio: i64,
    compositions_published: i64,
    soundpacks_delivered: i64,
    voice_reels_validated: i64,
    adaptive_systems_shipped: i64,
    programming_contributions: i64,
    projects_credited: i64,
    missions_completed: i64,
    verified_reach: i64,
    declared_reach: i64,
    review_grid_average: Option<BigDecimal>,
    orientations_distinct: i64,
    years_active: i64,
    featured_times: i64,
}

/// The seven audio bases, as a SQL list. Written once so a basis added to the
/// schema and forgotten here shows up as a term that stops counting rather
/// than as a silent undercount spread across four queries.
const AUDIO_BASES: &str = "'audio_composition_published', 'audio_soundpack_delivered', \
                           'audio_voice_reel_validated', 'audio_adaptive_system_shipped', \
                           'audio_programming_contribution', 'audio_project_credited', \
                           'featured_audio_creator'";

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is a compile-time constant, never anything a caller
    // supplies: it reaches SQL as text because a bound array would need a cast
    // in seven places for no gain.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL AND basis IN ({AUDIO_BASES}))
                AS attestations_audio,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'audio_composition_published')
                AS compositions_published,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'audio_soundpack_delivered')
                AS soundpacks_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'audio_voice_reel_validated')
                AS voice_reels_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'audio_adaptive_system_shipped')
                AS adaptive_systems_shipped,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'audio_programming_contribution')
                AS programming_contributions,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'audio_project_credited')
                AS projects_credited,

            (SELECT count(*) FROM missions
              WHERE assigned_user_id = $1 AND status = 'closed'
                AND skill_domain = 'audio')
                AS missions_completed,

            -- Plays, split by whether anything checked them. NULL stays out of
            -- the sum: a platform that publishes nothing must not read as zero.
            (SELECT COALESCE(sum(p.reach_count), 0)::BIGINT
               FROM user_external_portfolios p
               JOIN portfolio_platforms pf ON pf.slug = p.platform
              WHERE p.user_id = $1 AND pf.skill_domain = 'audio'
                AND NOT p.figures_are_declared)
                AS verified_reach,

            (SELECT COALESCE(sum(p.reach_count), 0)::BIGINT
               FROM user_external_portfolios p
               JOIN portfolio_platforms pf ON pf.slug = p.platform
              WHERE p.user_id = $1 AND pf.skill_domain = 'audio'
                AND p.figures_are_declared)
                AS declared_reach,

            -- Only scorings made against an audio grid. Averaging every grid a
            -- person has ever been scored on would let a strong code reviewer
            -- lift an audio score, and the term claims to measure audio work.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN review_grids g ON g.id = rgs.grid_id
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'audio')
                AS review_grid_average,

            (SELECT count(DISTINCT o.id)
               FROM countable_deliverables d
               JOIN project_slices ps ON ps.id = d.slice_id
               JOIN orientations o ON o.id = ps.orientation_id
              WHERE d.user_id = $1 AND o.primary_domain = 'audio')
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
                AND (ct.skill_domain = 'audio' OR o.primary_domain = 'audio'))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_audio_creator')
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
        "attestations_audio" => Some(m.attestations_audio as f64),
        "compositions_published" => Some(m.compositions_published as f64),
        "soundpacks_delivered" => Some(m.soundpacks_delivered as f64),
        "voice_reels_validated" => Some(m.voice_reels_validated as f64),
        "adaptive_systems_shipped" => Some(m.adaptive_systems_shipped as f64),
        "programming_contributions" => Some(m.programming_contributions as f64),
        "projects_credited" => Some(m.projects_credited as f64),
        "missions_completed" => Some(m.missions_completed as f64),
        "portfolio_reach" => Some(reach(m.verified_reach, m.declared_reach)),
        // Nobody has scored this person's audio work. Skipped rather than
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
                "craft_score_weights names an audio term nothing knows how to count"
            );
            None
        }
    })
    .await
}

/// The audience figure the score uses, from the two kinds of number.
///
/// Declared plays are halved before the term's own logarithm is applied, which
/// moves them by a constant rather than by a proportion. Kept as its own
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

/// Recompute everybody whose audio score is stale or was never computed.
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
            Err(e) => {
                tracing::error!(user = %user_id, error = %e, "audio craft score recompute failed")
            }
        }
    }
    metrics::counter!("skilluv_audio_craft_score_recomputed_total").increment(done);
    Ok(done)
}

// ═══════════════════════════════════════════════════════════════════
// The public profile
// ═══════════════════════════════════════════════════════════════════

/// One piece of published work, with enough to play it.
#[derive(Debug, Serialize, ToSchema)]
pub struct AudioHighlight {
    pub slice_id: Uuid,
    pub title: String,
    pub subtype: String,
    /// What the work is for — a game, a montage, a podcast, an interface.
    pub destination: Option<String>,
    /// Where it lives publicly, when the author named somewhere.
    pub external_url: Option<String>,
    /// Length of the longest master, in seconds. What a reader needs to decide
    /// whether to press play.
    pub duration_seconds: Option<i32>,
    /// Whether a generated preview exists to play without downloading a
    /// two-hundred-megabyte master.
    pub has_preview: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AudioProfile {
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
    /// client that reads `score` on any profile reads it on all eight. The flat
    /// fields above are the earlier shape, kept until the front moves off them.
    pub score: CraftScore,
    /// Verified work worth listening to first.
    pub highlights: Vec<AudioHighlight>,
}

/// Everything one person has to show in the audio trades.
///
/// Recomputes rather than reading the stored figure: the sweep runs hourly,
/// and a profile page showing an hour-old score is a page showing a revoked
/// attestation still counting.
pub async fn build(db: &PgPool, username: &str) -> Result<AudioProfile, AppError> {
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
          WHERE d.user_id = $1 AND o.primary_domain = 'audio'
          ORDER BY o.slug",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let highlights = highlights_for(db, user_id).await?;

    Ok(AudioProfile {
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
        highlights,
    })
}

/// Verified audio work, most recent first, capped.
///
/// Only verified and unrevoked, for the same reason the score counts nothing
/// else: a profile that plays work whose verification was withdrawn is a
/// profile making a claim the platform has retracted.
async fn highlights_for(db: &PgPool, user_id: Uuid) -> Result<Vec<AudioHighlight>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        slice_id: Uuid,
        title: String,
        subtype: String,
        destination: Option<String>,
        external_url: Option<String>,
        duration_ms: Option<i32>,
        has_preview: bool,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT ps.id AS slice_id,
               ps.title,
               ps.audio_subtype AS subtype,
               ps.audio_destination AS destination,
               ps.audio_external_hosting_url AS external_url,
               (SELECT max(f.duration_ms) FROM audio_artifact_files f
                 WHERE f.slice_id = ps.id AND f.role = 'master') AS duration_ms,
               EXISTS (SELECT 1 FROM audio_artifact_files f
                        WHERE f.slice_id = ps.id AND f.role = 'preview') AS has_preview
          FROM countable_deliverables d
          JOIN project_slices ps ON ps.id = d.slice_id
         WHERE d.user_id = $1 AND ps.slice_type = 'audio_artifact'
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
        .map(|r| AudioHighlight {
            slice_id: r.slice_id,
            title: r.title,
            subtype: r.subtype,
            destination: r.destination,
            external_url: r.external_url,
            duration_seconds: r.duration_ms.map(|ms| ms / 1000),
            has_preview: r.has_preview,
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
        // because most of this domain works on platforms that publish nothing.
        assert_eq!(reach(0, 0), 0.0);
    }
}
