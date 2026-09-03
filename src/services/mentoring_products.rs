//! Paid mentoring: the modes that were declared and never wired, plus the
//! two products built on top of them.
//!
//! ## What the audit found
//!
//! Migration 0107 gave mentors four economic modes and gave volunteer hours
//! and referral commissions their own tables, indexes and comments. Nothing
//! read or wrote any of it: only `paid_session` was ever connected. A mode a
//! mentor can choose and that then does nothing is worse than a mode that
//! does not exist — the mentor who picked `paid_monthly` believes they are
//! earning.
//!
//! This module connects the rest.
//!
//! ## The anti-double-dipping rule
//!
//! A mentor cannot be paid per session *and* take a placement commission on
//! the same relationship. The commission rewards hours given free; charging
//! for those hours and then claiming the reward for having given them is the
//! same money twice. The rule was written in 0107's comments and enforced
//! nowhere. It is enforced here.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

/// What Skilluv keeps on a session or a subscription between two people.
pub const PLATFORM_PERCENT: f64 = 20.0;

/// What Skilluv keeps when a company is the client.
///
/// Higher because Skilluv found the client. The mentor did not have to sell
/// anything, and the difference is what that is worth.
pub const CORPORATE_PERCENT: f64 = 25.0;

/// Hours given free before a mentor is eligible for a placement commission.
///
/// From migration 0107, where it is also the database's own floor.
pub const VOLUNTEER_THRESHOLD_HOURS: f64 = 5.0;

/// The share of a placement fee a qualifying mentor receives, in basis
/// points. Ten per cent, as decided in 0107.
pub const REFERRAL_RATE_BPS: i32 = 1000;

/// The commission rate for a programme.
pub fn program_commission(kind: &str) -> f64 {
    match kind {
        "corporate" => CORPORATE_PERCENT,
        _ => PLATFORM_PERCENT,
    }
}

/// How a payment divides between the mentor and the platform.
///
/// Cents in, cents out, and they always add back: the platform's share is
/// rounded down and the mentor takes the remainder, so a rounding error is
/// never taken out of somebody's fee.
pub fn split_cents(total_cents: i64, platform_percent: f64) -> (i64, i64) {
    if total_cents <= 0 {
        return (0, 0);
    }
    let platform = ((total_cents as f64) * platform_percent / 100.0).floor() as i64;
    (total_cents - platform, platform)
}

/// Why a mentor is or is not owed a placement commission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommissionEligibility {
    Eligible,
    /// Fewer than the threshold hours given free.
    NotEnoughVolunteerHours,
    /// The mentor charged this mentee. The commission rewards hours given
    /// free; charging for them and claiming the reward is the same money
    /// twice.
    AlreadyPaidForThoseHours,
}

/// Whether a mentor may take a placement commission on a mentee.
///
/// Written as a function over plain numbers so the rule can be read and
/// argued with. It was a paragraph in a migration comment for a year, which
/// is why it never ran.
pub fn commission_eligibility(
    volunteer_hours: f64,
    paid_sessions_with_this_mentee: i64,
) -> CommissionEligibility {
    if paid_sessions_with_this_mentee > 0 {
        return CommissionEligibility::AlreadyPaidForThoseHours;
    }
    if volunteer_hours < VOLUNTEER_THRESHOLD_HOURS {
        return CommissionEligibility::NotEnoughVolunteerHours;
    }
    CommissionEligibility::Eligible
}

// ═══════════════════════════════════════════════════════════════════
// Monthly subscriptions
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Subscription {
    pub id: Uuid,
    pub mentor_user_id: Uuid,
    pub mentee_user_id: Uuid,
    pub monthly_fee_cents: i64,
    pub currency: String,
    pub platform_percent: BigDecimal,
    pub sessions_included: i16,
    pub current_period_end: chrono::DateTime<chrono::Utc>,
    pub auto_renew: bool,
}

const SUBSCRIPTION_SELECT: &str = r#"
    SELECT id, mentor_user_id, mentee_user_id, monthly_fee_cents, currency,
           platform_percent, sessions_included, current_period_end, auto_renew
      FROM mentor_subscriptions
"#;

