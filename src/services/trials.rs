//! Paid weeks before a hire.
//!
//! ## What a trial is for
//!
//! A recruitment that fails at month three costs the company a salary and the
//! person a job they left another one for. A trial makes the same discovery in
//! three weeks, with the person paid for the work and both sides free to walk
//! away.
//!
//! It is only honest if the work is real and the pay is real. A "trial" of
//! unpaid exercises is an interview with extra steps, and this module refuses
//! to represent one: the hourly rate is required and positive.
//!
//! ## The reduced fee afterwards
//!
//! A trial that converts costs the client less than a direct hire, because
//! the trial already de-risked it for both sides and because the client has
//! already paid for the weeks. That reduction is the incentive that makes
//! anybody use this rather than hiring on a conversation.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// What Skilluv keeps on the hours worked.
pub const DEFAULT_PLATFORM_FEE: f64 = 15.0;

/// The success fee if a trial converts, against 10-15% for a direct hire.
pub const CONVERTED_SUCCESS_FEE: f64 = 8.0;

pub const OUTCOMES: &[&str] = &[
    "ongoing",
    "converted_hire",
    "declined_by_enterprise",
    "declined_by_talent",
    "lapsed",
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Trial {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub talent_user_id: Uuid,
    pub username: String,
    pub duration_weeks: i16,
    pub hourly_rate: BigDecimal,
    pub currency: String,
    pub platform_fee_percent: BigDecimal,
    pub converted_success_fee_percent: Option<BigDecimal>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub outcome: Option<String>,
    /// Approved hours only. Claimed-but-unapproved is not money owed, and
    /// showing it as a total would be showing a figure nobody agreed to.
    pub approved_hours: BigDecimal,
    pub pending_hours: BigDecimal,
}

const TRIAL_SELECT: &str = r#"
    SELECT t.id, t.enterprise_id, t.talent_user_id, u.username,
           t.duration_weeks, t.hourly_rate, t.currency, t.platform_fee_percent,
           t.converted_success_fee_percent, t.started_at, t.ends_at,
           t.ended_at, t.outcome,
           COALESCE((SELECT sum(h.hours) FROM recruitment_trial_hours h
                      WHERE h.trial_id = t.id AND h.approved_at IS NOT NULL), 0)
               AS approved_hours,
           COALESCE((SELECT sum(h.hours) FROM recruitment_trial_hours h
                      WHERE h.trial_id = t.id
                        AND h.approved_at IS NULL AND h.rejected_at IS NULL), 0)
               AS pending_hours
      FROM recruitment_trials t
      JOIN users u ON u.id = t.talent_user_id
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct StartInput {
    pub talent_user_id: Uuid,
    #[serde(default)]
    pub campaign_id: Option<Uuid>,
    pub duration_weeks: i16,
    pub hourly_rate: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

/// What the hours will cost the client in total, at most.
///
/// Computed from the duration at a full working week, because a client
/// agreeing to a trial is agreeing to an exposure and should see it before
/// signing rather than discover it on the first invoice.
pub fn maximum_cost(duration_weeks: i16, hourly_rate: &BigDecimal) -> BigDecimal {
    let hours = BigDecimal::from(duration_weeks.max(0) as i64 * 35);
    (hourly_rate * hours).with_scale_round(2, bigdecimal::RoundingMode::Up)
}

/// Split an amount of hours into what the talent receives and what the
/// platform keeps.
///
/// Rounded down for the platform, like every other share it takes: rounding
/// up means keeping a centime nobody agreed to, out of somebody's wages.
pub fn split(
    hours: &BigDecimal,
    hourly_rate: &BigDecimal,
    fee_percent: &BigDecimal,
) -> (BigDecimal, BigDecimal) {
    let gross = (hours * hourly_rate).with_scale_round(2, bigdecimal::RoundingMode::HalfUp);
    let platform = (&gross * fee_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);
    let talent = &gross - &platform;
    (talent, platform)
}

/// Start a trial.
pub async fn start(db: &PgPool, enterprise_id: Uuid, input: StartInput) -> Result<Trial, AppError> {
    if !(1..=8).contains(&input.duration_weeks) {
        return Err(AppError::Validation(
            "a trial runs between one and eight weeks — beyond that it is a job, and \
             should be one"
                .into(),
        ));
    }
    if !input.hourly_rate.is_positive() {
        return Err(AppError::Validation(
            "a trial is paid work. An unpaid one is an interview with extra steps, and \
             this does not represent it."
                .into(),
        ));
    }

    let ends_at = chrono::Utc::now() + chrono::Duration::weeks(input.duration_weeks as i64);

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO recruitment_trials
            (enterprise_id, talent_user_id, campaign_id, duration_weeks,
             hourly_rate, currency, platform_fee_percent,
             converted_success_fee_percent, ends_at, outcome)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'ongoing')
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.talent_user_id)
    .bind(input.campaign_id)
    .bind(input.duration_weeks)
    .bind(&input.hourly_rate)
    .bind(&input.currency)
    .bind(BigDecimal::try_from(DEFAULT_PLATFORM_FEE).unwrap_or_default())
    .bind(BigDecimal::try_from(CONVERTED_SUCCESS_FEE).unwrap_or_default())
    .bind(ends_at)
    .fetch_one(db)
    .await
    .map_err(overlap_error)?;

    by_id(db, id).await
}

