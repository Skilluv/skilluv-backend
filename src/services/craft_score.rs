//! The craft score (migration 0195).
//!
//! One number per person per domain, computed from counts that each have a
//! link behind them. Its job is to make a profile sortable; it is never a
//! substitute for reading the artefacts, and the breakdown is returned
//! alongside it for exactly that reason.
//!
//! ## Why the breakdown is part of the answer
//!
//! A score with no explanation is a number somebody has to trust. Every call
//! here returns what was counted and what each count was worth, so the
//! profile can show "3 bibliothèques publiées — 90 points" rather than "1240".
//! That is also the only honest way to publish a formula that will change.
//!
//! ## Where the weights come from
//!
//! Rows in `craft_score_weights`, read on every computation. A term the
//! service does not know how to count is skipped and logged rather than
//! guessed at: somebody adding a row is proposing a term, and the answer to
//! an unimplemented proposal is silence in the total, not an invented number.

use bigdecimal::{BigDecimal, ToPrimitive};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The ceiling. Higher than the other domains' because code has more ways of
/// leaving a trace, not because a coder is worth more.
pub const CAP: i32 = 10_000;

/// The domain this module scores. Named rather than inlined because the
/// storage and the formula are both keyed by it (migration 0204), and the
/// next domain is a second module reading its own weights rather than a
/// branch in this one.
pub const DOMAIN: &str = "code";

#[derive(Debug, Clone, Serialize)]
pub struct Term {
    pub term: String,
    /// What was counted. A whole number for `count` terms, the raw figure for
    /// the scaled ones.
    pub measured: f64,
    pub points: i32,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CraftScore {
    pub score: i32,
    pub tier_slug: String,
    pub tier_name: String,
    pub tier_description: String,
    /// The score at which the next tier starts, absent at the top.
    pub next_tier_at: Option<i32>,
    pub breakdown: Vec<Term>,
    /// True when the total hit the ceiling. Said out loud rather than left to
    /// be inferred from a round number.
    pub capped: bool,
}

#[derive(sqlx::FromRow)]
struct WeightRow {
    term: String,
    weight: BigDecimal,
    kind: String,
    baseline: Option<BigDecimal>,
    explanation: String,
}

/// Everything the formula counts, gathered in one round-trip.
///
/// One query rather than thirteen: this runs for every profile on an hourly
/// sweep, and thirteen round-trips per person is the difference between a
/// sweep that finishes and one that does not.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_code: i64,
    prs_merged_upstream: i64,
    projects_shipped: i64,
    libraries_published: i64,
    library_downloads: i64,
    rfcs_accepted: i64,
    standard_contributions: i64,
    devtools_adopted: i64,
    missions_completed: i64,
    review_grid_average: Option<BigDecimal>,
    languages_distinct: i64,
    years_active: i64,
    featured_times: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    sqlx::query_as::<_, Measurements>(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL AND basis IS NOT NULL)
                AS attestations_code,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'code_pr_merged_upstream')
                AS prs_merged_upstream,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'code_project_shipped')
                AS projects_shipped,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'code_library_published')
                AS libraries_published,

            -- Downloads across every package attached to a slice this person
            -- delivered. Recent figures where the registry publishes them,
            -- lifetime totals otherwise; NULL stays out of the sum, because a
            -- registry that measures nothing must not read as zero.
            -- Cast: summing bigints gives a NUMERIC, and decoding that into
            -- an i64 fails at runtime rather than at compile time.
            (SELECT COALESCE(sum(COALESCE(ps.downloads_recent, ps.downloads_total)), 0)::BIGINT
               FROM code_package_stats ps
               -- The disclosure view rather than the table: an artefact
               -- whose author never said whether an assistant helped stops
               -- counting when its window closes.
               JOIN countable_deliverables d ON d.slice_id = ps.slice_id
              WHERE d.user_id = $1)
                AS library_downloads,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'code_rfc_accepted')
                AS rfcs_accepted,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'code_standard_contribution')
                AS standard_contributions,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'code_devtool_adopted')
                AS devtools_adopted,

            (SELECT count(*) FROM missions
              WHERE assigned_user_id = $1 AND status = 'closed')
                AS missions_completed,

            -- The average of every grid scoring on this person's work. NULL
            -- when nobody has scored anything, which is not the same as
            -- having scored badly.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL)
                AS review_grid_average,

            (SELECT count(DISTINCT lang)
               FROM countable_deliverables d
               LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
               LEFT JOIN project_slices s ON s.id = d.slice_id
               CROSS JOIN LATERAL (
                   SELECT COALESCE(
                       NULLIF(ct.language, ''),
                       (SELECT sl FROM unnest(s.code_languages) AS sl LIMIT 1)
                   ) AS lang
               ) AS picked
              WHERE d.user_id = $1
                AND picked.lang IS NOT NULL)
                AS languages_distinct,

            -- Whole years since the first verified artefact. Somebody three
            -- months in scores zero here, which is correct: the term is meant
            -- to reward persistence, and three months is not persistence yet.
            (SELECT COALESCE(
                 EXTRACT(YEAR FROM age(NOW(), min(d.verified_at)))::BIGINT, 0)
               FROM countable_deliverables d
              WHERE d.user_id = $1)
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL AND basis = 'featured_coder')
                AS featured_times
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