/// Subscribe to a mentor by the month.
///
/// The price is read from the mentor's profile and frozen onto the
/// subscription: a mentor raising their rate must not change what somebody is
/// already paying without them agreeing again.
pub async fn subscribe(
    db: &PgPool,
    mentor_user_id: Uuid,
    mentee_user_id: Uuid,
) -> Result<Subscription, AppError> {
    if mentor_user_id == mentee_user_id {
        return Err(AppError::Validation("nobody mentors themselves".into()));
    }

    let profile: Option<(String, Option<i64>, bool)> = sqlx::query_as(
        "SELECT mode, monthly_subscription_cents, active
           FROM mentor_profiles WHERE user_id = $1",
    )
    .bind(mentor_user_id)
    .fetch_optional(db)
    .await?;
    let (mode, fee, active) = profile.ok_or_else(|| AppError::NotFound("no such mentor".into()))?;

    if !active {
        return Err(AppError::Validation(
            "this mentor is not taking mentees".into(),
        ));
    }
    if !matches!(mode.as_str(), "paid_monthly" | "hybrid") {
        return Err(AppError::Validation(format!(
            "this mentor works {mode} and does not offer a monthly arrangement"
        )));
    }
    let fee = fee.filter(|f| *f > 0).ok_or_else(|| {
        AppError::Validation(
            "this mentor has not set a monthly price yet. A monthly mode with no price \
             is why this mode never worked."
                .into(),
        )
    })?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mentor_subscriptions
            (mentor_user_id, mentee_user_id, monthly_fee_cents, platform_percent,
             current_period_end)
         VALUES ($1, $2, $3, $4, NOW() + INTERVAL '30 days')
         ON CONFLICT (mentor_user_id, mentee_user_id) WHERE cancelled_at IS NULL
         DO UPDATE SET
             -- Renewing early keeps the time already paid for.
             current_period_end = GREATEST(mentor_subscriptions.current_period_end, NOW())
                                  + INTERVAL '30 days',
             auto_renew = TRUE
         RETURNING id",
    )
    .bind(mentor_user_id)
    .bind(mentee_user_id)
    .bind(fee)
    .bind(BigDecimal::try_from(PLATFORM_PERCENT).unwrap_or_default())
    .fetch_one(db)
    .await?;

    // The mentor's share, captured now. The platform's is the remainder, and
    // the two add back to the fee.
    let (mentor_cents, platform_cents) = split_cents(fee, PLATFORM_PERCENT);
    let amount = BigDecimal::from(mentor_cents) / BigDecimal::from(100);

    let currency: ledger::Currency = "EUR".parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!(
            "mentor_subscription:{id}:{}",
            chrono::Utc::now().timestamp()
        ),
        mentor_user_id,
        amount,
        BigDecimal::from(0),
        currency,
        "mentor_subscription",
        id,
    )
    .await?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, amount_credits, fee_rate_bps, notes)
         VALUES ('mentor_session', $1, $2, $3, 'abonnement mensuel de mentorat')",
    )
    .bind(mentor_user_id)
    .bind(BigDecimal::from(platform_cents) / BigDecimal::from(100))
    .bind((PLATFORM_PERCENT * 100.0) as i32)
    .execute(db)
    .await?;

    subscription(db, id).await
}

pub async fn subscription(db: &PgPool, id: Uuid) -> Result<Subscription, AppError> {
    let sql = format!("{SUBSCRIPTION_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Subscription>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("subscription not found".into()))
}

pub async fn my_subscriptions(db: &PgPool, user_id: Uuid) -> Result<Vec<Subscription>, AppError> {
    let sql = format!(
        "{SUBSCRIPTION_SELECT} WHERE (mentee_user_id = $1 OR mentor_user_id = $1)
            AND cancelled_at IS NULL
          ORDER BY current_period_end DESC"
    );
    let rows = sqlx::query_as::<_, Subscription>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Stop renewing. What was paid for runs to its end.
pub async fn cancel_subscription(db: &PgPool, id: Uuid, caller: Uuid) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE mentor_subscriptions SET auto_renew = FALSE
          WHERE id = $1 AND (mentee_user_id = $2 OR mentor_user_id = $2)
            AND cancelled_at IS NULL",
    )
    .bind(id)
    .bind(caller)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("no such subscription of yours".into()));
    }
    Ok(())
}

