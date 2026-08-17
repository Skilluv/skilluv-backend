//! The craft score, for design (migration 0238).
//!
//! Same machinery as `craft_score`: weights are rows, tiers are rows, and the
//! breakdown is returned with the number so a profile can show "3 identités
//! livrées — 360 points" rather than a total nobody can argue with.
//!
//! ## Why this is a second module and not a branch
//!
//! The formula generalises; the counting does not. Almost every term here
//! reads a table code has no equivalent of, and the two that look shared are
//! the ones that differ most:
//!
//!   * `review_grid_average` reads `slice_validation_decisions.grid_scores`,
//!     not `review_grid_scores`. A design critique scores a version still in
//!     flight, before any deliverable exists; a code review scores a finished
//!     one. Same idea, two moments, two tables.
//!   * `missions_completed` is filtered through the mission's trade, because
//!     `missions` carries an orientation rather than a domain.
//!
//! A branch inside `craft_score` would have meant one function whose every
//! line was an `if domain ==`. Two modules reading their own rows is the
//! shape migration 0195 was built for.
//!
//! ## What is deliberately not counted
//!
//! Anything imported. Migration 0145 keeps external signals display-only, and
//! a score an imported portfolio could move would stop meaning "proven here".

use bigdecimal::{BigDecimal, ToPrimitive};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{CraftScore, Term, points_for};

/// The ceiling. Lower than code's, because design has fewer public counters
/// to accumulate — not because a designer is worth less. A ceiling that
/// nobody can reach measures nothing at the top.
pub const CAP: i32 = 8_000;

/// The domain this module scores.
pub const DOMAIN: &str = "design";

/// Rounds a deliverable has to have survived to count as converged. Three,
/// because two is a correction and three is a change of direction somebody
/// came back from.
const CONVERGENCE_ROUNDS: i16 = 3;

#[derive(sqlx::FromRow)]
struct WeightRow {
    term: String,
    weight: BigDecimal,
    kind: String,
    baseline: Option<BigDecimal>,
    explanation: String,
}

