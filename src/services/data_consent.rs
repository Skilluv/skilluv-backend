//! Who agreed to what, and the profile that rests on it.
//!
//! Every product in the data line describes somebody who is not the customer.
//! This module is the gate all of them go through, and it has one rule: the
//! answer is no until a person says otherwise, per purpose, in their own
//! session.
//!
//! ## Why per purpose
//!
//! Agreeing to appear in a public score API is not agreeing to be sold to a
//! bank. A single "share my data" switch would have made those the same
//! decision, and the person who set it for the first reason would have been
//! sold under the second.
//!
//! ## Why the wording is copied
//!
//! A purpose's description will be improved — clearer, longer, more honest.
//! Consent given to the old wording was not given to the new one, so the text
//! agreed to is stored on the consent row. What can be shown back in an audit
//! is what was actually on screen.
//!
//! ## Why revocation keeps the row
//!
//! A revoked consent proves consent existed for the period a dataset was
//! built in. Deleting it would make that unprovable in exactly the audit
//! where it matters.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const PURPOSES: &[&str] = &[
    "public_score_api",
    "research_licensing",
    "commercial_licensing",
    "identity_aggregation",
];

/// The floor below which an aggregate names the people in it.
///
/// Thirty is not a magic number, but it is the one every figure in this
/// codebase is held to, and having one number written once is worth more than
/// having the best number written five times.
pub const COHORT_FLOOR: i64 = 30;

/// Whether a figure may be published.
///
/// A "skills gap in Cotonou" chart drawn from four people names those four,
/// whatever the header says.
pub fn cohort_is_publishable(cohort_size: i64) -> bool {
    cohort_size >= COHORT_FLOOR
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Purpose {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub commercial: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Consent {
    pub purpose: String,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revenue_share_percent: bigdecimal::BigDecimal,
    pub wording_agreed: String,
}

pub async fn purposes(db: &PgPool) -> Result<Vec<Purpose>, AppError> {
    let rows = sqlx::query_as::<_, Purpose>(
        "SELECT slug, label, description, commercial FROM data_purposes
          ORDER BY commercial, slug",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Everything one person has been asked, and how they answered.
///
/// Revoked rows included on purpose: somebody who turned something off should
/// see that they did, and when.
pub async fn for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<Consent>, AppError> {
    let rows = sqlx::query_as::<_, Consent>(
        "SELECT purpose, granted_at, revoked_at, revenue_share_percent, wording_agreed
           FROM talent_data_consent WHERE user_id = $1 ORDER BY purpose",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Say yes.
///
/// The wording is read from the purpose and copied onto the row, rather than
/// taken from the caller: a client that sent its own text could record
/// agreement to something nobody was shown.
pub async fn grant(db: &PgPool, user_id: Uuid, purpose: &str) -> Result<(), AppError> {
    if !PURPOSES.contains(&purpose) {
        return Err(AppError::Validation(format!(
            "purpose must be one of: {}",
            PURPOSES.join(", ")
        )));
    }

    let wording: Option<String> =
        sqlx::query_scalar("SELECT description FROM data_purposes WHERE slug = $1")
            .bind(purpose)
            .fetch_optional(db)
            .await?;
    let wording = wording.ok_or_else(|| AppError::NotFound("no such purpose".into()))?;

    sqlx::query(
        "INSERT INTO talent_data_consent (user_id, purpose, wording_agreed)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, purpose) DO UPDATE
             SET granted_at = NOW(),
                 revoked_at = NULL,
                 -- Re-consenting is consent to what is on screen now, not to
                 -- whatever was there the first time.
                 wording_agreed = EXCLUDED.wording_agreed",
    )
    .bind(user_id)
    .bind(purpose)
    .bind(&wording)
    .execute(db)
    .await?;

    metrics::counter!("skilluv_data_consent_granted_total", "purpose" => purpose.to_string())
        .increment(1);
    Ok(())
}

/// Say no, or stop saying yes.
///
/// Takes effect immediately for everything not already built: a dataset
/// shipped last month cannot be unshipped, and pretending otherwise would be
/// the dishonest part.
pub async fn revoke(db: &PgPool, user_id: Uuid, purpose: &str) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE talent_data_consent SET revoked_at = NOW()
          WHERE user_id = $1 AND purpose = $2 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(purpose)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you have not agreed to that, so there is nothing to withdraw".into(),
        ));
    }

    metrics::counter!("skilluv_data_consent_revoked_total", "purpose" => purpose.to_string())
        .increment(1);
    Ok(())
}

/// The single answer to "may we".
pub async fn allows(db: &PgPool, user_id: Uuid, purpose: &str) -> Result<bool, AppError> {
    let allowed: bool = sqlx::query_scalar("SELECT has_data_consent($1, $2)")
        .bind(user_id)
        .bind(purpose)
        .fetch_one(db)
        .await?;
    Ok(allowed)
}

/// How many people are covered by a purpose right now.
pub async fn cohort_size(db: &PgPool, purpose: &str) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM talent_data_consent
          WHERE purpose = $1 AND revoked_at IS NULL",
    )
    .bind(purpose)
    .fetch_one(db)
    .await?;
    Ok(count)
}

