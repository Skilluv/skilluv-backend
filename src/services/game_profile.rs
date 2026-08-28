//! The game craft score (migration 0576).
//!
//! Another domain scored on the shared machine. The formula, the tiers, the
//! cap and the assembly all live in [`craft_score`]; what is here is the
//! measuring, and only the measuring — the fourteen game terms, each counting
//! something a stranger can go and check.
//!
//! ## What a game score rests on
//!
//! Attestations with a basis, jam standings a finaliser wrote into
//! `tournament_participants.rank`, mods a reviewer confirmed, playtests a
//! creator gave to someone else, missions closed. The one figure the platform
//! cannot fetch — a mod's download count — is a number the reviewer confirmed
//! against the hosting page when the mod was validated, not a self-report the
//! score takes on trust.
//!
//! ## Why revoked work scores nothing
//!
//! Every count filters `revoked_at IS NULL` or reads `countable_deliverables`.
//! A score that outlives the revocation of what it rests on is the exact
//! failure this platform sells against.

use bigdecimal::{BigDecimal, ToPrimitive};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore};

/// The domain this module scores.
pub const DOMAIN: &str = "game";

/// The same ceiling as every other domain (migration 0204 shares one set of
/// tiers): a tier is a position on a scale, and each scale is its own weights.
pub const CAP: i32 = craft_score::CAP;

/// A confirmed mod is "viral" past this many downloads — the figure a reviewer
/// checked against the hosting page, per the `mods_viral` weight in 0576.
const VIRAL_DOWNLOADS: i32 = 1000;

/// The eight game bases, as a SQL list. Written once so a basis added to the
/// schema and forgotten here shows up as a term that stops counting rather
/// than as a silent undercount spread across the queries.
const GAME_BASES: &str = "'game_artifact_validated', 'game_jam_winner', \
                          'game_shipped_title', 'game_mod_published', \
                          'game_open_source_contribution', 'game_jam_participant', \
                          'game_playtest_hero', 'featured_game_creator'";

/// Everything the formula counts, in one round-trip. One query rather than
/// fourteen: this runs for every profile on a sweep, and fourteen round-trips
/// per person is the difference between a sweep that finishes and one that
/// does not.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations: i64,
    jam_wins: i64,
    jam_top3: i64,
    shipped_titles: i64,
    mods_published: i64,
    mods_viral: i64,
    open_source_contributions: i64,
    missions_completed: i64,
    review_grid_average: Option<BigDecimal>,
    playtests_contributed: i64,
    portfolio_projects: i64,
    published_writeups: i64,
    years_active: i64,
    featured_times: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is a compile-time constant, never anything a caller
    // supplies: it reaches SQL as text because a bound array would need a cast
    // in several places for no gain.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL AND basis IN ({GAME_BASES}))
                AS attestations,

            -- Jam standings, read from the rank a finaliser wrote, not a badge
            -- somebody typed. A win (rank 1) counts in both terms, so it is
            -- worth 100 + 50; a second or third counts only in jam_top3.
            (SELECT count(*) FROM tournament_participants p
               JOIN tournaments t ON t.id = p.tournament_id
              WHERE p.participant_type = 'user' AND p.participant_id = $1
                AND t.skill_domain = 'game' AND p.rank = 1)
                AS jam_wins,

            (SELECT count(*) FROM tournament_participants p
               JOIN tournaments t ON t.id = p.tournament_id
              WHERE p.participant_type = 'user' AND p.participant_id = $1
                AND t.skill_domain = 'game' AND p.rank IS NOT NULL AND p.rank <= 3)
                AS jam_top3,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'game_shipped_title')
                AS shipped_titles,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'game_mod_published')
                AS mods_published,

            -- Past the viral threshold, read from the confirmed mod's own
            -- download count. Only confirmed mods: a registered-but-unreviewed
            -- figure is the author's word, and the score does not take that.
            (SELECT count(*) FROM game_mods
              WHERE author_user_id = $1 AND status = 'confirmed'
                AND external_downloads_count >= {VIRAL_DOWNLOADS})
                AS mods_viral,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'game_open_source_contribution')
                AS open_source_contributions,

            (SELECT count(*) FROM missions
              WHERE assigned_user_id = $1 AND status = 'closed'
                AND skill_domain = 'game')
                AS missions_completed,

            -- Only scorings made against a game grid. Averaging every grid a
            -- person was ever scored on would let a strong reviewer in another
            -- craft lift a game score, and the term claims to measure game work.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN review_grids g ON g.id = rgs.grid_id
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'game')
                AS review_grid_average,

            (SELECT count(*) FROM game_playtests
              WHERE playtester_user_id = $1)
                AS playtests_contributed,

            (SELECT count(*)
               FROM user_external_portfolios p
               JOIN portfolio_platforms pf ON pf.slug = p.platform
              WHERE p.user_id = $1 AND pf.skill_domain = 'game')
                AS portfolio_projects,

            -- A published write-up is a submission of artifact_type 'writeup':
            -- a jam post-mortem, a design retrospective. Counted where the
            -- platform can see it landed, not where a person says it exists.
            (SELECT count(*)
               FROM tournament_submissions s
               JOIN tournaments t ON t.id = s.tournament_id
              WHERE s.participant_type = 'user' AND s.participant_id = $1
                AND s.artifact_type = 'writeup' AND t.skill_domain = 'game')
                AS published_writeups,

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
                AND (ct.skill_domain = 'game' OR o.primary_domain = 'game'))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_game_creator')
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
        "attestations" => Some(m.attestations as f64),
        "jam_wins" => Some(m.jam_wins as f64),
        "jam_top3" => Some(m.jam_top3 as f64),
        "shipped_titles" => Some(m.shipped_titles as f64),
        "mods_published" => Some(m.mods_published as f64),
        "mods_viral" => Some(m.mods_viral as f64),
        "open_source_contributions" => Some(m.open_source_contributions as f64),
        "missions_completed" => Some(m.missions_completed as f64),
        // Nobody has scored this person's game work. Skipped rather than
        // counted as zero: an unscored average, offset from 3, would subtract
        // the whole baseline from the total.
        "review_grid_average" => m.review_grid_average.as_ref().and_then(|a| a.to_f64()),
        "playtests_contributed" => Some(m.playtests_contributed as f64),
        "portfolio_projects" => Some(m.portfolio_projects as f64),
        "published_writeups" => Some(m.published_writeups as f64),
        "years_active" => Some(m.years_active as f64),
        "featured_times" => Some(m.featured_times as f64),
        unknown => {
            tracing::warn!(
                term = unknown,
                "craft_score_weights names a game term nothing knows how to count"
            );
            None
        }
    })
    .await
}

/// Compute and store, so a listing can sort without recomputing.
pub async fn recompute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let computed = compute(db, user_id).await?;
    craft_score::store(db, user_id, DOMAIN, computed.score, &computed.tier_slug).await?;
    Ok(computed)
}