/// How many of the included sessions have been used this month.
///
/// Without this, "two sessions a month" is a promise nobody can check and
/// nobody can dispute.
pub async fn sessions_used(db: &PgPool, subscription_id: Uuid) -> Result<(i64, i16), AppError> {
    let subscription = subscription(db, subscription_id).await?;
    let used: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mentor_subscription_sessions
          WHERE subscription_id = $1
            AND counts_for_month = date_trunc('month', CURRENT_DATE)::DATE",
    )
    .bind(subscription_id)
    .fetch_one(db)
    .await?;
    Ok((used, subscription.sessions_included))
}

// ═══════════════════════════════════════════════════════════════════
// Volunteer hours and the placement commission
// ═══════════════════════════════════════════════════════════════════

/// Record hours given free.
pub async fn record_volunteer_hours(
    db: &PgPool,
    mentor_user_id: Uuid,
    mentee_user_id: Uuid,
    session_id: Option<Uuid>,
    hours: BigDecimal,
) -> Result<BigDecimal, AppError> {
    let mode: Option<String> =
        sqlx::query_scalar("SELECT mode FROM mentor_profiles WHERE user_id = $1")
            .bind(mentor_user_id)
            .fetch_optional(db)
            .await?;
    let mode = mode.ok_or_else(|| AppError::NotFound("no such mentor".into()))?;

    // Only free modes accumulate. Recording volunteer hours while charging
    // for them is the double-dip the rule exists to stop, and it has to be
    // refused at the point of recording rather than at the point of paying —
    // by then the hours are in the table and look genuine.
    if !matches!(mode.as_str(), "volunteer" | "hybrid") {
        return Err(AppError::Validation(format!(
            "you work {mode}. Volunteer hours are hours given free, and the placement \
             commission rewards exactly that — recording paid hours here would claim \
             the reward for something already charged for."
        )));
    }

    sqlx::query(
        "INSERT INTO mentor_volunteer_hours
            (mentor_user_id, mentee_user_id, session_id, hours_spent)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(mentor_user_id)
    .bind(mentee_user_id)
    .bind(session_id)
    .bind(&hours)
    .execute(db)
    .await?;

    volunteer_total(db, mentor_user_id, mentee_user_id).await
}

pub async fn volunteer_total(
    db: &PgPool,
    mentor_user_id: Uuid,
    mentee_user_id: Uuid,
) -> Result<BigDecimal, AppError> {
    let total: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT COALESCE(sum(hours_spent), 0) FROM mentor_volunteer_hours
          WHERE mentor_user_id = $1 AND mentee_user_id = $2",
    )
    .bind(mentor_user_id)
    .bind(mentee_user_id)
    .fetch_one(db)
    .await?;
    Ok(total.unwrap_or_else(|| BigDecimal::from(0)))
}

