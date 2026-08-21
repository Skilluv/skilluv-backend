//! Recruitment Skilluv runs, rather than a search the client runs.
//!
//! ## The two numbers worth testing
//!
//! The volume discount and the guarantee refund are both scales — a series of
//! thresholds with different answers on each side. Both are pure, both are
//! tested here, and both are the kind of arithmetic that is wrong silently:
//! a discount off by a band overcharges a client who will not check, and a
//! refund off by a band underpays one who will.
//!
//! ## The rule that is not arithmetic
//!
//! Nobody is presented to a client without having agreed. That is enforced by
//! a trigger, because the shortlist is written from an admin endpoint, a
//! curation job and eventually an import — and a service check would hold for
//! one of the three.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const KINDS: &[&str] = &["managed", "volume", "private_pool"];

pub const STATUSES: &[&str] = &[
    "briefing",
    "sourcing",
    "shortlist_delivered",
    "interviews",
    "hired",
    "closed",
    "cancelled",
];

/// How long a hire is guaranteed, unless the contract says otherwise.
pub const DEFAULT_GUARANTEE_DAYS: i64 = 182;

/// The reduction earned by hiring several people at once.
///
/// A scale rather than a formula: the bands are a commercial decision, and
/// somebody should be able to read them without evaluating an expression.
pub fn volume_discount_percent(positions: i16) -> f64 {
    match positions {
        p if p >= 20 => 30.0,
        p if p >= 10 => 20.0,
        p if p >= 5 => 10.0,
        _ => 0.0,
    }
}

