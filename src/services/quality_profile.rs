//! What somebody has actually done in the quality trades, and one number
//! for it.
//!
//! ## Where the formula lives
//!
//! In `craft_score_weights`, as rows, the same as every other domain. The
//! backlog proposed a formula in Rust and a `craft_score_qa` column; both were
//! declined for the reason the ops profile gives: a weight written in code is
//! one nobody outside the team can argue with, and a stored score is wrong
//! from the moment the next attestation lands — worse, it keeps its points
//! when a proof is revoked, unless somebody remembers to recompute.
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
//! ## The term that reads somebody else's opinion
//!
//! `critical_bugs_confirmed` counts the reviewer's severity, not the
//! reporter's. Self-rated severity is a self-service multiplier, and every bug
//! bounty programme has already found that out. A report nobody has reviewed
//! counts towards `bugs_confirmed` and not towards this one — its severity is
//! a claim until somebody has read it.
//!
//! ## `review_grid_average` is skipped, not zeroed
//!
//! It is counted from three out of five, so somebody nobody has reviewed would
//! have the whole baseline subtracted from their total if it were treated as
//! zero. Returning `None` leaves the term out, which is what "not measured
//! yet" should do.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore};

pub const DOMAIN: &str = "quality";

#[derive(Debug, Serialize)]
pub struct QualityProfile {
    pub username: String,
    pub display_name: Option<String>,
    /// The quality trades this person claims, from their orientations.
    pub orientations: Vec<serde_json::Value>,
    pub score: CraftScore,
    /// Confirmed defect reports: the ones whose fix shipped and was
    /// re-checked. Listed with the fix link, so a reader can go and see the
    /// change rather than take the count's word for it.
    pub confirmed_bugs: Vec<serde_json::Value>,
    /// Which domains this person's verified work was aimed at, and how much
    /// of it. The breakdown the backlog asked for in W-05, on the column that
    /// records it.
    pub target_domain_breakdown: Vec<serde_json::Value>,
    pub attestations: Vec<serde_json::Value>,
    /// Test runs a reviewer verified. Never the unverified ones: anybody can
    /// point at a green badge on a repository they control.
    pub verified_test_runs: Vec<serde_json::Value>,
}

/// Everything the quality formula counts, in one round-trip.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_quality: i64,
    test_plans_validated: i64,
    test_strategies_validated: i64,
    automation_suites_shipped: i64,
    bugs_confirmed: i64,
    critical_bugs_confirmed: i64,
    usability_studies_completed: i64,
    a11y_audits_delivered: i64,
    playtests_facilitated: i64,
    coverage_analyses_accepted: i64,
    target_domains_distinct: i64,
    missions_completed: i64,
    review_grid_average: Option<BigDecimal>,
    years_active: i64,
    featured_times: i64,
}

/// The nine bases this domain issues. Written once here rather than repeated
/// in every count below, and used by the profile query too.
const BASES: &str = "'quality_test_plan_validated', 'quality_test_strategy_validated', \
     'quality_automation_shipped', 'quality_bug_report_validated', \
     'quality_usability_study_completed', 'quality_a11y_audit_delivered', \
     'quality_playtest_report_validated', 'quality_coverage_analysis_accepted', \
     'featured_quality_engineer'";

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is interpolated because it is a compile-time constant of
    // this module — no request data reaches it. Everything a caller supplies
    // is bound.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis IN ({BASES}))
                AS attestations_quality,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_test_plan_validated')
                AS test_plans_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_test_strategy_validated')
                AS test_strategies_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_automation_shipped')
                AS automation_suites_shipped,

            -- Read from the reports rather than from the attestations. The
            -- attestation is issued once per slice; a slice can hold one
            -- report, so the two agree — and this way the severity below
            -- comes from the same rows as the count above it.
            (SELECT count(*) FROM quality_bug_reports
              WHERE reporter_user_id = $1
                AND fix_confirmed_at IS NOT NULL
                AND rejected_reason IS NULL)
                AS bugs_confirmed,

            -- The reviewer's severity, never the reporter's. A report nobody
            -- has read has a severity that is still a claim, and it counts
            -- towards the line above rather than towards this one.
            (SELECT count(*) FROM quality_bug_reports
              WHERE reporter_user_id = $1
                AND fix_confirmed_at IS NOT NULL
                AND rejected_reason IS NULL
                AND reviewed_at IS NOT NULL
                AND COALESCE(severity_adjusted_to, severity) IN ('critical', 'high'))
                AS critical_bugs_confirmed,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_usability_study_completed')
                AS usability_studies_completed,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_a11y_audit_delivered')
                AS a11y_audits_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_playtest_report_validated')
                AS playtests_facilitated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'quality_coverage_analysis_accepted')
                AS coverage_analyses_accepted,

            -- Distinct domains this person's verified quality work was aimed
            -- at. NULL is cross-domain and is not counted: one artefact that
            -- targets everything must not satisfy a term about breadth.
            (SELECT count(DISTINCT ps.target_domain)
               FROM deliverables d
               JOIN project_slices ps ON ps.id = d.slice_id
              WHERE d.user_id = $1
                AND d.verification_status = 'verified'
                AND d.revoked_at IS NULL
                AND ps.slice_type = 'qa_report'
                AND ps.target_domain IS NOT NULL)
                AS target_domains_distinct,

            (SELECT count(*) FROM missions m
               JOIN mission_types t ON t.id = m.mission_type_id
              WHERE m.assigned_user_id = $1
                AND m.status = 'closed'
                AND t.skill_domain = 'quality')
                AS missions_completed,

            -- Scorings against a quality grid only. The average across every
            -- domain would let a strong code reviewer's marks carry a quality
            -- tier, which is the one thing a per-domain score exists to stop.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
               JOIN review_grids g ON g.id = rgs.grid_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'quality')
                AS review_grid_average,

            -- BIGINT rather than INT, for the reason `ops_profile` documents
            -- at length: every other figure here is a `count(*)`, which
            -- PostgreSQL returns as bigint, and sqlx does not widen. One
            -- `::INT` here made the whole row undecodable and the endpoint
            -- answered 500 to every call.
            (SELECT COALESCE(
                        date_part('year', age(NOW(), min(a.issued_at)))::BIGINT + 1,
                        0)
               FROM attestations a
              WHERE a.user_id = $1 AND a.revoked_at IS NULL
                AND a.basis IN ({BASES}))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_quality_engineer')
                AS featured_times
        "#
    );

    sqlx::query_as::<_, Measurements>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(AppError::from)
}