/// A mentee was hired. Pay the mentor who got them there, if the rule allows.
///
/// Returns `None` when the rule does not allow it, with the reason in the
/// error only for the cases somebody can act on: a silent no would leave a
/// mentor wondering for months.
pub async fn award_placement_commission(
    db: &PgPool,
    mentor_user_id: Uuid,
    mentee_user_id: Uuid,
    enterprise_id: Uuid,
    placement_amount_cents: i64,
) -> Result<i64, AppError> {
    let hours = volunteer_total(db, mentor_user_id, mentee_user_id).await?;
    let hours_f = hours.to_f64().unwrap_or(0.0);

    let paid_sessions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mentorship_sessions
          WHERE mentor_user_id = $1 AND mentee_user_id = $2
            AND status IN ('paid', 'confirmed', 'completed')
            AND price_total_cents > 0",
    )
    .bind(mentor_user_id)
    .bind(mentee_user_id)
    .fetch_one(db)
    .await?;

    match commission_eligibility(hours_f, paid_sessions) {
        CommissionEligibility::Eligible => {}
        CommissionEligibility::NotEnoughVolunteerHours => {
            return Err(AppError::Validation(format!(
                "{hours_f} volunteer hours with this mentee, and the threshold is \
                 {VOLUNTEER_THRESHOLD_HOURS}."
            )));
        }
        CommissionEligibility::AlreadyPaidForThoseHours => {
            return Err(AppError::Validation(
                "this mentee has paid for sessions with this mentor. The placement \
                 commission rewards hours given free; taking both would be the same \
                 money twice."
                    .into(),
            ));
        }
    }

    let mentor_share = placement_amount_cents * REFERRAL_RATE_BPS as i64 / 10_000;
    if mentor_share <= 0 {
        return Err(AppError::Validation(
            "the placement is too small to produce a commission".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO mentor_referral_commissions
            (mentor_user_id, mentee_user_id, enterprise_id, placement_amount_cents,
             mentor_share_cents, commission_rate_bps, hours_mentored_volunteer)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(mentor_user_id)
    .bind(mentee_user_id)
    .bind(enterprise_id)
    .bind(placement_amount_cents)
    .bind(mentor_share)
    .bind(REFERRAL_RATE_BPS)
    .bind(&hours)
    .execute(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("uniq_mentor_referral_commissions_triple")
        {
            AppError::Validation(
                "a commission already exists for this mentor, mentee and company. The \
                 same person hired twice by the same company through the same mentor is \
                 worth a look before it is worth a payment."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    let currency: ledger::Currency = "EUR".parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("mentor_referral:{mentor_user_id}:{mentee_user_id}:{enterprise_id}"),
        mentor_user_id,
        BigDecimal::from(mentor_share) / BigDecimal::from(100),
        BigDecimal::from(0),
        currency,
        "mentor_referral_commission",
        enterprise_id,
    )
    .await?;

    Ok(mentor_share)
}

// ═══════════════════════════════════════════════════════════════════
// Programmes
// ═══════════════════════════════════════════════════════════════════

pub const PROGRAM_KINDS: &[&str] = &["premium_cohort", "corporate"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Program {
    pub id: Uuid,
    pub mentor_user_id: Uuid,
    pub kind: String,
    pub payer: String,
    pub enterprise_id: Option<Uuid>,
    pub title: String,
    pub brief_md: String,
    pub skill_domain: String,
    pub duration_months: i16,
    pub sessions_per_month: i16,
    pub price_per_mentee: Option<BigDecimal>,
    pub monthly_fee: Option<BigDecimal>,
    pub currency: String,
    pub commission_percent: BigDecimal,
    pub max_mentees: i16,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const PROGRAM_SELECT: &str = r#"
    SELECT id, mentor_user_id, kind, payer, enterprise_id, title, brief_md,
           skill_domain, duration_months, sessions_per_month, price_per_mentee,
           monthly_fee, currency, commission_percent, max_mentees, status, created_at
      FROM mentoring_programs
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ProgramInput {
    pub kind: String,
    #[serde(default)]
    pub enterprise_id: Option<Uuid>,
    pub title: String,
    pub brief_md: String,
    pub skill_domain: String,
    pub duration_months: i16,
    #[serde(default)]
    pub sessions_per_month: Option<i16>,
    #[serde(default)]
    pub price_per_mentee: Option<BigDecimal>,
    #[serde(default)]
    pub monthly_fee: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
    pub max_mentees: i16,
    #[serde(default)]
    pub starts_on: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub ends_on: Option<chrono::NaiveDate>,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn open_program(
    db: &PgPool,
    mentor_user_id: Uuid,
    input: ProgramInput,
) -> Result<Program, AppError> {
    if !PROGRAM_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            PROGRAM_KINDS.join(", ")
        )));
    }
    if input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the programme covers. People are committing months to it.".into(),
        ));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;

    let payer = if input.kind == "corporate" {
        "enterprise"
    } else {
        "mentee"
    };
    let commission = program_commission(&input.kind);

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mentoring_programs
            (mentor_user_id, kind, payer, enterprise_id, title, brief_md, skill_domain,
             duration_months, sessions_per_month, price_per_mentee, monthly_fee,
             currency, commission_percent, max_mentees, starts_on, ends_on)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,COALESCE($9,2),$10,$11,$12,$13,$14,$15,$16)
         RETURNING id",
    )
    .bind(mentor_user_id)
    .bind(&input.kind)
    .bind(payer)
    .bind(input.enterprise_id)
    .bind(input.title.trim())
    .bind(input.brief_md.trim())
    .bind(&input.skill_domain)
    .bind(input.duration_months)
    .bind(input.sessions_per_month)
    .bind(input.price_per_mentee.as_ref())
    .bind(input.monthly_fee.as_ref())
    .bind(&input.currency)
    .bind(BigDecimal::try_from(commission).unwrap_or_default())
    .bind(input.max_mentees)
    .bind(input.starts_on)
    .bind(input.ends_on)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("a_cohort_is_priced_per_head") {
            AppError::Validation(
                "a premium cohort is priced per mentee, and each of them pays. Set \
                 price_per_mentee and nothing else."
                    .into(),
            )
        } else if m.contains("corporate_mentoring_is_priced_monthly") {
            AppError::Validation(
                "corporate mentoring is billed to a company by the month. Name the \
                 company and set monthly_fee."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    program(db, id).await
}

pub async fn program(db: &PgPool, id: Uuid) -> Result<Program, AppError> {
    let sql = format!("{PROGRAM_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Program>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("programme not found".into()))
}

/// What a mentee can join. Corporate runs are absent: their places are
/// allocated by the client, not browsed.
pub async fn open_programs(db: &PgPool) -> Result<Vec<Program>, AppError> {
    let sql = format!(
        "{PROGRAM_SELECT} WHERE status = 'recruiting' AND kind = 'premium_cohort'
          ORDER BY created_at DESC LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Program>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Enrol, and pay the mentor their share.
pub async fn enrol(
    db: &PgPool,
    program_id: Uuid,
    mentee_user_id: Option<Uuid>,
    mentee_email: Option<&str>,
    mentee_name: Option<&str>,
) -> Result<Uuid, AppError> {
    let program = program(db, program_id).await?;

    if mentee_user_id.is_some() == mentee_email.is_some() {
        return Err(AppError::Validation(
            "name the mentee once: an account or an email".into(),
        ));
    }
    if program.kind == "premium_cohort" && mentee_user_id.is_none() {
        return Err(AppError::Validation(
            "a premium cohort enrols Skilluv accounts — the mentee pays for it \
             themselves and needs somewhere to be paid from and reviewed"
                .into(),
        ));
    }
    if mentee_user_id == Some(program.mentor_user_id) {
        return Err(AppError::Validation("nobody mentors themselves".into()));
    }

    let paid = program
        .price_per_mentee
        .clone()
        .or_else(|| program.monthly_fee.clone());

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mentoring_program_members
            (program_id, mentee_user_id, mentee_email, mentee_name, amount_paid)
         VALUES ($1,$2,$3,$4,$5)
         RETURNING id",
    )
    .bind(program_id)
    .bind(mentee_user_id)
    .bind(mentee_email.map(str::trim))
    .bind(mentee_name.map(str::trim))
    .bind(paid.as_ref())
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("already has its") || m.contains("is not enrolling") {
            AppError::Validation(
                m.rsplit("ERROR:")
                    .next()
                    .unwrap_or("this programme is full")
                    .trim()
                    .to_string(),
            )
        } else if m.contains("idx_one_enrolment") {
            AppError::Validation("already enrolled".into())
        } else {
            AppError::from(e)
        }
    })?;

    if let Some(amount) = paid {
        let cents = (&amount * BigDecimal::from(100)).to_i64().unwrap_or(0);
        let percent = program
            .commission_percent
            .to_f64()
            .unwrap_or(PLATFORM_PERCENT);
        let (mentor_cents, platform_cents) = split_cents(cents, percent);

        let currency: ledger::Currency = program.currency.parse()?;
        ledger::capture_for_recipient(
            db,
            "stripe",
            format!("mentoring_program:{id}"),
            program.mentor_user_id,
            BigDecimal::from(mentor_cents) / BigDecimal::from(100),
            BigDecimal::from(0),
            currency,
            "mentoring_program",
            program_id,
        )
        .await?;

        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_talent_id, related_enterprise_id, amount_credits,
                 fee_rate_bps, notes)
             VALUES ('mentoring_program', $1, $2, $3, $4, $5)",
        )
        .bind(program.mentor_user_id)
        .bind(program.enterprise_id)
        .bind(BigDecimal::from(platform_cents) / BigDecimal::from(100))
        .bind(ledger::percent_to_bps(&program.commission_percent))
        .bind(format!("{} — {}", program.kind, program.title))
        .execute(db)
        .await?;
    }

    Ok(id)
}