/// What is owed back when somebody leaves inside the guarantee.
///
/// Proportional to how much of the window is left, in three bands. The bands
/// exist rather than a straight proration because the work Skilluv did is the
/// same whether the person stayed one month or five, and a straight line
/// would refund almost everything for a departure at week two — which is
/// exactly when the client is angriest and least interested in the argument.
///
/// Returns a fraction of the fee, from 0 to 1.
pub fn refund_fraction(days_stayed: i64, guarantee_days: i64) -> f64 {
    if guarantee_days <= 0 || days_stayed >= guarantee_days {
        return 0.0;
    }
    if days_stayed < 0 {
        return 1.0;
    }

    let elapsed = days_stayed as f64 / guarantee_days as f64;
    match elapsed {
        // Left almost immediately: the placement did not happen in any
        // meaningful sense.
        e if e < 0.25 => 1.0,
        e if e < 0.5 => 0.5,
        _ => 0.25,
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Campaign {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub kind: String,
    pub title: String,
    pub brief_md: String,
    pub target_role: String,
    pub target_domain: String,
    pub target_orientations: Vec<String>,
    pub target_countries: Vec<String>,
    pub seniority_range: Vec<String>,
    pub salary_range: Option<serde_json::Value>,
    pub remote_ok: bool,
    pub positions_count: i16,
    pub setup_fee: Option<BigDecimal>,
    pub success_fee_percent: Option<BigDecimal>,
    pub volume_discount_percent: BigDecimal,
    pub monthly_fee: Option<BigDecimal>,
    pub refresh_cadence_days: Option<i16>,
    pub last_refreshed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub currency: String,
    pub status: String,
    pub assigned_to: Option<Uuid>,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub shortlist_delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const CAMPAIGN_SELECT: &str = r#"
    SELECT id, enterprise_id, kind, title, brief_md, target_role, target_domain,
           target_orientations, target_countries, seniority_range, salary_range,
           remote_ok, positions_count, setup_fee, success_fee_percent,
           volume_discount_percent, monthly_fee, refresh_cadence_days,
           last_refreshed_at, currency, status, assigned_to, deadline,
           shortlist_delivered_at, created_at
      FROM recruitment_campaigns
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct BriefInput {
    #[serde(default = "default_kind")]
    pub kind: String,
    pub title: String,
    pub brief_md: String,
    pub target_role: String,
    pub target_domain: String,
    #[serde(default)]
    pub target_orientations: Vec<String>,
    #[serde(default)]
    pub target_countries: Vec<String>,
    #[serde(default)]
    pub seniority_range: Vec<String>,
    #[serde(default)]
    pub salary_range: Option<serde_json::Value>,
    #[serde(default = "yes")]
    pub remote_ok: bool,
    #[serde(default = "one")]
    pub positions_count: i16,
    #[serde(default)]
    pub setup_fee: Option<BigDecimal>,
    /// The rate before the volume discount. The discount is applied here, not
    /// by the person filling in the form.
    #[serde(default)]
    pub success_fee_percent: Option<BigDecimal>,
    #[serde(default)]
    pub monthly_fee: Option<BigDecimal>,
    #[serde(default)]
    pub refresh_cadence_days: Option<i16>,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_kind() -> String {
    "managed".into()
}
fn yes() -> bool {
    true
}
fn one() -> i16 {
    1
}
fn eur() -> String {
    "EUR".into()
}

/// Take a brief.
pub async fn open_campaign(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: BriefInput,
) -> Result<Campaign, AppError> {
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("title is required".into()));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    if input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a brief with nothing in it cannot be sourced against".into(),
        ));
    }
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;
    if input.target_role.trim().is_empty() {
        return Err(AppError::Validation("target_role is required".into()));
    }
    if !(1..=200).contains(&input.positions_count) {
        return Err(AppError::Validation(
            "positions_count must be between 1 and 200".into(),
        ));
    }

    // Every named trade must exist. A brief targeting a typo sources against
    // nothing and nobody finds out until the shortlist is empty.
    for slug in &input.target_orientations {
        let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
            .bind(slug)
            .fetch_one(db)
            .await?;
        if resolved.is_none() {
            return Err(AppError::Validation(format!(
                "'{slug}' is not a trade Skilluv knows — a brief targeting a typo \
                 sources against nothing"
            )));
        }
    }

    let discount = volume_discount_percent(input.positions_count);
    // Applied here rather than typed: a discount somebody enters by hand is a
    // discount that eventually disagrees with the scale it came from.
    let success_fee = match (&input.success_fee_percent, input.kind.as_str()) {
        (_, "private_pool") => None,
        (Some(rate), _) => {
            let discounted = rate
                * BigDecimal::try_from(1.0 - discount / 100.0)
                    .map_err(|_| AppError::Internal("discount is not a number".into()))?;
            Some(discounted.with_scale_round(2, bigdecimal::RoundingMode::HalfUp))
        }
        (None, _) => {
            return Err(AppError::Validation(
                "a campaign paid on success needs a success_fee_percent".into(),
            ));
        }
    };

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO recruitment_campaigns
            (enterprise_id, kind, title, brief_md, target_role, target_domain,
             target_orientations, target_countries, seniority_range, salary_range,
             remote_ok, positions_count, setup_fee, success_fee_percent,
             volume_discount_percent, monthly_fee, refresh_cadence_days,
             currency, deadline, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(&input.kind)
    .bind(input.title.trim())
    .bind(input.brief_md.trim())
    .bind(input.target_role.trim())
    .bind(&input.target_domain)
    .bind(&input.target_orientations)
    .bind(&input.target_countries)
    .bind(&input.seniority_range)
    .bind(input.salary_range.as_ref())
    .bind(input.remote_ok)
    .bind(input.positions_count)
    .bind(input.setup_fee.as_ref())
    .bind(success_fee.as_ref())
    .bind(BigDecimal::try_from(discount).unwrap_or_default())
    .bind(input.monthly_fee.as_ref())
    .bind(input.refresh_cadence_days)
    .bind(&input.currency)
    .bind(input.deadline)
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(pricing_error)?;

    by_id(db, id).await
}

/// The CHECK constraints on the campaign speak in constraint names; this says
/// the same in words the person filling in the form can act on.
fn pricing_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    if message.contains("a_pool_is_paid_monthly") {
        return AppError::Validation(
            "a retained pool is paid monthly and refreshed on a cadence, and charges no \
             success fee — the client is already paying to keep it warm"
                .into(),
        );
    }
    if message.contains("volume_means_several_positions") {
        return AppError::Validation(
            "a volume campaign is five positions or more — one position at a reduced rate \
             is a discount, and should be recorded as the percentage it is"
                .into(),
        );
    }
    AppError::from(e)
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Campaign, AppError> {
    let sql = format!("{CAMPAIGN_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Campaign>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("campaign not found".into()))
}

pub async fn for_enterprise(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Campaign>, AppError> {
    let sql = format!("{CAMPAIGN_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Campaign>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// The shortlist
// ═══════════════════════════════════════════════════════════════════

/// The guarantee, as the refund calculation needs to see it.
#[derive(sqlx::FromRow)]
struct GuaranteeState {
    hired_at: chrono::DateTime<chrono::Utc>,
    guarantee_ends_at: chrono::DateTime<chrono::Utc>,
    success_fee_amount: BigDecimal,
    refunded_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ShortlistEntry {
    pub talent_user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub match_reason_md: String,
    pub status: String,
    /// Their score in the campaign's domain, so a client can weigh the
    /// argument against something measured.
    pub craft_score: Option<i32>,
    pub craft_tier: Option<String>,
    pub talent_responded_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Put somebody forward, with the argument for them.
pub async fn add_to_shortlist(
    db: &PgPool,
    campaign_id: Uuid,
    talent_user_id: Uuid,
    match_reason_md: &str,
) -> Result<(), AppError> {
    if match_reason_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say why — a shortlist of names with no argument is a search result, and the \
             client is paying not to do that reading themselves"
                .into(),
        ));
    }
    crate::validators::check_max_len(match_reason_md, "match_reason_md", 8000)?;

    sqlx::query(
        "INSERT INTO recruitment_shortlist
            (campaign_id, talent_user_id, match_reason_md)
         VALUES ($1, $2, $3)
         ON CONFLICT (campaign_id, talent_user_id) DO UPDATE
             SET match_reason_md = EXCLUDED.match_reason_md",
    )
    .bind(campaign_id)
    .bind(talent_user_id)
    .bind(match_reason_md.trim())
    .execute(db)
    .await?;

    Ok(())
}

pub async fn shortlist_of(db: &PgPool, campaign_id: Uuid) -> Result<Vec<ShortlistEntry>, AppError> {
    let rows = sqlx::query_as::<_, ShortlistEntry>(
        r#"
        SELECT s.talent_user_id, u.username, u.display_name, s.match_reason_md,
               s.status, cs.score AS craft_score, cs.tier_slug AS craft_tier,
               s.talent_responded_at, s.created_at
          FROM recruitment_shortlist s
          JOIN users u ON u.id = s.talent_user_id
          JOIN recruitment_campaigns c ON c.id = s.campaign_id
          LEFT JOIN craft_scores cs
                 ON cs.user_id = s.talent_user_id
                AND cs.skill_domain = c.target_domain
         WHERE s.campaign_id = $1
         ORDER BY cs.score DESC NULLS LAST, s.created_at ASC
        "#,
    )
    .bind(campaign_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// The talent answers.
///
/// Their own decision, taken through their own session — an admin cannot
/// answer on somebody's behalf, which is the point of the column.
pub async fn talent_responds(
    db: &PgPool,
    campaign_id: Uuid,
    talent_user_id: Uuid,
    interested: bool,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE recruitment_shortlist
            SET status = CASE WHEN $3 THEN 'interested' ELSE 'declined' END,
                talent_responded_at = NOW()
          WHERE campaign_id = $1 AND talent_user_id = $2
            AND status IN ('proposed', 'interested', 'declined')",
    )
    .bind(campaign_id)
    .bind(talent_user_id)
    .bind(interested)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you are not on this shortlist, or it has moved past this point".into(),
        ));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// The fee, and the guarantee
// ═══════════════════════════════════════════════════════════════════

/// Record a confirmed hire and what it costs.
///
/// The guarantee window starts now. `guarantee_days` is a parameter rather
/// than a constant because it is negotiable, and a contract signed at nine
/// months should not be silently shortened by a number in the code.
#[allow(clippy::too_many_arguments)]
pub async fn record_hire(
    db: &PgPool,
    campaign_id: Uuid,
    talent_user_id: Uuid,
    annual_salary: BigDecimal,
    currency: &str,
    guarantee_days: i64,
) -> Result<Uuid, AppError> {
    let campaign = by_id(db, campaign_id).await?;
    let Some(rate) = campaign.success_fee_percent.clone() else {
        return Err(AppError::Validation(
            "this campaign charges no success fee".into(),
        ));
    };
    if !annual_salary.is_positive() {
        return Err(AppError::Validation(
            "an annual salary of nothing is not a salary".into(),
        ));
    }

    // Rounded down, like every other share the platform takes: rounding up
    // means charging for a centime nobody agreed to.
    let fee = (&annual_salary * &rate / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);

    let ends_at = chrono::Utc::now() + chrono::Duration::days(guarantee_days.max(1));

    let mut tx = db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO recruitment_success_fees
            (campaign_id, enterprise_id, talent_user_id, hired_at,
             annual_salary_declared, currency, success_fee_percent,
             success_fee_amount, guarantee_ends_at)
         VALUES ($1,$2,$3,NOW(),$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(campaign_id)
    .bind(campaign.enterprise_id)
    .bind(talent_user_id)
    .bind(&annual_salary)
    .bind(currency)
    .bind(&rate)
    .bind(&fee)
    .bind(ends_at)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE recruitment_campaigns
            SET status = 'hired', hired_at = NOW() WHERE id = $1",
    )
    .bind(campaign_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE recruitment_shortlist SET status = 'hired', decided_at = NOW()
          WHERE campaign_id = $1 AND talent_user_id = $2",
    )
    .bind(campaign_id)
    .bind(talent_user_id)
    .execute(&mut *tx)
    .await?;

    // Booked when it is charged, not when it is collected: the ledger's job
    // is to say what was earned.
    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, related_enterprise_id, amount_credits,
             fee_rate_bps, notes)
         VALUES ('recruitment_success_fee', $1, $2, $3, $4, $5)",
    )
    .bind(talent_user_id)
    .bind(campaign.enterprise_id)
    .bind(&fee)
    .bind(crate::services::ledger::percent_to_bps(&rate))
    .bind(format!("{rate}% sur {annual_salary} {currency}"))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(id)
}

/// Somebody left inside the guarantee. Work out what is owed back.
pub async fn record_departure(
    db: &PgPool,
    fee_id: Uuid,
    left_at: chrono::DateTime<chrono::Utc>,
    reason: &str,
) -> Result<BigDecimal, AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "a refund needs a reason the client can read".into(),
        ));
    }

    let row: Option<GuaranteeState> = sqlx::query_as(
        "SELECT hired_at, guarantee_ends_at, success_fee_amount, refunded_at
           FROM recruitment_success_fees WHERE id = $1",
    )
    .bind(fee_id)
    .fetch_optional(db)
    .await?;
    let GuaranteeState {
        hired_at,
        guarantee_ends_at,
        success_fee_amount: fee,
        refunded_at: already_refunded,
    } = row.ok_or_else(|| AppError::NotFound("fee not found".into()))?;

    if already_refunded.is_some() {
        return Err(AppError::Validation(
            "this fee has already been refunded".into(),
        ));
    }

    let stayed = (left_at - hired_at).num_days();
    let window = (guarantee_ends_at - hired_at).num_days();
    let fraction = refund_fraction(stayed, window);

    let refund = (&fee
        * BigDecimal::try_from(fraction)
            .map_err(|_| AppError::Internal("refund fraction is not a number".into()))?)
    .with_scale_round(2, bigdecimal::RoundingMode::HalfUp);

    sqlx::query(
        "UPDATE recruitment_success_fees
            SET left_at = $2, refund_amount = $3, refund_reason = $4,
                refunded_at = CASE WHEN $3 > 0 THEN NOW() ELSE NULL END
          WHERE id = $1",
    )
    .bind(fee_id)
    .bind(left_at)
    .bind(&refund)
    .bind(reason.trim())
    .execute(db)
    .await?;

    Ok(refund)
}

