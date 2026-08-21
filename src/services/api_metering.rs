//! The metered public API: plans, quotas, and what a key is allowed to read.
//!
//! ## Two limits, not one
//!
//! A monthly quota is what the client bought. A daily ceiling stops one
//! runaway script spending the month in an afternoon — the client finds out on
//! day one instead of at the next invoice, which is the difference between a
//! bug and a bill.
//!
//! ## What the API may say about somebody
//!
//! Only what that person agreed to, purpose by purpose. The check is in
//! [`readable_profile`] and there is exactly one of it, so no endpoint can
//! invent its own slightly more generous version.
//!
//! A person who has not opted in is reported as not found rather than as
//! private. "This user exists but is not shareable" is itself information
//! about them, and a directory built from those answers is a directory of
//! everybody who declined.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The outcome of asking whether a call may proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowance {
    Allowed { remaining_today: Option<i64> },
    OverDaily { ceiling: i64 },
    OverMonthly { quota: i64 },
}

/// Whether one more call fits.
///
/// The daily ceiling is checked first: it is the one that produces a useful
/// message, because a client over the daily limit will be under it again
/// tomorrow and a client over the monthly one will not.
pub fn allowance(
    used_today: i64,
    used_this_month: i64,
    daily_ceiling: Option<i64>,
    monthly_quota: Option<i64>,
) -> Allowance {
    if let Some(ceiling) = daily_ceiling
        && used_today >= ceiling
    {
        return Allowance::OverDaily { ceiling };
    }
    if let Some(quota) = monthly_quota
        && used_this_month >= quota
    {
        return Allowance::OverMonthly { quota };
    }
    Allowance::Allowed {
        remaining_today: daily_ceiling.map(|c| (c - used_today).max(0)),
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Plan {
    pub slug: String,
    pub label: String,
    pub monthly_quota: Option<i32>,
    pub daily_ceiling: Option<i32>,
    pub monthly_fee: bigdecimal::BigDecimal,
    pub currency: String,
    pub attribution_required: bool,
    pub sla: bool,
}

pub async fn plans(db: &PgPool) -> Result<Vec<Plan>, AppError> {
    let rows = sqlx::query_as::<_, Plan>(
        "SELECT slug, label, monthly_quota, daily_ceiling, monthly_fee, currency,
                attribution_required, sla
           FROM api_plans WHERE is_active ORDER BY sort_order",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// A key that has been checked and is inside its limits.
#[derive(Debug, Clone)]
pub struct CallerKey {
    pub id: Uuid,
    pub enterprise_id: Option<Uuid>,
    pub plan: String,
    pub attribution_required: bool,
}

/// A key joined to its plan, as the authorisation query returns it.
#[derive(sqlx::FromRow)]
struct KeyRow {
    id: Uuid,
    enterprise_id: Option<Uuid>,
    key_hash: String,
    plan: String,
    attribution_required: bool,
    daily_ceiling: Option<i32>,
    monthly_quota: Option<i32>,
}

/// Resolve a presented key, check its limits, and count the call.
///
/// The count happens whether or not the call is allowed: a refused call still
/// costs us the lookup, and a client watching their throttled count is the
/// one who will upgrade rather than complain.
pub async fn authorise(db: &PgPool, presented: &str) -> Result<CallerKey, AppError> {
    let prefix: String = presented.chars().take(12).collect();

    let row: Option<KeyRow> = sqlx::query_as(
        "SELECT k.id, k.enterprise_id, k.key_hash, k.plan, p.attribution_required,
                p.daily_ceiling, p.monthly_quota
           FROM api_keys k
           JOIN api_plans p ON p.slug = k.plan
          WHERE k.key_prefix = $1 AND k.active AND k.revoked_at IS NULL",
    )
    .bind(&prefix)
    .fetch_optional(db)
    .await?;

    let KeyRow {
        id,
        enterprise_id,
        key_hash,
        plan,
        attribution_required,
        daily_ceiling: daily,
        monthly_quota: monthly,
    } = row.ok_or(AppError::Unauthorized)?;

    if !crate::services::AuthService::verify_password(presented, &key_hash)? {
        return Err(AppError::Unauthorized);
    }

    let (used_today, used_month) = usage(db, id).await?;

    match allowance(
        used_today,
        used_month,
        daily.map(|d| d as i64),
        monthly.map(|m| m as i64),
    ) {
        Allowance::Allowed { .. } => {
            count(db, id, true).await?;
            Ok(CallerKey {
                id,
                enterprise_id,
                plan,
                attribution_required,
            })
        }
        // 429 with the wait, which is what a machine client acts on. The
        // daily case says come back tomorrow and the monthly one says come
        // back next month; that difference is the whole point of two limits.
        Allowance::OverDaily { .. } => {
            count(db, id, false).await?;
            Err(AppError::RateLimited(seconds_until_tomorrow()))
        }
        Allowance::OverMonthly { .. } => {
            count(db, id, false).await?;
            Err(AppError::RateLimited(seconds_until_next_month()))
        }
    }
}

/// Seconds to the next UTC midnight, when the daily ceiling resets.
fn seconds_until_tomorrow() -> i64 {
    let now = chrono::Utc::now();
    let tomorrow = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .map(|d| d.and_utc());
    tomorrow
        .map(|t| (t - now).num_seconds().max(1))
        .unwrap_or(3600)
}

/// Seconds to the first of next month, when the quota resets.
fn seconds_until_next_month() -> i64 {
    use chrono::Datelike;
    let now = chrono::Utc::now();
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| (d.and_utc() - now).num_seconds().max(1))
        .unwrap_or(86_400)
}

async fn usage(db: &PgPool, key_id: Uuid) -> Result<(i64, i64), AppError> {
    let row: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT
             (SELECT requests FROM api_usage_daily
               WHERE api_key_id = $1 AND used_on = CURRENT_DATE)::BIGINT,
             (SELECT sum(requests) FROM api_usage_daily
               WHERE api_key_id = $1
                 AND used_on >= date_trunc('month', CURRENT_DATE)::DATE)::BIGINT",
    )
    .bind(key_id)
    .fetch_one(db)
    .await?;
    Ok((row.0.unwrap_or(0), row.1.unwrap_or(0)))
}

async fn count(db: &PgPool, key_id: Uuid, served: bool) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO api_usage_daily (api_key_id, used_on, requests, throttled)
         VALUES ($1, CURRENT_DATE, $2, $3)
         ON CONFLICT (api_key_id, used_on) DO UPDATE
             SET requests = api_usage_daily.requests + EXCLUDED.requests,
                 throttled = api_usage_daily.throttled + EXCLUDED.throttled",
    )
    .bind(key_id)
    .bind(i32::from(served))
    .bind(i32::from(!served))
    .execute(db)
    .await?;
    Ok(())
}

