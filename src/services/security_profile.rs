//! What somebody has actually done in security, and one number for it.
//!
//! ## This is the "cyber trust score"
//!
//! Ticket A-02 asked for a composite score on the profile, stored on `users`
//! and recomputed by an hourly cron. Migration 0551 explains why the storage
//! and the hard-coded formula were both refused; this module is the other half
//! of that answer. The weights are rows, the arithmetic is
//! [`craft_score::assemble`] shared with every other domain, and what this file
//! contributes is the one thing that cannot be a row: what each term counts.
//!
//! Nothing is stored. The score is computed on read, which is the only way a
//! revoked proof can stop counting the moment it is revoked — and in a domain
//! whose entire product is "this can be checked", a number that outlives its
//! evidence is the failure being sold against.
//!
//! ## The terms that read somebody else's judgement
//!
//! `findings_high_or_critical` counts `severity_tier`, which is what a
//! validator settled, never `severity_reported_tier`, which is what the
//! reporter claimed. `co_credits` counts findings a person ruled duplicate.
//! `review_grid_average` is the marks received. Nothing in this score can be
//! raised by filing more of your own opinion, which is the property that makes
//! it worth showing a recruiter.
//!
//! ## Why duplicates are excluded from `findings_confirmed` and counted
//! separately
//!
//! Two people found the same thing. The first is credited with a finding; the
//! second did real work and is credited with a co-discovery worth a third as
//! much. Folding them together would make the count of confirmed
//! vulnerabilities in this domain larger than the number of vulnerabilities,
//! which is the sort of figure that gets a platform's numbers discounted
//! entirely.
//!
//! ## `review_grid_average` is skipped, not zeroed
//!
//! Somebody nobody has reviewed would otherwise have the whole baseline
//! subtracted from their total. `None` leaves the term out, which is what "not
//! measured yet" should do.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore};

pub const DOMAIN: &str = "security";

/// The seventeen bases this domain issues. Written once, used by the counts and
/// by the profile listing.
const BASES: &str = "'security_finding_confirmed', 'security_finding_published', \
     'security_finding_co_credit', 'security_ctf_solved', \
     'security_blue_lab_completed', 'security_machine_walkthrough_validated', \
     'security_training_completed', 'security_code_audit_delivered', \
     'security_threat_model_validated', 'security_detection_shipped', \
     'security_incident_analysis_validated', 'security_policy_validated', \
     'security_purple_exercise_facilitated', 'security_external_bounty_confirmed', \
     'security_competition_won', 'security_mission_delivered', \
     'featured_security_researcher'";

#[derive(Debug, Serialize)]
pub struct SecurityProfile {
    pub username: String,
    pub display_name: Option<String>,
    /// The security trades this person claims, from their orientations.
    pub orientations: Vec<serde_json::Value>,
    pub score: CraftScore,
    /// Findings a reviewer confirmed. Titles only for the published ones: the
    /// title of an embargoed finding is half the disclosure.
    pub findings: Vec<serde_json::Value>,
    /// Solved practice challenges, by kind and tier. A count rather than a
    /// list: forty rows of "solved a lab" is not a portfolio.
    pub practice: Vec<serde_json::Value>,
    pub attestations: Vec<serde_json::Value>,
    /// Declared certifications, each saying whether anybody checked it.
    pub credentials: Vec<serde_json::Value>,
    /// External reputation the person linked, with the same distinction between
    /// a figure a platform gave us and one they typed.
    pub external_platforms: Vec<serde_json::Value>,
}