/// Fees whose guarantee has run out with nobody having left.
///
/// Read by a monthly sweep. Not to do anything automatic — a guarantee
/// expiring is the absence of an event, and the only thing to record is that
/// the window closed.
pub async fn guarantees_expiring(
    db: &PgPool,
    within_days: i32,
) -> Result<Vec<(Uuid, chrono::DateTime<chrono::Utc>)>, AppError> {
    let rows = sqlx::query_as(
        "SELECT id, guarantee_ends_at FROM recruitment_success_fees
          WHERE refunded_at IS NULL AND left_at IS NULL
            AND guarantee_ends_at < NOW() + ($1 || ' days')::INTERVAL
          ORDER BY guarantee_ends_at",
    )
    .bind(within_days.max(0).to_string())
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_scale_has_the_bands_the_pricing_says() {
        assert_eq!(volume_discount_percent(1), 0.0);
        assert_eq!(volume_discount_percent(4), 0.0);
        assert_eq!(volume_discount_percent(5), 10.0);
        assert_eq!(volume_discount_percent(9), 10.0);
        assert_eq!(volume_discount_percent(10), 20.0);
        assert_eq!(volume_discount_percent(19), 20.0);
        assert_eq!(volume_discount_percent(20), 30.0);
        assert_eq!(volume_discount_percent(200), 30.0);
    }

    #[test]
    fn a_departure_after_the_guarantee_owes_nothing() {
        assert_eq!(refund_fraction(200, 182), 0.0);
        assert_eq!(refund_fraction(182, 182), 0.0);
    }

    #[test]
    fn leaving_almost_immediately_refunds_everything() {
        // Two weeks of a six-month guarantee: the placement did not happen in
        // any meaningful sense.
        assert_eq!(refund_fraction(14, 182), 1.0);
        assert_eq!(refund_fraction(45, 182), 1.0);
    }

    #[test]
    fn the_refund_falls_in_bands_rather_than_a_straight_line() {
        // Halfway through, half back. Not "half the remaining window", which
        // would refund almost everything at week two — exactly when the
        // client is angriest and least interested in the argument.
        assert_eq!(refund_fraction(91, 182), 0.25);
        assert_eq!(refund_fraction(60, 182), 0.5);
        assert_eq!(refund_fraction(170, 182), 0.25);
    }

    #[test]
    fn a_nonsensical_window_refunds_nothing_rather_than_dividing_by_zero() {
        assert_eq!(refund_fraction(10, 0), 0.0);
        assert_eq!(refund_fraction(10, -5), 0.0);
    }

    #[test]
    fn a_departure_before_the_hire_is_a_full_refund() {
        // Data entry gone wrong, or a hire that was undone. Either way the
        // client owes nothing, and answering with a negative fraction would
        // credit the platform.
        assert_eq!(refund_fraction(-3, 182), 1.0);
    }

    #[test]
    fn the_statuses_end_somewhere() {
        // Every campaign reaches closed or cancelled. A status machine with
        // no terminal state is one that leaves rows open forever.
        assert!(STATUSES.contains(&"closed"));
        assert!(STATUSES.contains(&"cancelled"));
    }
}