/// This month's usage for a key, for the client's own dashboard.
pub async fn month_to_date(db: &PgPool, key_id: Uuid) -> Result<(i64, i64), AppError> {
    let row: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT sum(requests)::BIGINT, sum(throttled)::BIGINT
           FROM api_usage_daily
          WHERE api_key_id = $1
            AND used_on >= date_trunc('month', CURRENT_DATE)::DATE",
    )
    .bind(key_id)
    .fetch_one(db)
    .await?;
    Ok((row.0.unwrap_or(0), row.1.unwrap_or(0)))
}

// ═══════════════════════════════════════════════════════════════════
// What the API may say
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct PublicProfile {
    pub username: String,
    pub domains: serde_json::Value,
    pub attestations_count: i64,
    pub attestations_url: String,
    pub last_active: Option<chrono::DateTime<chrono::Utc>>,
}

/// One person's public figures, if they agreed to be readable.
///
/// Returns `None` for somebody who has not opted in, and the caller turns
/// that into a plain not-found. Saying "exists but private" would let a
/// client enumerate everybody who declined, which is information about them
/// they did not agree to share either.
pub async fn readable_profile(
    db: &PgPool,
    username: &str,
    site_url: &str,
) -> Result<Option<PublicProfile>, AppError> {
    // `updated_at` rather than a last-seen column, which this schema does
    // not have. Named `last_active` in the response because that is what a
    // reader will take it for, and it is the closest true thing we hold.
    let user: Option<(Uuid, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT id, username, updated_at FROM users WHERE lower(username) = lower($1)",
    )
    .bind(username)
    .fetch_optional(db)
    .await?;

    let Some((user_id, username, last_active)) = user else {
        return Ok(None);
    };

    let allowed: bool = sqlx::query_scalar("SELECT has_data_consent($1, 'public_score_api')")
        .bind(user_id)
        .fetch_one(db)
        .await?;
    if !allowed {
        return Ok(None);
    }

    let domains: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT COALESCE(
                    jsonb_object_agg(
                        skill_domain,
                        jsonb_build_object('craft_score', score, 'tier', tier_slug)
                    ),
                    '{}'::jsonb
                )
           FROM craft_scores WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let attestations_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL AND public",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    Ok(Some(PublicProfile {
        attestations_url: format!("{site_url}/@{username}/attestations"),
        username,
        domains: domains.unwrap_or_else(|| serde_json::json!({})),
        attestations_count,
        last_active,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_inside_both_limits_is_allowed() {
        assert_eq!(
            allowance(10, 100, Some(1000), Some(10000)),
            Allowance::Allowed {
                remaining_today: Some(990)
            }
        );
    }

    #[test]
    fn the_daily_ceiling_is_reported_before_the_monthly_quota() {
        // The daily message is the useful one: that client will be under it
        // again tomorrow, and the monthly one will not.
        assert_eq!(
            allowance(1000, 1000, Some(1000), Some(10000)),
            Allowance::OverDaily { ceiling: 1000 }
        );
    }

    #[test]
    fn a_month_spent_stops_the_calls_even_on_a_fresh_day() {
        assert_eq!(
            allowance(0, 10000, Some(1000), Some(10000)),
            Allowance::OverMonthly { quota: 10000 }
        );
    }

    #[test]
    fn an_unmetered_plan_has_no_limits_to_be_over() {
        assert_eq!(
            allowance(1_000_000, 50_000_000, None, None),
            Allowance::Allowed {
                remaining_today: None
            }
        );
    }

    #[test]
    fn the_limit_is_the_last_allowed_call_not_the_first_refused_one() {
        // 999 used out of 1000 leaves exactly one.
        assert_eq!(
            allowance(999, 0, Some(1000), None),
            Allowance::Allowed {
                remaining_today: Some(1)
            }
        );
    }
}
