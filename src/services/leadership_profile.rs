//! What somebody has actually led, and one number for it.
//!
//! ## Where the formula lives
//!
//! In `craft_score_weights`, as rows, the same as every other domain. This
//! module contributes what cannot be a row: what each term counts. The
//! arithmetic, the ceiling and the tier lookup are
//! [`craft_score::assemble`], shared so that "Senior" cannot come to mean two
//! things.
//!
//! ## The term this domain most needed
//!
//! `commitments_acknowledged`. Everything else here can be produced alone at
//! a desk: a roadmap, a decision record, a playbook. A commitment another
//! project's steward has read and accepted cannot, and it is the closest
//! thing leadership has to a merged pull request.
//!
//! ## What a public profile does not show
//!
//! Confidential artefacts are counted and never displayed. That is the point
//! of the redaction state: somebody who has spent five years writing internal
//! strategy has a score that says so, and a profile that shows what it is
//! allowed to. The alternative — refusing to count what cannot be shown — is
//! a platform where only the unemployed can build a record.
//!
//! Anonymised artefacts are displayed only once a reviewer has confirmed the
//! redaction. An unconfirmed one is counted by nothing and shown by nothing:
//! it is neither proof nor publishable until somebody other than its author
//! has read it.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::craft_score::{self, CraftScore};

pub const DOMAIN: &str = "leadership";

#[derive(Debug, Serialize)]
pub struct LeadershipProfile {
    pub username: String,
    pub display_name: Option<String>,
    pub orientations: Vec<serde_json::Value>,
    pub score: CraftScore,
    /// Documents a reader can open: public ones, and anonymised ones a
    /// reviewer has confirmed.
    pub artefacts: Vec<serde_json::Value>,
    /// What confidential work exists, said in the abstract. What kind, at
    /// what scale, in what industry — and never what or where.
    pub confidential_summary: Vec<serde_json::Value>,
    /// Cohorts led to their end, with the numbers that make the claim
    /// checkable: how many joined, how many finished.
    pub cohorts: Vec<serde_json::Value>,
    /// Retrospectives whose action items actually landed.
    pub retrospectives: Vec<serde_json::Value>,
    /// Which domains this person's verified leadership work was aimed at.
    pub target_domain_breakdown: Vec<serde_json::Value>,
    pub attestations: Vec<serde_json::Value>,
}

#[derive(sqlx::FromRow)]
struct Measurements {
    attestations_leadership: i64,
    roadmaps_validated: i64,
    decisions_recorded: i64,
    rfcs_accepted: i64,
    retrospectives_followed_through: i64,
    cohorts_completed: i64,
    mentees_graduated: i64,
    people_frameworks_validated: i64,
    playbooks_published: i64,
    community_initiatives_impact: i64,
    commitments_acknowledged: i64,
    missions_completed: i64,
    review_grid_average: Option<BigDecimal>,
    years_active: i64,
    featured_times: i64,
}

/// The nine bases this domain issues. A constant of this module, and the only
/// thing interpolated into the SQL below — everything a caller supplies is
/// bound.
const BASES: &str = "'leadership_roadmap_validated', 'leadership_decision_recorded', \
     'leadership_rfc_accepted', 'leadership_retrospective_facilitated', \
     'leadership_cohort_completed', 'leadership_playbook_published', \
     'leadership_people_framework_validated', \
     'leadership_community_initiative_impact', 'featured_leader'";

async fn measure(db: &PgPool, user_id: Uuid) -> Result<Measurements, AppError> {
    let sql = format!(
        r#"
        SELECT
            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL AND basis IN ({BASES}))
                AS attestations_leadership,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_roadmap_validated')
                AS roadmaps_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_decision_recorded')
                AS decisions_recorded,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_rfc_accepted')
                AS rfcs_accepted,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_retrospective_facilitated')
                AS retrospectives_followed_through,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_cohort_completed')
                AS cohorts_completed,

            -- People, not cohorts. A cohort of three and a cohort of twenty
            -- are not the same undertaking, and `cohorts_completed` alone
            -- says they are. Counted only on cohorts that were led to their
            -- end: graduating four people out of a run that collapsed is not
            -- four graduations.
            --
            -- `::BIGINT`, and it is not decoration. `sum()` over a bigint
            -- returns NUMERIC in PostgreSQL, every other figure in this row is
            -- a `count(*)` and therefore bigint, and sqlx does not widen — one
            -- numeric here made the whole row undecodable and the endpoint
            -- answered 500 to every call. `ops_profile` documents the same
            -- outage from the opposite direction, an `::INT` narrowing.
            (SELECT COALESCE(sum(o.graduated_total), 0)::BIGINT
               FROM cohort_outcomes o
              WHERE o.led_by_user_id = $1 AND o.led_to_the_end)
                AS mentees_graduated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_people_framework_validated')
                AS people_frameworks_validated,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_playbook_published')
                AS playbooks_published,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'leadership_community_initiative_impact')
                AS community_initiatives_impact,

            -- Read from the links rather than from an attestation, because
            -- there is no attestation for it: the acknowledgement is another
            -- person's act on this person's document, and it counts one by
            -- one.
            (SELECT count(*)
               FROM leadership_artifact_links l
               JOIN project_slices ps ON ps.id = l.leadership_slice_id
               JOIN deliverables d ON d.slice_id = ps.id
              WHERE d.user_id = $1
                AND d.verification_status = 'verified'
                AND d.revoked_at IS NULL
                AND l.link_kind IN ('commits', 'depends_on')
                AND l.acknowledged_at IS NOT NULL)
                AS commitments_acknowledged,

            (SELECT count(*) FROM missions m
               JOIN mission_types t ON t.id = m.mission_type_id
              WHERE m.assigned_user_id = $1
                AND m.status = 'closed'
                AND t.skill_domain = 'leadership')
                AS missions_completed,

            (SELECT avg(rgs.average)
               FROM review_grid_scores rgs
               JOIN reviews r ON r.id = rgs.review_id
               JOIN deliverables d ON d.id = r.deliverable_id
               JOIN review_grids g ON g.id = rgs.grid_id
              WHERE d.user_id = $1 AND d.revoked_at IS NULL
                AND g.domain = 'leadership')
                AS review_grid_average,

            -- BIGINT rather than INT: sqlx does not widen, and one int4 in a
            -- row of bigints makes the whole row undecodable. `ops_profile`
            -- documents the outage that taught us.
            (SELECT COALESCE(
                        date_part('year', age(NOW(), min(a.issued_at)))::BIGINT + 1,
                        0)
               FROM attestations a
              WHERE a.user_id = $1 AND a.revoked_at IS NULL
                AND a.basis IN ({BASES}))
                AS years_active,

            (SELECT count(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND basis = 'featured_leader')
                AS featured_times
        "#
    );

    sqlx::query_as::<_, Measurements>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(AppError::from)
}