// ═══════════════════════════════════════════════════════════════════
// The unified profile
// ═══════════════════════════════════════════════════════════════════

/// How the parts add up.
///
/// Deliberately flat and legible rather than clever. A score somebody cannot
/// reconstruct from their own profile page is a score they cannot argue with,
/// and this one is going to be shown to banks.
pub fn aggregate(
    craft_score_total: i64,
    attestations: i64,
    verified_signals: i64,
    merged_contributions: i64,
) -> i64 {
    craft_score_total + attestations * 50 + verified_signals * 25 + merged_contributions * 30
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UnifiedScore {
    pub user_id: Uuid,
    pub aggregate_score: i32,
    pub platforms_covered: Vec<String>,
    pub breakdown: serde_json::Value,
    pub last_computed_at: chrono::DateTime<chrono::Utc>,
}

/// Recompute one person's unified profile.
///
/// Runs on a schedule and on demand, never on read: a bank's query must not
/// cost a hundred joins, and a figure that changes between two reads of the
/// same page is a figure nobody trusts.
pub async fn recompute(db: &PgPool, user_id: Uuid) -> Result<UnifiedScore, AppError> {
    let craft: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(sum(score), 0)::BIGINT FROM craft_scores WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let craft = craft.unwrap_or(0);

    let attestations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let signals: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT provider FROM external_signals
          WHERE user_id = $1 AND verified_at IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let verified_signals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_signals
          WHERE user_id = $1 AND verified_at IS NOT NULL",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let merged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND basis = 'code_pr_merged_upstream'
            AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let score = aggregate(craft, attestations, verified_signals, merged);

    // Skilluv itself always counts when there is anything of ours in the
    // figure; otherwise a score built entirely here would claim no sources.
    let mut platforms = signals;
    if craft > 0 || attestations > 0 {
        platforms.push("skilluv".into());
    }
    platforms.sort();
    platforms.dedup();

    let breakdown = serde_json::json!({
        "craft_score": craft,
        "attestations": attestations,
        "verified_signals": verified_signals,
        "merged_contributions": merged,
    });

    sqlx::query(
        "INSERT INTO unified_identity_scores
            (user_id, aggregate_score, platforms_covered, breakdown, last_computed_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (user_id) DO UPDATE
             SET aggregate_score = EXCLUDED.aggregate_score,
                 platforms_covered = EXCLUDED.platforms_covered,
                 breakdown = EXCLUDED.breakdown,
                 last_computed_at = NOW()",
    )
    .bind(user_id)
    .bind(score.min(i32::MAX as i64) as i32)
    .bind(&platforms)
    .bind(&breakdown)
    .execute(db)
    .await?;

    unified_score(db, user_id).await
}

pub async fn unified_score(db: &PgPool, user_id: Uuid) -> Result<UnifiedScore, AppError> {
    sqlx::query_as::<_, UnifiedScore>(
        "SELECT user_id, aggregate_score, platforms_covered, breakdown, last_computed_at
           FROM unified_identity_scores WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("no unified profile yet".into()))
}

/// The partners one person has allowed, named one by one.
///
/// "A bank" and "any bank" are not the same permission, and a single flag
/// would have made them the same.
pub async fn allowed_partners(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT partner_slug FROM identity_licensing_partners
          WHERE user_id = $1 AND revoked_at IS NULL ORDER BY partner_slug",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartnerInput {
    pub partner_slug: String,
    pub allow: bool,
}

pub async fn set_partner(db: &PgPool, user_id: Uuid, input: PartnerInput) -> Result<(), AppError> {
    let slug = input.partner_slug.trim();
    if slug.is_empty() || slug.len() > 60 {
        return Err(AppError::Validation("name the partner".into()));
    }

    // Naming a partner before agreeing to the aggregation at all would be
    // consent to a use of something that does not exist yet.
    if input.allow && !allows(db, user_id, "identity_aggregation").await? {
        return Err(AppError::Validation(
            "agree to the unified profile first — naming a partner for something you \
             have not allowed is not a decision anybody could act on"
                .into(),
        ));
    }

    if input.allow {
        sqlx::query(
            "INSERT INTO identity_licensing_partners (user_id, partner_slug)
             VALUES ($1, $2)
             ON CONFLICT (user_id, partner_slug) DO UPDATE SET revoked_at = NULL,
                 allowed_at = NOW()",
        )
        .bind(user_id)
        .bind(slug)
        .execute(db)
        .await?;
    } else {
        sqlx::query(
            "UPDATE identity_licensing_partners SET revoked_at = NOW()
              WHERE user_id = $1 AND partner_slug = $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(slug)
        .execute(db)
        .await?;
    }

    Ok(())
}

/// Whether a named partner may read this person's unified profile.
///
/// Both gates: the purpose, and the partner. Either one missing is a no.
pub async fn partner_may_read(
    db: &PgPool,
    user_id: Uuid,
    partner_slug: &str,
) -> Result<bool, AppError> {
    if !allows(db, user_id, "identity_aggregation").await? {
        return Ok(false);
    }
    let named: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM identity_licensing_partners
              WHERE user_id = $1 AND partner_slug = $2 AND revoked_at IS NULL
         )",
    )
    .bind(user_id)
    .bind(partner_slug)
    .fetch_one(db)
    .await?;
    Ok(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_resting_on_four_people_is_not_publishable() {
        // It names those four, whatever the header says.
        assert!(!cohort_is_publishable(4));
        assert!(!cohort_is_publishable(29));
        assert!(cohort_is_publishable(30));
        assert!(cohort_is_publishable(4000));
    }

    #[test]
    fn an_empty_cohort_is_not_publishable() {
        assert!(!cohort_is_publishable(0));
    }

    #[test]
    fn every_part_of_the_score_adds_and_none_subtracts() {
        let base = aggregate(1000, 0, 0, 0);
        assert_eq!(base, 1000);
        assert!(aggregate(1000, 1, 0, 0) > base);
        assert!(aggregate(1000, 0, 1, 0) > base);
        assert!(aggregate(1000, 0, 0, 1) > base);
    }

    #[test]
    fn a_merged_contribution_counts_more_than_a_self_declared_signal() {
        // One is somebody else's decision; the other is a link the person
        // supplied. Weighting them equally would make the score bluffable.
        let merged = aggregate(0, 0, 0, 1);
        let signal = aggregate(0, 0, 1, 0);
        assert!(merged > signal);
    }

    #[test]
    fn an_empty_profile_scores_nothing() {
        assert_eq!(aggregate(0, 0, 0, 0), 0);
    }

    #[test]
    fn every_purpose_is_a_known_one() {
        assert_eq!(PURPOSES.len(), 4);
        assert!(PURPOSES.contains(&"commercial_licensing"));
    }
}