/// Everything the security formula counts, in one round trip.
#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_security: i64,
    findings_confirmed: i64,
    findings_high_or_critical: i64,
    findings_published: i64,
    co_credits: i64,
    ctf_solved: i64,
    ctf_first_solves: i64,
    labs_completed: i64,
    walkthroughs_validated: i64,
    audits_delivered: i64,
    threat_models_validated: i64,
    incidents_analysed: i64,
    governance_artefacts_validated: i64,
    purple_exercises: i64,
    detections_shipped: i64,
    external_bounties_confirmed: i64,
    missions_completed: i64,
    review_grid_average: Option<BigDecimal>,
    years_active: i64,
    featured_times: i64,
}

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    // The basis list is interpolated because it is a compile-time constant of
    // this module — no request data reaches it. Everything a caller supplies is
    // bound.
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis IN ({BASES}))
                AS attestations_security,

            -- Read from the findings rather than from the attestations, so that
            -- the severity term below comes from the same rows as the count
            -- above it. Originals only.
            (SELECT count(*) FROM security_findings
              WHERE reporter_user_id = $1
                AND status IN ('confirmed', 'fixed', 'published')
                AND dedup_state <> 'duplicate_confirmed')
                AS findings_confirmed,

            -- The validator's severity, never the reporter's.
            (SELECT count(*) FROM security_findings
              WHERE reporter_user_id = $1
                AND status IN ('confirmed', 'fixed', 'published')
                AND dedup_state <> 'duplicate_confirmed'
                AND severity_tier IN ('critical', 'high'))
                AS findings_high_or_critical,

            (SELECT count(*) FROM security_findings
              WHERE reporter_user_id = $1 AND status = 'published')
                AS findings_published,

            (SELECT count(*) FROM security_findings
              WHERE reporter_user_id = $1
                AND dedup_state = 'duplicate_confirmed')
                AS co_credits,

            -- Solves, from the attempt rows. Not from the attestations: a
            -- captured flag issues one and this way the first-solve term below
            -- reads the same table.
            (SELECT count(*) FROM security_flag_attempts
              WHERE user_id = $1 AND correct)
                AS ctf_solved,

            (SELECT count(*) FROM (
                 SELECT DISTINCT ON (challenge_id) challenge_id, user_id
                   FROM security_flag_attempts
                  WHERE correct
                  ORDER BY challenge_id, attempted_at ASC) f
              WHERE f.user_id = $1)
                AS ctf_first_solves,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_blue_lab_completed')
                AS labs_completed,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis IN ('security_machine_walkthrough_validated',
                              'security_training_completed'))
                AS walkthroughs_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_code_audit_delivered')
                AS audits_delivered,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_threat_model_validated')
                AS threat_models_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_incident_analysis_validated')
                AS incidents_analysed,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_policy_validated')
                AS governance_artefacts_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_purple_exercise_facilitated')
                AS purple_exercises,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_detection_shipped')
                AS detections_shipped,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'security_external_bounty_confirmed')
                AS external_bounties_confirmed,

            (SELECT count(*) FROM missions m
               JOIN mission_types t ON t.id = m.mission_type_id
              WHERE m.assigned_user_id = $1
                AND m.status = 'closed'
                AND t.skill_domain = 'security')
                AS missions_completed,

            -- Scorings against a security grid only. An average across every
            -- domain would let a strong code reviewer's marks carry a security
            -- tier, which is the one thing a per-domain score exists to stop.
            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
               JOIN review_grids g ON g.id = rgs.grid_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'security')
                AS review_grid_average,

            -- BIGINT, not INT: every other figure here is a count(*), which
            -- PostgreSQL returns as bigint and sqlx does not widen. One ::INT
            -- makes the whole row undecodable and the endpoint answers 500 to
            -- every call — the failure the ops profile documents at length.
            (SELECT COALESCE(
                        date_part('year', age(NOW(), min(a.issued_at)))::BIGINT + 1,
                        0)
               FROM attestations a
              WHERE a.user_id = $1 AND a.revoked_at IS NULL
                AND a.basis IN ({BASES}))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_security_researcher')
                AS featured_times
        "#
    );

    sqlx::query_as::<_, Measurements>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(AppError::from)
}

/// Compute the security score without storing it.
pub async fn compute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let m = measure(db, user_id).await?;
    let weights = craft_score::weights_for(db, DOMAIN).await?;

    craft_score::assemble(db, DOMAIN, weights, |term| {
        Some(match term {
            "attestations_security" => m.attestations_security as f64,
            "findings_confirmed" => m.findings_confirmed as f64,
            "findings_high_or_critical" => m.findings_high_or_critical as f64,
            "findings_published" => m.findings_published as f64,
            "co_credits" => m.co_credits as f64,
            "ctf_solved" => m.ctf_solved as f64,
            "ctf_first_solves" => m.ctf_first_solves as f64,
            "labs_completed" => m.labs_completed as f64,
            "walkthroughs_validated" => m.walkthroughs_validated as f64,
            "audits_delivered" => m.audits_delivered as f64,
            "threat_models_validated" => m.threat_models_validated as f64,
            "incidents_analysed" => m.incidents_analysed as f64,
            "governance_artefacts_validated" => m.governance_artefacts_validated as f64,
            "purple_exercises" => m.purple_exercises as f64,
            "detections_shipped" => m.detections_shipped as f64,
            "external_bounties_confirmed" => m.external_bounties_confirmed as f64,
            "missions_completed" => m.missions_completed as f64,
            "review_grid_average" => {
                return m.review_grid_average.as_ref().and_then(|v| v.to_f64());
            }
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            unknown => {
                tracing::warn!(
                    term = unknown,
                    "craft_score_weights names a security term nothing knows how to count"
                );
                return None;
            }
        })
    })
    .await
}