// ═══════════════════════════════════════════════════════════════════
// One-off slots
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OpenSlot {
    pub id: Uuid,
    pub mentor_user_id: Uuid,
    pub specific_date: chrono::NaiveDate,
    pub start_time: chrono::NaiveTime,
    pub end_time: chrono::NaiveTime,
    pub timezone: String,
}

/// Open a single slot, without committing to it every week for ever.
pub async fn open_slot(
    db: &PgPool,
    mentor_user_id: Uuid,
    date: chrono::NaiveDate,
    start: chrono::NaiveTime,
    end: chrono::NaiveTime,
    timezone: &str,
) -> Result<Uuid, AppError> {
    if end <= start {
        return Err(AppError::Validation(
            "a slot has to end after it starts".into(),
        ));
    }
    if date < chrono::Utc::now().date_naive() {
        return Err(AppError::Validation("that day has already been".into()));
    }

    use chrono::Datelike;
    let weekday = date.weekday().num_days_from_sunday() as i32;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mentor_availability
            (mentor_user_id, weekday, start_time, end_time, timezone, specific_date)
         VALUES ($1,$2,$3,$4,$5,$6)
         RETURNING id",
    )
    .bind(mentor_user_id)
    .bind(weekday)
    .bind(start)
    .bind(end)
    .bind(timezone)
    .bind(date)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// What is free to book, one-off slots only.