fn overlap_error(e: sqlx::Error) -> AppError {
    if matches!(&e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")) {
        return AppError::Validation(
            "this person is already on a trial with you — a second one at the same time \
             would double the hours and halve the point"
                .into(),
        );
    }
    AppError::from(e)
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Trial, AppError> {
    let sql = format!("{TRIAL_SELECT} WHERE t.id = $1");
    sqlx::query_as::<_, Trial>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("trial not found".into()))
}

pub async fn for_enterprise(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Trial>, AppError> {
    let sql = format!("{TRIAL_SELECT} WHERE t.enterprise_id = $1 ORDER BY t.started_at DESC");
    let rows = sqlx::query_as::<_, Trial>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_talent(db: &PgPool, talent_user_id: Uuid) -> Result<Vec<Trial>, AppError> {
    let sql = format!("{TRIAL_SELECT} WHERE t.talent_user_id = $1 ORDER BY t.started_at DESC");
    let rows = sqlx::query_as::<_, Trial>(sqlx::AssertSqlSafe(sql))
        .bind(talent_user_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HoursEntry {
    pub id: Uuid,
    pub worked_on: chrono::NaiveDate,
    pub hours: BigDecimal,
    pub summary: String,
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rejected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rejection_reason: Option<String>,
}

/// Claim a day's work.
pub async fn log_hours(
    db: &PgPool,
    trial_id: Uuid,
    talent_user_id: Uuid,
    worked_on: chrono::NaiveDate,
    hours: BigDecimal,
    summary: &str,
) -> Result<Uuid, AppError> {
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM recruitment_trials
              WHERE id = $1 AND talent_user_id = $2 AND ended_at IS NULL)",
    )
    .bind(trial_id)
    .bind(talent_user_id)
    .fetch_one(db)
    .await?;
    if !owns {
        return Err(AppError::NotFound(
            "that trial is not yours, or it has ended".into(),
        ));
    }

    if summary.trim().is_empty() {
        return Err(AppError::Validation(
            "say what you did — it is what the client approves against, and what you \
             point at when an entry is questioned"
                .into(),
        ));
    }
    crate::validators::check_max_len(summary, "summary", 2000)?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO recruitment_trial_hours (trial_id, worked_on, hours, summary)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (trial_id, worked_on) DO UPDATE
             SET hours = EXCLUDED.hours,
                 summary = EXCLUDED.summary,
                 -- A corrected entry goes back to unapproved: an approval
                 -- belongs to the figure it approved.
                 approved_at = NULL, approved_by = NULL,
                 rejected_at = NULL, rejection_reason = NULL
         RETURNING id",
    )
    .bind(trial_id)
    .bind(worked_on)
    .bind(&hours)
    .bind(summary.trim())
    .fetch_one(db)
    .await
    .map_err(window_error)?;

    Ok(id)
}

fn window_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    for marker in ["before the trial started", "after the trial ended"] {
        if message.contains(marker) {
            return AppError::Validation(format!("that day is {marker}"));
        }
    }
    AppError::from(e)
}

pub async fn hours_of(db: &PgPool, trial_id: Uuid) -> Result<Vec<HoursEntry>, AppError> {
    let rows = sqlx::query_as::<_, HoursEntry>(
        "SELECT id, worked_on, hours, summary, approved_at, rejected_at, rejection_reason
           FROM recruitment_trial_hours
          WHERE trial_id = $1 ORDER BY worked_on",
    )
    .bind(trial_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// The client approves or refuses a day.
pub async fn decide_hours(
    db: &PgPool,
    entry_id: Uuid,
    enterprise_id: Uuid,
    decider: Uuid,
    approve: bool,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let reason = reason.map(str::trim).filter(|s| !s.is_empty());
    if !approve && reason.is_none() {
        return Err(AppError::Validation(
            "refusing somebody's hours without saying why is refusing their wages \
             without saying why"
                .into(),
        ));
    }

    let done = sqlx::query(
        "UPDATE recruitment_trial_hours h
            SET approved_at = CASE WHEN $4 THEN NOW() END,
                approved_by = CASE WHEN $4 THEN $3 END,
                rejected_at = CASE WHEN $4 THEN NULL ELSE NOW() END,
                rejection_reason = CASE WHEN $4 THEN NULL ELSE $5 END
           FROM recruitment_trials t
          WHERE h.id = $1 AND h.trial_id = t.id AND t.enterprise_id = $2",
    )
    .bind(entry_id)
    .bind(enterprise_id)
    .bind(decider)
    .bind(approve)
    .bind(reason)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("that entry is not on your trial".into()));
    }
    Ok(())
}

