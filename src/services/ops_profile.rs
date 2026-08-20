//! What somebody has actually done in the ops trades, and one number for it.
//!
//! ## Where the formula lives
//!
//! In `craft_score_weights`, as rows, the same as the code domain. The
//! backlog proposed a formula in Rust and a `craft_score_ops` column; both
//! were declined for the same reason. A weight written in code is one nobody
//! outside the team can argue with, and a stored score is wrong from the
//! moment the next attestation lands — worse, it keeps its points when a
//! proof is revoked, unless somebody remembers to recompute.
//!
//! This module contributes the one thing that cannot be a row: what each term
//! counts. The arithmetic, the ceiling and the tier lookup are
//! [`craft_score::assemble`], shared with every other domain so that "Senior"
//! cannot come to mean two different things.
//!
//! ## Why revoked work scores nothing
//!
//! Every count filters `revoked_at IS NULL`. A score that survives the
//! revocation of what it rests on is the exact failure this platform sells
//! against.
//!
//! ## The two terms that are not counts
//!
//! `cost_saved_annual` is the total annual saving in euros, fed to a
//! logarithmic term: a million saved is worth about twice a thousand, not a
//! thousand times, because the second one is often just a bigger bill to
//! start with. `review_grid_average` is counted from three out of five, and
//! is skipped entirely for somebody nobody has reviewed — counting it as zero
//! would subtract the whole baseline from their total.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore};

pub const DOMAIN: &str = "ops";

#[derive(Debug, Serialize)]
pub struct OpsProfile {
    pub username: String,
    pub display_name: Option<String>,
    /// The ops trades this person claims, from their orientations.
    pub orientations: Vec<serde_json::Value>,
    pub score: CraftScore,
    /// Objectives held, with the figure and its source, so a reader can go
    /// and check rather than take the tier's word for it.
    pub objectives: Vec<serde_json::Value>,
    pub incidents: Vec<serde_json::Value>,
    pub cost_work: Vec<serde_json::Value>,
    pub attestations: Vec<serde_json::Value>,
    /// Certifications somebody else issued, verified and still valid. Listed
    /// after the attestations and never mixed into them: one is a thing
    /// Skilluv stands behind, the other is a thing Skilluv checked a link to.
    pub credentials: Vec<serde_json::Value>,
}

/// Everything the ops formula counts, in one round-trip.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_ops: i64,
    infra_artifacts_shipped: i64,
    objectives_met: i64,
    incidents_led: i64,
    migrations_completed: i64,
    observability_stacks_shipped: i64,
    cost_saved_annual: Option<BigDecimal>,
    platforms_distinct: i64,
    missions_completed: i64,
    review_grid_average: Option<BigDecimal>,
    years_active: i64,
    featured_times: i64,
    credentials_current: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    sqlx::query_as::<_, Measurements>(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis IN ('ops_infra_shipped', 'ops_uptime_achievement',
                              'ops_incident_led', 'ops_migration_completed',
                              'ops_observability_stack_shipped',
                              'ops_cost_optimization', 'featured_ops_engineer'))
                AS attestations_ops,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ops_infra_shipped')
                AS infra_artifacts_shipped,

            -- Verified windows only. An objective somebody closed themselves
            -- with a figure nobody checked is a claim, and the score is not
            -- where claims are settled.
            (SELECT count(*) FROM ops_service_objectives
              WHERE owner_user_id = $1
                AND verified_at IS NOT NULL
                AND achieved_percent IS NOT NULL
                AND achieved_percent >= target_percent)
                AS objectives_met,

            (SELECT count(*) FROM ops_incidents
              WHERE commander_user_id = $1
                AND postmortem_published_at IS NOT NULL)
                AS incidents_led,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ops_migration_completed')
                AS migrations_completed,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'ops_observability_stack_shipped')
                AS observability_stacks_shipped,

            -- Only reductions a reviewer confirmed still leave the service
            -- standing. A saving that broke it is an outage with a
            -- spreadsheet, and it must not score.
            (SELECT sum((monthly_before - monthly_after) * 12)
               FROM ops_cost_optimisations
              WHERE owner_user_id = $1
                AND verified_at IS NOT NULL
                AND service_still_meets_slo = TRUE)
                AS cost_saved_annual,

            -- Platforms an artefact of this person's actually runs on, from
            -- the slices behind their verified deliverables.
            (SELECT count(DISTINCT p)
               FROM deliverables d
               JOIN project_slices s ON s.id = d.slice_id
               CROSS JOIN LATERAL unnest(
                   COALESCE(s.ops_target_platforms, ARRAY[]::TEXT[])) AS p
              WHERE d.user_id = $1
                AND d.verification_status = 'verified'
                AND d.revoked_at IS NULL)
                AS platforms_distinct,

            (SELECT count(*) FROM missions m
               JOIN mission_types t ON t.id = m.mission_type_id
              WHERE m.assigned_user_id = $1
                AND m.status = 'closed'
                AND t.skill_domain = 'ops')
                AS missions_completed,

            -- Scorings against an ops grid only. The average across every
            -- domain would let a strong code reviewer's marks carry an ops
            -- tier, which is the one thing a per-domain score exists to stop.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
               JOIN review_grids g ON g.id = rgs.grid_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'ops')
                AS review_grid_average,

            -- BIGINT, not INT. Every other figure here is a `count(*)`, which
            -- PostgreSQL returns as bigint, and the struct reads them all as
            -- `i64`. This one went through `date_part`, so the cast decided
            -- the column's width — and `::INT` made it the one int4 in the
            -- row. sqlx does not widen: it refused to decode the whole row,
            -- so `GET /users/{username}/ops-profile` answered 500 to every
            -- call it has ever received. One test reaches this endpoint,
            -- which is why one test failed rather than the suite.
            (SELECT COALESCE(
                        date_part('year', age(NOW(), min(a.issued_at)))::BIGINT + 1,
                        0)
               FROM attestations a
              WHERE a.user_id = $1 AND a.revoked_at IS NULL
                AND a.basis LIKE 'ops%')
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_ops_engineer')
                AS featured_times,

            -- Verified and still valid. A lapsed certification scores
            -- nothing, and an unverified one has not been checked by anybody.
            (SELECT count(*) FROM credentials_with_currency
              WHERE user_id = $1
                AND verified_at IS NOT NULL
                AND is_current = TRUE)
                AS credentials_current
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(AppError::from)
}