/// Everything the formula counts, in one round-trip.
///
/// One query rather than thirteen: this runs for every profile on a sweep,
/// and thirteen round-trips per person is the difference between a sweep that
/// finishes and one that does not.
#[derive(sqlx::FromRow)]
struct Measurements {
    deliverables_validated: i64,
    iterations_converged: i64,
    review_grid_average: Option<BigDecimal>,
    trades_distinct: i64,
    contests_won: i64,
    contests_entered: i64,
    jury_service: i64,
    brand_systems_delivered: i64,
    typefaces_released: i64,
    systems_adopted: i64,
    missions_completed: i64,
    years_active: i64,
    featured_times: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    sqlx::query_as::<_, Measurements>(
        r#"
        SELECT
            -- The disclosure view rather than the table: an artefact whose
            -- author never said whether an assistant helped stops counting
            -- when its window closes.
            (SELECT count(*) FROM countable_deliverables d
              WHERE d.user_id = $1 AND d.artifact_type = 'design_artifact')
                AS deliverables_validated,

            -- Validated after three critique rounds or more. Being told the
            -- direction is wrong and coming back is the harder thing, and a
            -- score that only counted first-round approvals would quietly
            -- favour the timid brief.
            (SELECT count(*) FROM countable_deliverables d
              WHERE d.user_id = $1
                AND d.artifact_type = 'design_artifact'
                AND EXISTS (
                    SELECT 1 FROM slice_validation_decisions v
                     WHERE v.slice_id = d.slice_id AND v.round >= $2))
                AS iterations_converged,

            -- Design grids live on the decision, not on a review: a critique
            -- scores a version still in flight. NULL when nobody has scored
            -- anything, which is not the same as having scored badly.
            (SELECT avg((v.grid_scores ->> 'average')::NUMERIC)
               FROM slice_validation_decisions v
               JOIN project_slices s ON s.id = v.slice_id
              WHERE s.claimed_by_user_id = $1
                AND v.grid_scores ? 'average')
                AS review_grid_average,

            -- Range. A type designer and a service designer share almost no
            -- craft, so breadth here says something volume cannot buy.
            (SELECT count(DISTINCT s.orientation_id)
               FROM countable_deliverables d
               JOIN project_slices s ON s.id = d.slice_id
              WHERE d.user_id = $1
                AND d.artifact_type = 'design_artifact'
                AND s.orientation_id IS NOT NULL)
                AS trades_distinct,

            (SELECT count(*) FROM tournament_participants p
               JOIN tournaments t ON t.id = p.tournament_id
              WHERE p.participant_type = 'user' AND p.participant_id = $1
                AND t.skill_domain = 'design' AND p.rank = 1)
                AS contests_won,

            (SELECT count(*) FROM tournament_submissions sub
               JOIN tournaments t ON t.id = sub.tournament_id
              WHERE sub.participant_type = 'user' AND sub.participant_id = $1
                AND t.skill_domain = 'design'
                AND sub.status NOT IN ('rejected', 'disqualified'))
                AS contests_entered,

            -- Per contest, not per score: judging twelve entries in one
            -- contest is one act of service.
            (SELECT count(DISTINCT j.tournament_id)
               FROM tournament_juries j
               JOIN tournaments t ON t.id = j.tournament_id
              WHERE j.juror_user_id = $1 AND j.accepted_at IS NOT NULL
                AND t.skill_domain = 'design')
                AS jury_service,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'design_brand_system_delivered')
                AS brand_systems_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'design_typeface_released')
                AS typefaces_released,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'design_system_adopted')
                AS systems_adopted,

            -- `missions` carries a trade rather than a domain, so the filter
            -- goes through the orientation.
            (SELECT count(*) FROM missions m
               LEFT JOIN orientations o ON o.id = m.orientation_id
              WHERE m.assigned_user_id = $1 AND m.status = 'closed'
                AND o.primary_domain = 'design')
                AS missions_completed,

            -- Whole years since the first verified design artefact. Somebody
            -- three months in scores zero, which is correct: the term rewards
            -- persistence, and three months is not persistence yet.
            (SELECT COALESCE(
                 EXTRACT(YEAR FROM age(NOW(), min(d.verified_at)))::BIGINT, 0)
               FROM countable_deliverables d
              WHERE d.user_id = $1 AND d.artifact_type = 'design_artifact')
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_designer')
                AS featured_times
        "#,
    )
    .bind(user_id)
    .bind(CONVERGENCE_ROUNDS)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
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
            "deliverables_validated" => m.deliverables_validated as f64,
            "iterations_converged" => m.iterations_converged as f64,
            "review_grid_average" => match &m.review_grid_average {
                Some(avg) => avg.to_f64().unwrap_or(0.0),
                // Nobody has scored this person's work. Skipped entirely
                // rather than counted as zero: an unscored average would
                // otherwise subtract the whole baseline from the total.
                None => continue,
            },
            "trades_distinct" => m.trades_distinct as f64,
            "contests_won" => m.contests_won as f64,
            "contests_entered" => m.contests_entered as f64,
            "jury_service" => m.jury_service as f64,
            "brand_systems_delivered" => m.brand_systems_delivered as f64,
            "typefaces_released" => m.typefaces_released as f64,
            "systems_adopted" => m.systems_adopted as f64,
            "missions_completed" => m.missions_completed as f64,
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            unknown => {
                // Somebody added a row proposing a term. The answer to an
                // unimplemented proposal is silence in the total.
                tracing::warn!(
                    term = unknown,
                    domain = DOMAIN,
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

/// Somebody's stored design score, without recomputing it.
///
/// Absent means never computed, which is not zero.
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

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn converging_is_worth_more_than_being_right_first_time() {
        // The weights say so, and this pins the intent: 20 for a validated
        // deliverable, 35 more when it took three rounds. Somebody who was
        // told their direction was wrong and came back scores higher than
        // somebody who was never challenged.
        let straight_through = points_for("count", 20.0, None, 1.0);
        let converged = straight_through + points_for("count", 35.0, None, 1.0);
        assert!(converged > straight_through * 2);
    }

    #[test]
    fn an_average_at_the_baseline_is_worth_nothing() {
        // Three out of five is the middle of the grid. Crediting it would pay
        // for showing up.
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 3.0), 0);
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 2.0), 0);
        assert_eq!(points_for("offset_scaled", 200.0, Some(3.0), 4.0), 200);
    }

    #[test]
    fn winning_is_worth_much_more_than_entering() {
        assert!(points_for("count", 150.0, None, 1.0) > points_for("count", 10.0, None, 5.0));
    }

    #[test]
    fn the_design_ceiling_is_reachable() {
        // A ceiling nobody reaches measures nothing at the top. Twenty-five
        // validated deliverables, five trades, three contest wins and a
        // decade of work should be inside it, not clipped by it.
        let plausible = points_for("count", 20.0, None, 25.0)
            + points_for("count", 60.0, None, 5.0)
            + points_for("count", 150.0, None, 3.0)
            + points_for("count", 25.0, None, 10.0);
        assert!(plausible < CAP, "a strong career must fit under the cap");
    }
}