/// The public profile behind `/api/users/{username}/security-profile`.
///
/// ## What is withheld, and why that is not a weaker profile
///
/// The title of an embargoed finding is half of its disclosure — "SQL injection
/// in the export endpoint" tells a reader where to look. So a confirmed finding
/// appears with its severity, its weakness class and its date, and its title
/// only once it is published. That is what a coordinated disclosure looks like
/// from outside, and it is what a recruiter needs: somebody found a critical in
/// March, and the details are not theirs to read yet.
///
/// Nothing unverified appears at all. A submitted report is a claim, and a
/// public profile is where a stranger forms a judgement.
pub async fn build(db: &PgPool, username: &str) -> Result<SecurityProfile, AppError> {
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
            AND o.primary_domain = 'security'
          ORDER BY uo.is_primary DESC, uo.started_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let findings: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', id,
                    'title', CASE WHEN status = 'published' THEN title END,
                    'severity_tier', severity_tier,
                    'cvss_score', cvss_score,
                    'cwe_id', cwe_id,
                    'status', status,
                    'disclosure_stage', disclosure_stage,
                    'target_kind', target_kind,
                    'writeup_url', CASE WHEN status = 'published'
                                        THEN writeup_url END,
                    'confirmed_month', to_char(created_at, 'YYYY-MM'))
           FROM security_findings
          WHERE reporter_user_id = $1
            AND status IN ('confirmed', 'fixed', 'published')
          ORDER BY
              CASE severity_tier
                  WHEN 'critical' THEN 5 WHEN 'high' THEN 4
                  WHEN 'medium' THEN 3 WHEN 'low' THEN 2 ELSE 1 END DESC,
              created_at DESC
          LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let practice: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'kind', c.security_kind,
                    'tier', c.security_difficulty_tier,
                    'solved', count(*))
           FROM challenge_submissions s
           JOIN challenge_templates c ON c.id = s.challenge_id
          WHERE s.user_id = $1 AND s.status = 'success'
            AND c.security_kind IS NOT NULL
          GROUP BY c.security_kind, c.security_difficulty_tier
          ORDER BY c.security_kind, c.security_difficulty_tier",
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

    // Declared, and whether anybody checked. A client asking for an OSCP is
    // entitled to know which of the two they are looking at.
    let credentials: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'issuer', issuer, 'name', name, 'level', level,
                    'issued_on', issued_on, 'expires_on', expires_on,
                    'evidence_url', evidence_url,
                    'verified', verified_at IS NOT NULL)
           FROM external_credentials
          WHERE user_id = $1
          ORDER BY verified_at IS NOT NULL DESC, issued_on DESC NULLS LAST",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let external_platforms: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'platform', p.slug, 'name', p.name,
                    'handle', e.handle, 'profile_url', e.profile_url,
                    'items_label', p.items_label, 'items', e.items_count,
                    'reach_label', p.reach_label, 'reach', e.reach_count,
                    'figures_are_declared', e.figures_are_declared,
                    'verified', e.verified_at IS NOT NULL,
                    'last_synced_at', e.last_synced_at)
           FROM user_external_portfolios e
           JOIN portfolio_platforms p ON p.slug = e.platform
          WHERE e.user_id = $1 AND p.skill_domain = 'security'
          ORDER BY p.sort_order",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(SecurityProfile {
        username,
        display_name,
        orientations,
        score: compute(db, user_id).await?,
        findings,
        practice,
        attestations,
        credentials,
        external_platforms,
    })
}