/// Compute the ops score without storing it.
pub async fn compute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let m = measure(db, user_id).await?;

    let weights = craft_score::weights_for(db, DOMAIN).await?;

    craft_score::assemble(db, DOMAIN, weights, |term| {
        Some(match term {
            "attestations_ops" => m.attestations_ops as f64,
            "infra_artifacts_shipped" => m.infra_artifacts_shipped as f64,
            "objectives_met" => m.objectives_met as f64,
            "incidents_led" => m.incidents_led as f64,
            "migrations_completed" => m.migrations_completed as f64,
            "observability_stacks_shipped" => m.observability_stacks_shipped as f64,
            "cost_saved_annual" => {
                return m.cost_saved_annual.as_ref().and_then(|v| v.to_f64());
            }
            "platforms_distinct" => m.platforms_distinct as f64,
            "missions_completed" => m.missions_completed as f64,
            "review_grid_average" => {
                return m.review_grid_average.as_ref().and_then(|v| v.to_f64());
            }
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            "credentials_current" => m.credentials_current as f64,
            unknown => {
                tracing::warn!(
                    term = unknown,
                    "craft_score_weights names an ops term nothing knows how to count"
                );
                return None;
            }
        })
    })
    .await
}

/// The public profile behind `/api/users/{username}/ops-profile`.
///
/// Nothing here is private by accident: it reads the same rows the person's
/// own dashboard reads, minus anything unverified. An objective still open,
/// an incident with no post-mortem and a cost claim nobody checked are all
/// absent — a public profile is the place a stranger forms a judgement, and
/// it must only carry what a stranger could confirm.
pub async fn build(db: &PgPool, username: &str) -> Result<OpsProfile, AppError> {
    let user: Option<(Uuid, String, Option<String>)> =
        sqlx::query_as("SELECT id, username, display_name FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(db)
            .await?;

    let (user_id, username, display_name) =
        user.ok_or_else(|| AppError::NotFound("No such person".into()))?;

    let orientations: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('slug', o.slug, 'name', o.name,
                                   'reviewer_group', o.reviewer_group)
           FROM user_orientations uo
           JOIN orientations o ON o.id = uo.orientation_id
          WHERE uo.user_id = $1 AND uo.ended_at IS NULL
            AND o.primary_domain = 'ops'
          ORDER BY uo.is_primary DESC, uo.started_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let objectives: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'service_name', service_name,
                    'target_percent', target_percent,
                    'achieved_percent', achieved_percent,
                    'window_days', window_days,
                    'evidence_url', evidence_url,
                    'met', achieved_percent >= target_percent)
           FROM ops_service_objectives
          WHERE owner_user_id = $1 AND verified_at IS NOT NULL
          ORDER BY started_on DESC
          LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // No name of anybody appears here, and there is no column that could
    // carry one. What is published is what the system allowed.
    let incidents: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'severity', severity,
                    'time_to_detect_minutes', time_to_detect_minutes,
                    'time_to_resolve_minutes', time_to_resolve_minutes,
                    'postmortem_published_at', postmortem_published_at)
           FROM ops_incidents
          WHERE commander_user_id = $1 AND postmortem_published_at IS NOT NULL
          ORDER BY postmortem_published_at DESC
          LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let cost_work: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'scope', scope,
                    'monthly_before', monthly_before,
                    'monthly_after', monthly_after,
                    'currency', currency,
                    'service_still_meets_slo', service_still_meets_slo)
           FROM ops_cost_optimisations
          WHERE owner_user_id = $1 AND verified_at IS NOT NULL
          ORDER BY verified_at DESC
          LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let attestations: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'basis', basis, 'title', title,
                    'verification_code', verification_code,
                    'issued_at', issued_at)
           FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL
            AND (basis LIKE 'ops%' OR basis = 'featured_ops_engineer')
          ORDER BY issued_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let credentials: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'issuer', issuer, 'name', name, 'level', level,
                    'evidence_url', evidence_url,
                    'issued_on', issued_on, 'expires_on', expires_on)
           FROM credentials_with_currency
          WHERE user_id = $1 AND verified_at IS NOT NULL AND is_current = TRUE
          ORDER BY issued_on DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(OpsProfile {
        username,
        display_name,
        credentials,
        orientations,
        score: compute(db, user_id).await?,
        objectives,
        incidents,
        cost_work,
        attestations,
    })
}