/// What one term is worth, given what was measured.
///
/// Split out and tested: the three kinds are the part of the formula where a
/// mistake is silent — a log term that credits a thousand times too much
/// looks like a very good developer rather than like a bug.
pub fn points_for(kind: &str, weight: f64, baseline: Option<f64>, measured: f64) -> i32 {
    let raw = match kind {
        "count" => weight * measured,
        // log10(1 + n): ten downloads is worth about one weight, a thousand
        // about three, a million about six. A linear term here would make
        // download count the only thing the score measured.
        "log_scaled" => {
            if measured <= 0.0 {
                0.0
            } else {
                weight * (1.0 + measured).log10()
            }
        }
        // Counted from the baseline, and never below zero: a grid average of
        // 2 means the work needs another pass, not that the person owes the
        // platform points.
        "offset_scaled" => {
            let baseline = baseline.unwrap_or(0.0);
            (weight * (measured - baseline)).max(0.0)
        }
        _ => 0.0,
    };
    raw.round().max(0.0) as i32
}

/// Compute the score without storing it.
pub async fn compute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let weights = sqlx::query_as::<_, WeightRow>(
        "SELECT term, weight, kind, baseline, explanation
           FROM craft_score_weights
          WHERE skill_domain = $1 AND is_active = TRUE
          ORDER BY sort_order, term",
    )
    .bind(DOMAIN)
    .fetch_all(db)
    .await?;

    let m = measure(db, user_id).await?;

    let mut breakdown = Vec::new();
    let mut total: i64 = 0;

    for w in weights {
        let measured: f64 = match w.term.as_str() {
            "attestations_code" => m.attestations_code as f64,
            "prs_merged_upstream" => m.prs_merged_upstream as f64,
            "projects_shipped" => m.projects_shipped as f64,
            "libraries_published" => m.libraries_published as f64,
            "library_downloads" => m.library_downloads as f64,
            "rfcs_accepted" => m.rfcs_accepted as f64,
            "standard_contributions" => m.standard_contributions as f64,
            "devtools_adopted" => m.devtools_adopted as f64,
            "missions_completed" => m.missions_completed as f64,
            "review_grid_average" => match &m.review_grid_average {
                Some(avg) => avg.to_f64().unwrap_or(0.0),
                // Nobody has scored this person's work. Skipped entirely
                // rather than counted as zero: an unscored average would
                // otherwise subtract the whole baseline from the total.
                None => continue,
            },
            "languages_distinct" => m.languages_distinct as f64,
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            unknown => {
                // Somebody added a row proposing a term. The answer to an
                // unimplemented proposal is silence in the total.
                tracing::warn!(
                    term = unknown,
                    "craft_score_weights names a term nothing knows how to count"
                );
                continue;
            }
        };

        if measured == 0.0 {
            continue;
        }

        let points = points_for(
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

    let tier: Option<(String, String, String, Option<i32>)> = sqlx::query_as(
        "SELECT slug, name, description,
                (SELECT min(t2.min_score) FROM craft_score_tiers t2
                  WHERE t2.skill_domain = $2 AND t2.min_score > $1)
           FROM craft_score_tiers
          WHERE skill_domain = $2
            AND min_score <= $1
            AND (max_score IS NULL OR max_score >= $1)
          ORDER BY min_score DESC
          LIMIT 1",
    )
    .bind(score)
    .bind(DOMAIN)
    .fetch_optional(db)
    .await?;

    let (tier_slug, tier_name, tier_description, next_tier_at) = tier
        .ok_or_else(|| AppError::Internal(format!("no {DOMAIN} tier covers a score of {score}")))?;

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

/// Compute and store, with the tier resolved in the same write.
///
/// The tier duplicates what the score and the thresholds imply. Written here
/// so there is exactly one place that can get the duplicate wrong, rather
/// than a search endpoint recomputing it per row.
pub async fn recompute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let computed = compute(db, user_id).await?;
    sqlx::query(
        "INSERT INTO craft_scores (user_id, skill_domain, score, tier_slug, computed_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (user_id, skill_domain) DO UPDATE
             SET score = EXCLUDED.score,
                 tier_slug = EXCLUDED.tier_slug,
                 computed_at = NOW()",
    )
    .bind(user_id)
    .bind(DOMAIN)
    .bind(computed.score)
    .bind(&computed.tier_slug)
    .execute(db)
    .await?;
    Ok(computed)
}

/// Somebody's stored score in this domain, without recomputing it.
///
/// What a listing reads. Absent means never computed, which is not zero.
pub async fn stored(db: &PgPool, user_id: Uuid) -> Result<Option<(i32, String)>, AppError> {
    let row: Option<(i32, Option<String>)> = sqlx::query_as(
        "SELECT score, tier_slug FROM craft_scores
          WHERE user_id = $1 AND skill_domain = $2",
    )
    .bind(user_id)
    .bind(DOMAIN)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(score, tier)| (score, tier.unwrap_or_else(|| "apprentice".into()))))
}