/// Compute the quality score without storing it.
pub async fn compute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let m = measure(db, user_id).await?;
    let weights = craft_score::weights_for(db, DOMAIN).await?;

    craft_score::assemble(db, DOMAIN, weights, |term| {
        Some(match term {
            "attestations_quality" => m.attestations_quality as f64,
            "test_plans_validated" => m.test_plans_validated as f64,
            "test_strategies_validated" => m.test_strategies_validated as f64,
            "automation_suites_shipped" => m.automation_suites_shipped as f64,
            "bugs_confirmed" => m.bugs_confirmed as f64,
            "critical_bugs_confirmed" => m.critical_bugs_confirmed as f64,
            "usability_studies_completed" => m.usability_studies_completed as f64,
            "a11y_audits_delivered" => m.a11y_audits_delivered as f64,
            "playtests_facilitated" => m.playtests_facilitated as f64,
            "coverage_analyses_accepted" => m.coverage_analyses_accepted as f64,
            "target_domains_distinct" => m.target_domains_distinct as f64,
            "missions_completed" => m.missions_completed as f64,
            "review_grid_average" => {
                return m.review_grid_average.as_ref().and_then(|v| v.to_f64());
            }
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            unknown => {
                tracing::warn!(
                    term = unknown,
                    "craft_score_weights names a quality term nothing knows how to count"
                );
                return None;
            }
        })
    })
    .await
}

/// The public profile behind `/api/users/{username}/quality-profile`.
///
/// Nothing here is private by accident: it reads the same rows the person's
/// own dashboard reads, minus anything unverified. A report nobody reviewed, a
/// fix nobody confirmed and a test run nobody checked are all absent — a
/// public profile is where a stranger forms a judgement, and it must only
/// carry what a stranger could confirm.
///
/// Session recordings never appear, whatever their status. They belong to the
/// participants who consented to one use of them, and a portfolio is not that
/// use.
pub async fn build(db: &PgPool, username: &str) -> Result<QualityProfile, AppError> {
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
            AND o.primary_domain = 'quality'
          ORDER BY uo.is_primary DESC, uo.started_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // Confirmed only, and the fix link with them. The title is included and
    // the reproduction is not: a public list of reproductions for defects in
    // other people's products would be a disclosure channel nobody agreed to.
    let confirmed_bugs: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'title', title,
                    'severity', COALESCE(severity_adjusted_to, severity),
                    'severity_reviewed', reviewed_at IS NOT NULL,
                    'reproducibility', reproducibility,
                    'fix_url', fix_url,
                    'fix_confirmed_at', fix_confirmed_at)
           FROM quality_bug_reports
          WHERE reporter_user_id = $1
            AND fix_confirmed_at IS NOT NULL
            AND rejected_reason IS NULL
          ORDER BY fix_confirmed_at DESC
          LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let target_domain_breakdown: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'target_domain', ps.target_domain,
                    'name', sd.name,
                    'artefacts', count(*))
           FROM deliverables d
           JOIN project_slices ps ON ps.id = d.slice_id
           JOIN skill_domains sd ON sd.slug = ps.target_domain
          WHERE d.user_id = $1
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
            AND ps.slice_type = 'qa_report'
          GROUP BY ps.target_domain, sd.name, sd.sort_order
          ORDER BY sd.sort_order",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let attestations: Vec<serde_json::Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT jsonb_build_object(
                    'basis', basis, 'title', title,
                    'verification_code', verification_code,
                    'issued_at', issued_at)
           FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL
            AND basis IN ({BASES})
          ORDER BY issued_at DESC"
    )))
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let verified_test_runs: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'source', r.source,
                    'report_url', r.report_url,
                    'tests_total', r.tests_total,
                    'tests_failed', r.tests_failed,
                    'coverage_percent', r.coverage_percent,
                    'imported_at', r.imported_at)
           FROM quality_test_runs r
          WHERE r.imported_by = $1 AND r.verified_at IS NOT NULL
          ORDER BY r.imported_at DESC
          LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(QualityProfile {
        username,
        display_name,
        orientations,
        score: compute(db, user_id).await?,
        confirmed_bugs,
        target_domain_breakdown,
        attestations,
        verified_test_runs,
    })
}