/// End a trial, saying which way it went.
///
/// The outcome names which side walked away. "It did not work out" hides the
/// single most useful thing to know when the same client tries again.
pub async fn conclude(
    db: &PgPool,
    trial_id: Uuid,
    outcome: &str,
    note: Option<&str>,
) -> Result<Trial, AppError> {
    if !OUTCOMES.contains(&outcome) || outcome == "ongoing" {
        return Err(AppError::Validation(format!(
            "outcome must be one of: {}",
            OUTCOMES
                .iter()
                .filter(|o| **o != "ongoing")
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    sqlx::query(
        "UPDATE recruitment_trials
            SET outcome = $2, outcome_note = $3, ended_at = COALESCE(ended_at, NOW())
          WHERE id = $1 AND ended_at IS NULL",
    )
    .bind(trial_id)
    .bind(outcome)
    .bind(note.map(str::trim).filter(|s| !s.is_empty()))
    .execute(db)
    .await?;

    by_id(db, trial_id).await
}

/// What is owed for a concluded trial, and the platform's share of it.
///
/// Approved hours only. Claimed-but-unapproved is not money owed, and paying
/// it would remove the client's only say over what they are billed.
pub async fn settle(db: &PgPool, trial_id: Uuid) -> Result<(BigDecimal, BigDecimal), AppError> {
    let trial = by_id(db, trial_id).await?;
    let (talent, platform) = split(
        &trial.approved_hours,
        &trial.hourly_rate,
        &trial.platform_fee_percent,
    );

    if platform.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_talent_id, related_enterprise_id, amount_credits,
                 fee_rate_bps, notes)
             VALUES ('recruitment_success_fee', $1, $2, $3, $4, $5)",
        )
        .bind(trial.talent_user_id)
        .bind(trial.enterprise_id)
        .bind(&platform)
        .bind(crate::services::ledger::percent_to_bps(
            &trial.platform_fee_percent,
        ))
        .bind(format!(
            "période d'essai : {} h approuvées à {} {}",
            trial.approved_hours, trial.hourly_rate, trial.currency
        ))
        .execute(db)
        .await?;
    }

    Ok((talent, platform))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    // The reduction is the incentive that makes anybody use this rather than
    // hiring on a conversation. Compile-time: both sides are constants, and a
    // build that removes the incentive should not produce a binary.
    const _: () = assert!(CONVERTED_SUCCESS_FEE < 10.0);

    #[test]
    fn the_platform_rounds_its_own_share_down() {
        // 15% of 333.33 is 49.9995. Rounding up would keep a centime nobody
        // agreed to, out of somebody's wages.
        let (talent, platform) = split(&dec("1"), &dec("333.33"), &dec("15.00"));
        assert_eq!(platform, dec("49.99"));
        assert_eq!(talent, dec("283.34"));
        assert_eq!(&talent + &platform, dec("333.33"));
    }

    #[test]
    fn the_two_halves_always_add_back_to_the_whole() {
        for (hours, rate, fee) in [
            ("7.5", "80.00", "15.00"),
            ("3.25", "42.10", "15.00"),
            ("1", "0.01", "15.00"),
            ("40", "12345.67", "30.00"),
        ] {
            let (talent, platform) = split(&dec(hours), &dec(rate), &dec(fee));
            let gross =
                (dec(hours) * dec(rate)).with_scale_round(2, bigdecimal::RoundingMode::HalfUp);
            assert_eq!(
                &talent + &platform,
                gross,
                "{hours} h at {rate} lost a centime"
            );
        }
    }

    #[test]
    fn the_maximum_cost_is_visible_before_signing() {
        // Four weeks at 80 an hour, thirty-five hours a week: 11 200. A
        // client agreeing to a trial is agreeing to an exposure and should
        // see it before signing rather than on the first invoice.
        assert_eq!(maximum_cost(4, &dec("80.00")), dec("11200.00"));
        assert_eq!(maximum_cost(0, &dec("80.00")), dec("0.00"));
    }

    #[test]
    fn the_outcome_says_which_side_walked_away() {
        assert!(OUTCOMES.contains(&"declined_by_enterprise"));
        assert!(OUTCOMES.contains(&"declined_by_talent"));
        // No generic "failed": it would hide the one useful fact.
        assert!(!OUTCOMES.contains(&"failed"));
    }
}