/// Compute the leadership score without storing it.
pub async fn compute(db: &PgPool, user_id: Uuid) -> Result<CraftScore, AppError> {
    let m = measure(db, user_id).await?;
    let weights = craft_score::weights_for(db, DOMAIN).await?;

    craft_score::assemble(db, DOMAIN, weights, |term| {
        Some(match term {
            "attestations_leadership" => m.attestations_leadership as f64,
            "roadmaps_validated" => m.roadmaps_validated as f64,
            "decisions_recorded" => m.decisions_recorded as f64,
            "rfcs_accepted" => m.rfcs_accepted as f64,
            "retrospectives_followed_through" => m.retrospectives_followed_through as f64,
            "cohorts_completed" => m.cohorts_completed as f64,
            "mentees_graduated" => m.mentees_graduated as f64,
            "people_frameworks_validated" => m.people_frameworks_validated as f64,
            "playbooks_published" => m.playbooks_published as f64,
            "community_initiatives_impact" => m.community_initiatives_impact as f64,
            "commitments_acknowledged" => m.commitments_acknowledged as f64,
            "missions_completed" => m.missions_completed as f64,
            "review_grid_average" => {
                return m.review_grid_average.as_ref().and_then(|v| v.to_f64());
            }
            "years_active" => m.years_active as f64,
            "featured_times" => m.featured_times as f64,
            unknown => {
                tracing::warn!(
                    term = unknown,
                    "craft_score_weights names a leadership term nothing knows how to count"
                );
                return None;
            }
        })
    })
    .await
}

/// The public profile behind `/api/users/{username}/leadership-profile`.
pub async fn build(db: &PgPool, username: &str) -> Result<LeadershipProfile, AppError> {
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
            AND o.primary_domain = 'leadership'
          ORDER BY uo.is_primary DESC, uo.started_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // What can be shown: public, or anonymised and confirmed by somebody who
    // is not the author. An anonymised document nobody has checked is not
    // shown, whatever its author believes about it.
    let artefacts: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'title', ps.title,
                    'subtype', ps.leadership_subtype,
                    'redaction_state', ps.redaction_state,
                    'target_domain', ps.target_domain,
                    'adopted', ps.leadership_adopted_at IS NOT NULL,
                    'url', d.artifact_url,
                    'verified_at', d.verified_at)
           FROM project_slices ps
           JOIN deliverables d ON d.slice_id = ps.id
          WHERE d.user_id = $1
            AND ps.slice_type = 'leadership_artifact'
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
            AND (ps.redaction_state = 'public'
                 OR (ps.redaction_state = 'anonymised'
                     AND ps.redaction_confirmed_at IS NOT NULL))
          ORDER BY d.verified_at DESC
          LIMIT 25",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // The abstract claim, and nothing else. No title, no URL, no domain of
    // the client — the context object is what the author agreed could be
    // said, and a title is often enough to identify a product.
    let confidential_summary: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'subtype', ps.leadership_subtype,
                    'context', ps.leadership_context,
                    'verified_at', d.verified_at)
           FROM project_slices ps
           JOIN deliverables d ON d.slice_id = ps.id
          WHERE d.user_id = $1
            AND ps.slice_type = 'leadership_artifact'
            AND ps.redaction_state = 'confidential'
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
          ORDER BY d.verified_at DESC
          LIMIT 25",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // The denominator travels with the rate. A graduation figure over the
    // survivors is not a graduation figure, and a profile that showed one
    // would be publishing the number this domain refuses.
    let cohorts: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', o.slug,
                    'target_domain', o.target_domain,
                    'joined_total', o.joined_total,
                    'graduated_total', o.graduated_total,
                    'left_for_work', o.left_for_work,
                    'concluded_at', o.concluded_at,
                    'led_to_the_end', o.led_to_the_end)
           FROM cohort_outcomes o
          WHERE o.led_by_user_id = $1 AND o.concluded_at IS NOT NULL
          ORDER BY o.concluded_at DESC
          LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let retrospectives: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'held_on', f.held_on,
                    'actions_total', f.actions_total,
                    'actions_resolved_in_window', f.actions_resolved_in_window,
                    'followed_through', f.followed_through)
           FROM leadership_retrospective_followthrough f
          WHERE f.facilitator_user_id = $1
          ORDER BY f.held_on DESC
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
            AND ps.slice_type = 'leadership_artifact'
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

    Ok(LeadershipProfile {
        username,
        display_name,
        orientations,
        score: compute(db, user_id).await?,
        artefacts,
        confidential_summary,
        cohorts,
        retrospectives,
        target_domain_breakdown,
        attestations,
    })
}