/// Recompute everybody whose score is stale or has never been computed.
///
/// Run hourly. Bounded per pass, ordered oldest first: a sweep that tries to
/// do the whole table in one go is one that times out and never gets to the
/// end of the alphabet.
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
            Err(e) => tracing::error!(user = %user_id, error = %e, "craft score recompute failed"),
        }
    }
    metrics::counter!("skilluv_craft_score_recomputed_total").increment(done);
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_term_is_multiplication() {
        assert_eq!(points_for("count", 15.0, None, 4.0), 60);
        assert_eq!(points_for("count", 200.0, None, 1.0), 200);
        assert_eq!(points_for("count", 15.0, None, 0.0), 0);
    }

    #[test]
    fn downloads_are_scaled_so_they_do_not_swallow_the_score() {
        // With weight 50: ten downloads ≈ 52, a thousand ≈ 150, a million
        // ≈ 300. Linear, a million downloads would be fifty million points
        // and nothing else in the formula would exist.
        let ten = points_for("log_scaled", 50.0, None, 10.0);
        let thousand = points_for("log_scaled", 50.0, None, 1_000.0);
        let million = points_for("log_scaled", 50.0, None, 1_000_000.0);

        assert!(ten < thousand && thousand < million, "more is still more");
        assert!(
            million < CAP,
            "no single term may reach the ceiling on its own"
        );
        assert!(
            million < thousand * 3,
            "a million is not a thousand thousands"
        );
    }

    #[test]
    fn nothing_downloaded_scores_nothing_rather_than_minus_infinity() {
        assert_eq!(points_for("log_scaled", 50.0, None, 0.0), 0);
        assert_eq!(points_for("log_scaled", 50.0, None, -1.0), 0);
    }

    #[test]
    fn the_middle_of_the_grid_is_worth_nothing() {
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 3.0), 0);
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 4.0), 200);
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 4.5), 300);
    }

    #[test]
    fn a_bad_average_does_not_go_negative() {
        // Work that needs another pass is not a debt owed to the platform.
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 1.0), 0);
    }

    #[test]
    fn an_unknown_kind_contributes_nothing() {
        assert_eq!(points_for("something_new", 100.0, None, 5.0), 0);
    }
}