pub async fn open_slots(db: &PgPool, mentor_user_id: Uuid) -> Result<Vec<OpenSlot>, AppError> {
    let rows = sqlx::query_as::<_, OpenSlot>(
        "SELECT id, mentor_user_id, specific_date, start_time, end_time, timezone
           FROM mentor_availability
          WHERE mentor_user_id = $1 AND specific_date IS NOT NULL
            AND specific_date >= CURRENT_DATE
            AND consumed_by_session_id IS NULL
          ORDER BY specific_date, start_time",
    )
    .bind(mentor_user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Take a one-off slot for a session.
///
/// A recurring slot is never consumed; a one-off is, and offering it twice is
/// how two people arrive at the same call.
pub async fn consume_slot(db: &PgPool, slot_id: Uuid, session_id: Uuid) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE mentor_availability SET consumed_by_session_id = $2
          WHERE id = $1 AND specific_date IS NOT NULL AND consumed_by_session_id IS NULL",
    )
    .bind(slot_id)
    .bind(session_id)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::Validation("that slot has just been taken".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_always_adds_back_to_the_total() {
        for total in [1, 99, 100, 3333, 100_000, 999_999] {
            let (mentor, platform) = split_cents(total, PLATFORM_PERCENT);
            assert_eq!(mentor + platform, total, "{total} cents went missing");
            assert!(mentor >= 0 && platform >= 0);
        }
    }

    #[test]
    fn the_rounding_goes_to_the_mentor() {
        // 3333 at 20% is 666.6; the platform takes 666 and the mentor 2667.
        let (mentor, platform) = split_cents(3333, PLATFORM_PERCENT);
        assert_eq!(platform, 666);
        assert_eq!(mentor, 2667);
    }

    #[test]
    fn a_company_client_costs_more_than_a_person() {
        // Skilluv found the client; the mentor did not have to sell anything.
        assert!(program_commission("corporate") > program_commission("premium_cohort"));
        assert_eq!(program_commission("premium_cohort"), PLATFORM_PERCENT);
    }

    #[test]
    fn nothing_is_split_out_of_nothing() {
        assert_eq!(split_cents(0, PLATFORM_PERCENT), (0, 0));
        assert_eq!(split_cents(-100, PLATFORM_PERCENT), (0, 0));
    }

    #[test]
    fn a_mentor_who_charged_cannot_also_claim_the_placement_reward() {
        // The commission rewards hours given free. Charging for them and
        // claiming the reward is the same money twice.
        assert_eq!(
            commission_eligibility(40.0, 1),
            CommissionEligibility::AlreadyPaidForThoseHours
        );
    }

    #[test]
    fn the_volunteer_threshold_holds() {
        assert_eq!(
            commission_eligibility(4.9, 0),
            CommissionEligibility::NotEnoughVolunteerHours
        );
        assert_eq!(
            commission_eligibility(5.0, 0),
            CommissionEligibility::Eligible
        );
        assert_eq!(
            commission_eligibility(120.0, 0),
            CommissionEligibility::Eligible
        );
    }

    #[test]
    fn no_hours_and_no_payment_is_still_no_commission() {
        assert_eq!(
            commission_eligibility(0.0, 0),
            CommissionEligibility::NotEnoughVolunteerHours
        );
    }
}
