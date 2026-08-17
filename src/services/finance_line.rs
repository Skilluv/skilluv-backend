//! The finance line: advances, introductions, guarantees, funded cohorts.
//!
//! Everything here moves money that is not a payment for work, which is the
//! part regulators care about. Three rules run through it.
//!
//! **An advance is not a loan.** It points at one issued invoice, cannot
//! exceed it, and repays from it. That is what keeps it outside credit
//! regulation, so it is enforced rather than assumed: no invoice, no advance.
//!
//! **An introduction needs a permission.** Referring somebody to a lender or
//! an insurer is a regulated act. A partnership cannot be active without a
//! stated regulatory basis and a signed contract, and nothing can be referred
//! through an inactive one.
//!
//! **A trainee never owes anything.** The funded-cohort product is an income
//! share agreement with the direction reversed: the company pays, the trainee
//! does not, and declining the job at the end is free. There is no column for
//! a trainee obligation and there is not going to be one.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

pub const PARTNERSHIP_KINDS: &[&str] = &[
    "loan",
    "insurance_professional",
    "insurance_income",
    "insurance_cyber",
    "insurance_health",
];

/// The default cut on an advance, and the band it may be negotiated in.
///
/// Four per cent on money that arrives weeks early is comparable to what the
/// mobile money operators charge for the same service, and the ceiling exists
/// so that a bad month cannot turn the product into something the people
/// using it would be better off without.
pub const DEFAULT_ADVANCE_FEE: f64 = 4.0;
pub const MAX_ADVANCE_FEE: f64 = 8.0;

/// What an advance pays out, and what it costs.
///
/// The fee comes off the advance rather than being added to the repayment:
/// the contributor sees the net amount before agreeing, which is the number
/// they actually care about.
pub fn advance_figures(
    expected_payment: &BigDecimal,
    advance_percent: &BigDecimal,
    fee_percent: &BigDecimal,
) -> (BigDecimal, BigDecimal, BigDecimal) {
    let gross = (expected_payment * advance_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);
    let fee = (&gross * fee_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::HalfUp);
    let net = &gross - &fee;
    (gross, fee, net)
}

/// Why an advance was refused, when it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
    Eligible,
    /// The invoice has not been issued, or has already been paid.
    InvoiceNotAdvanceable,
    /// The rank floor. The advance is priced on Skilluv's assessment, and an
    /// assessment of somebody with no history is not an assessment.
    RankTooLow {
        needs: &'static str,
    },
    /// Somebody with an advance already outstanding on an invoice the client
    /// never paid. Lending again into the same situation helps nobody.
    OutstandingWriteOff,
}

/// The rank floor for an advance.
pub const ADVANCE_MIN_RANK: &str = "artisan";

/// Whether somebody may take an advance, from facts rather than from a score.
///
/// Kept as a function over plain values so the rule can be read, argued with
/// and changed in one place — an eligibility rule spread across three queries
/// is a rule nobody can state.
pub fn eligibility(invoice_status: &str, rank: &str, written_off_advances: i64) -> Eligibility {
    if invoice_status != "issued" {
        return Eligibility::InvoiceNotAdvanceable;
    }
    if written_off_advances > 0 {
        return Eligibility::OutstandingWriteOff;
    }
    if !crate::services::ambassadors::rank_clears(rank, ADVANCE_MIN_RANK) {
        return Eligibility::RankTooLow {
            needs: ADVANCE_MIN_RANK,
        };
    }
    Eligibility::Eligible
}

/// What a guarantee will actually pay on a claim.
///
/// Bounded by the per-mission ceiling and by what is left of the year. Both
/// caps, in one function, because applying one and forgetting the other is
/// how a scheme pays out more than it sold.
pub fn guarantee_payout(
    claimed: &BigDecimal,
    max_per_mission: &BigDecimal,
    annual_cap: &BigDecimal,
    already_paid_this_year: &BigDecimal,
) -> BigDecimal {
    let left_this_year = annual_cap - already_paid_this_year;
    if !left_this_year.is_positive() {
        return BigDecimal::from(0);
    }
    let mut payout = claimed.clone();
    if payout > *max_per_mission {
        payout = max_per_mission.clone();
    }
    if payout > left_this_year {
        payout = left_this_year;
    }
    payout
}

// ═══════════════════════════════════════════════════════════════════
// Partnerships and referrals
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Partnership {
    pub id: Uuid,
    pub partner_org: String,
    pub kind: String,
    pub countries: Vec<String>,
    pub commission_percent: BigDecimal,
    pub regulatory_basis: Option<String>,
    pub registry_url: Option<String>,
    pub min_rank: Option<String>,
    pub status: String,
}

const PARTNERSHIP_SELECT: &str = r#"
    SELECT id, partner_org, kind, countries, commission_percent, regulatory_basis,
           registry_url, min_rank, status
      FROM financial_partnerships
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct PartnershipInput {
    pub partner_org: String,
    pub kind: String,
    pub countries: Vec<String>,
    pub commission_percent: BigDecimal,
    #[serde(default)]
    pub regulatory_basis: Option<String>,
    #[serde(default)]
    pub registry_url: Option<String>,
    #[serde(default)]
    pub contract_url: Option<String>,
    #[serde(default)]
    pub min_rank: Option<String>,
}

pub async fn open_partnership(
    db: &PgPool,
    input: PartnershipInput,
) -> Result<Partnership, AppError> {
    if !PARTNERSHIP_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            PARTNERSHIP_KINDS.join(", ")
        )));
    }
    if input.countries.is_empty() {
        return Err(AppError::Validation(
            "say which countries the partner is licensed in. An introduction made \
             outside their licence is the one that gets both of us fined."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO financial_partnerships
            (partner_org, kind, countries, commission_percent, regulatory_basis,
             registry_url, contract_url, min_rank)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(input.partner_org.trim())
    .bind(&input.kind)
    .bind(&input.countries)
    .bind(&input.commission_percent)
    .bind(input.regulatory_basis.as_deref())
    .bind(input.registry_url.as_deref())
    .bind(input.contract_url.as_deref())
    .bind(input.min_rank.as_deref())
    .fetch_one(db)
    .await?;

    partnership(db, id).await
}

pub async fn partnership(db: &PgPool, id: Uuid) -> Result<Partnership, AppError> {
    let sql = format!("{PARTNERSHIP_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Partnership>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("partnership not found".into()))
}

/// What a person can actually be introduced to, in their country.
pub async fn open_partnerships(
    db: &PgPool,
    country: Option<&str>,
) -> Result<Vec<Partnership>, AppError> {
    let sql = format!(
        "{PARTNERSHIP_SELECT} WHERE status = 'active'
            AND ($1::CHAR(2) IS NULL OR $1 = ANY(countries))
          ORDER BY kind, partner_org"
    );
    let rows = sqlx::query_as::<_, Partnership>(sqlx::AssertSqlSafe(sql))
        .bind(country)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Turn a partnership on, once it is permitted.
pub async fn activate_partnership(db: &PgPool, id: Uuid) -> Result<Partnership, AppError> {
    sqlx::query("UPDATE financial_partnerships SET status = 'active' WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| {
            if e.to_string()
                .contains("an_active_partnership_states_its_permission")
            {
                AppError::Validation(
                    "a live partnership states what permits it and points at a signed \
                     contract. Introducing somebody to a lender or an insurer is a \
                     regulated act, and this is the switch that document turns on."
                        .into(),
                )
            } else {
                AppError::from(e)
            }
        })?;
    partnership(db, id).await
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReferralInput {
    pub partnership_id: Uuid,
    pub purpose: String,
    #[serde(default)]
    pub amount_requested: Option<BigDecimal>,
    #[serde(default)]
    pub coverage_requested: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

/// Ask to be introduced.
///
/// Started by the person, always. Nobody is referred to a lender because an
/// algorithm noticed they might need money.
pub async fn request_referral(
    db: &PgPool,
    user_id: Uuid,
    input: ReferralInput,
) -> Result<Uuid, AppError> {
    let partnership = partnership(db, input.partnership_id).await?;
    if partnership.status != "active" {
        return Err(AppError::Validation(
            "that partnership is not open for introductions".into(),
        ));
    }
    if input.purpose.trim().is_empty() {
        return Err(AppError::Validation(
            "say what it is for. The partner will ask, and answering for you would be \
             putting words in your mouth on a credit file."
                .into(),
        ));
    }

    if let Some(floor) = &partnership.min_rank {
        let rank: Option<String> =
            sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(db)
                .await?;
        let rank = rank.unwrap_or_else(|| "apprenti".into());
        if !crate::services::ambassadors::rank_clears(&rank, floor) {
            return Err(AppError::Validation(format!(
                "this partner asks for {floor} and you are {rank}. They are pricing on \
                 our assessment, and an assessment of somebody with no history is not \
                 an assessment."
            )));
        }
    }

    // What is passed on, recorded. The person is entitled to know what was
    // said about them, and the partner priced on it.
    let snapshot: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'rank', (SELECT rank FROM user_ranks WHERE user_id = $1),
                    'attestations', (SELECT count(*) FROM attestations
                                      WHERE user_id = $1 AND revoked_at IS NULL),
                    'craft_scores', (SELECT COALESCE(
                                        jsonb_object_agg(skill_domain, score), '{}'::jsonb)
                                       FROM craft_scores WHERE user_id = $1),
                    'member_since', (SELECT created_at FROM users WHERE id = $1)
                )",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO partnership_referrals
            (partnership_id, user_id, purpose, amount_requested, coverage_requested,
             currency, shared_snapshot)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         RETURNING id",
    )
    .bind(input.partnership_id)
    .bind(user_id)
    .bind(input.purpose.trim())
    .bind(input.amount_requested.as_ref())
    .bind(input.coverage_requested.as_ref())
    .bind(&input.currency)
    .bind(&snapshot)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// The partner decided.
pub async fn record_decision(
    db: &PgPool,
    referral_id: Uuid,
    approved: bool,
    approved_amount: Option<BigDecimal>,
    monthly_premium: Option<BigDecimal>,
    note: Option<&str>,
) -> Result<Option<BigDecimal>, AppError> {
    let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT r.partnership_id, r.user_id, r.currency, r.decision
           FROM partnership_referrals r WHERE r.id = $1",
    )
    .bind(referral_id)
    .fetch_optional(db)
    .await?;
    let (partnership_id, user_id, currency, decision) =
        row.ok_or_else(|| AppError::NotFound("referral not found".into()))?;

    if decision != "pending" {
        return Err(AppError::Validation(format!(
            "this referral is already {decision}"
        )));
    }

    if !approved {
        sqlx::query(
            "UPDATE partnership_referrals
                SET decision = 'rejected', decided_at = NOW(), decision_note = $2
              WHERE id = $1",
        )
        .bind(referral_id)
        .bind(note.map(str::trim).filter(|n| !n.is_empty()))
        .execute(db)
        .await?;
        return Ok(None);
    }

    let partnership = partnership(db, partnership_id).await?;

    // The commission is a share of what was granted, or of a year of
    // premiums for a policy. Twelve months rather than the policy's whole
    // life: a commission on a renewal nobody has made yet is revenue
    // recognised on a guess.
    let base = approved_amount
        .clone()
        .or_else(|| monthly_premium.as_ref().map(|p| p * BigDecimal::from(12)))
        .ok_or_else(|| {
            AppError::Validation(
                "an approval has to say what was granted — an amount or a premium".into(),
            )
        })?;

    let commission = (&base * &partnership.commission_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::HalfUp);

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE partnership_referrals
            SET decision = 'approved', decided_at = NOW(), decision_note = $2,
                approved_amount = $3, monthly_premium = $4,
                commission_amount = $5, commission_booked_at = NOW(),
                started_on = CURRENT_DATE
          WHERE id = $1",
    )
    .bind(referral_id)
    .bind(note.map(str::trim).filter(|n| !n.is_empty()))
    .bind(approved_amount.as_ref())
    .bind(monthly_premium.as_ref())
    .bind(&commission)
    .execute(&mut *tx)
    .await?;

    if commission.is_positive() {
        let stream = if partnership.kind == "loan" {
            "factoring_take"
        } else {
            "insurance_commission"
        };
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_talent_id, amount_credits, fee_rate_bps, notes)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(stream)
        .bind(user_id)
        .bind(&commission)
        .bind(ledger::percent_to_bps(&partnership.commission_percent))
        .bind(format!("apport {} ({currency})", partnership.partner_org))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(Some(commission))
}

// ═══════════════════════════════════════════════════════════════════
// Advances
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Advance {
    pub id: Uuid,
    pub user_id: Uuid,
    pub invoice_id: Uuid,
    pub expected_payment: BigDecimal,
    pub advance_percent: BigDecimal,
    pub advance_amount: BigDecimal,
    pub fee_percent: BigDecimal,
    pub fee_amount: BigDecimal,
    pub currency: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const ADVANCE_SELECT: &str = r#"
    SELECT id, user_id, invoice_id, expected_payment, advance_percent,
           advance_amount, fee_percent, fee_amount, currency, status, created_at
      FROM advance_pay_requests
"#;

/// Ask for an advance on one issued invoice.
///
/// Eligibility is decided here and the reason is returned in full. "Refused"
/// with no reason is the answer people remember, and the one they cannot act
/// on.
pub async fn request_advance(
    db: &PgPool,
    user_id: Uuid,
    invoice_id: Uuid,
    advance_percent: BigDecimal,
) -> Result<Advance, AppError> {
    let invoice: Option<(BigDecimal, String, String)> =
        sqlx::query_as("SELECT amount, currency, status FROM mission_invoices WHERE id = $1")
            .bind(invoice_id)
            .fetch_optional(db)
            .await?;
    let (amount, currency, invoice_status) =
        invoice.ok_or_else(|| AppError::NotFound("invoice not found".into()))?;

    let rank: Option<String> = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    let rank = rank.unwrap_or_else(|| "apprenti".into());

    let written_off: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM advance_pay_requests
          WHERE user_id = $1 AND status = 'written_off'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    match eligibility(&invoice_status, &rank, written_off) {
        Eligibility::Eligible => {}
        Eligibility::InvoiceNotAdvanceable => {
            return Err(AppError::Validation(format!(
                "this invoice is {invoice_status}. An advance is money already owed on \
                 work delivered — against anything else it would be a loan, which is \
                 not what this is."
            )));
        }
        Eligibility::RankTooLow { needs } => {
            return Err(AppError::Validation(format!(
                "an advance opens at {needs} and you are {rank}. It is priced on our \
                 assessment of you, and an assessment with no history behind it is not \
                 an assessment."
            )));
        }
        Eligibility::OutstandingWriteOff => {
            return Err(AppError::Validation(
                "you have an advance a client never paid. Advancing again into the same \
                 situation helps nobody — talk to us instead."
                    .into(),
            ));
        }
    }

    let fee_percent = BigDecimal::try_from(DEFAULT_ADVANCE_FEE).unwrap_or_default();
    let (gross, fee, _net) = advance_figures(&amount, &advance_percent, &fee_percent);

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO advance_pay_requests
            (user_id, invoice_id, expected_payment, advance_percent, advance_amount,
             fee_percent, fee_amount, currency)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(user_id)
    .bind(invoice_id)
    .bind(&amount)
    .bind(&advance_percent)
    .bind(&gross)
    .bind(&fee_percent)
    .bind(&fee)
    .bind(&currency)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("idx_one_live_advance_per_invoice") {
            AppError::Validation(
                "there is already an advance on this invoice. A second would advance \
                 more than the invoice is worth, with nothing to repay it from."
                    .into(),
            )
        } else if m.contains("advance_percent") {
            AppError::Validation(
                "an advance runs from 30% to 90% of the invoice. Below that it is not \
                 worth the fee; above it there is nothing left to absorb a dispute."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    advance(db, id).await
}

pub async fn advance(db: &PgPool, id: Uuid) -> Result<Advance, AppError> {
    let sql = format!("{ADVANCE_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Advance>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("advance not found".into()))
}

pub async fn advances_for(db: &PgPool, user_id: Uuid) -> Result<Vec<Advance>, AppError> {
    let sql = format!("{ADVANCE_SELECT} WHERE user_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Advance>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Approve and pay it out, net of the fee.
pub async fn disburse(db: &PgPool, advance_id: Uuid) -> Result<BigDecimal, AppError> {
    let advance = advance(db, advance_id).await?;
    if advance.status != "requested" {
        return Err(AppError::Validation(format!(
            "this advance is {} — only a new request can be paid out",
            advance.status
        )));
    }

    let net = &advance.advance_amount - &advance.fee_amount;

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE advance_pay_requests
            SET status = 'disbursed', approved_at = NOW(), disbursed_at = NOW()
          WHERE id = $1",
    )
    .bind(advance_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, amount_credits, fee_rate_bps, notes)
         VALUES ('factoring_take', $1, $2, $3, $4)",
    )
    .bind(advance.user_id)
    .bind(&advance.fee_amount)
    .bind(ledger::percent_to_bps(&advance.fee_percent))
    .bind(format!("avance sur la facture {}", advance.invoice_id))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let currency: ledger::Currency = advance.currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("advance:{advance_id}"),
        advance.user_id,
        net.clone(),
        BigDecimal::from(0),
        currency,
        "advance_pay",
        advance_id,
    )
    .await?;

    Ok(net)
}

/// The client paid; the advance is settled.
pub async fn mark_repaid(db: &PgPool, advance_id: Uuid) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE advance_pay_requests SET status = 'repaid', repaid_at = NOW()
          WHERE id = $1 AND status = 'disbursed'",
    )
    .bind(advance_id)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no outstanding advance with that id".into(),
        ));
    }
    Ok(())
}

/// The client never paid.
///
/// Skilluv carries it. The contributor keeps the money, which is the whole
/// reason the fee exists — an advance the recipient has to give back on a
/// client's default is not an advance, it is a loan with extra steps.
pub async fn write_off(db: &PgPool, advance_id: Uuid, reason: &str) -> Result<(), AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation("say what happened".into()));
    }
    sqlx::query(
        "UPDATE advance_pay_requests
            SET status = 'written_off', written_off_at = NOW(), written_off_reason = $2
          WHERE id = $1 AND status = 'disbursed'",
    )
    .bind(advance_id)
    .bind(reason.trim())
    .execute(db)
    .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// The payment guarantee
// ═══════════════════════════════════════════════════════════════════

/// What each tier costs and covers.
pub fn guarantee_tier(tier: &str) -> Option<(f64, f64, f64)> {
    match tier {
        "basic" => Some((5.0, 500.0, 1500.0)),
        "premium" => Some((20.0, 3000.0, 9000.0)),
        _ => None,
    }
}

pub async fn subscribe_guarantee(
    db: &PgPool,
    user_id: Uuid,
    tier: &str,
) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    let (fee, per_mission, annual) = guarantee_tier(tier)
        .ok_or_else(|| AppError::Validation("tier must be one of: basic, premium".into()))?;

    let expires: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "INSERT INTO payment_guarantee_subscriptions
            (user_id, tier, monthly_fee, max_per_mission, annual_cap, expires_at)
         VALUES ($1,$2,$3,$4,$5, NOW() + INTERVAL '30 days')
         ON CONFLICT (user_id) DO UPDATE
             SET tier = EXCLUDED.tier,
                 monthly_fee = EXCLUDED.monthly_fee,
                 max_per_mission = EXCLUDED.max_per_mission,
                 annual_cap = EXCLUDED.annual_cap,
                 -- Extend from whichever is later, so renewing early does not
                 -- throw away time already paid for.
                 expires_at = GREATEST(payment_guarantee_subscriptions.expires_at, NOW())
                              + INTERVAL '30 days',
                 cancelled_at = NULL,
                 auto_renew = TRUE
         RETURNING expires_at",
    )
    .bind(user_id)
    .bind(tier)
    .bind(BigDecimal::try_from(fee).unwrap_or_default())
    .bind(BigDecimal::try_from(per_mission).unwrap_or_default())
    .bind(BigDecimal::try_from(annual).unwrap_or_default())
    .fetch_one(db)
    .await?;

    Ok(expires)
}

/// Pay a contributor for work a client refused to pay for.
pub async fn honour_guarantee(
    db: &PgPool,
    user_id: Uuid,
    invoice_id: Option<Uuid>,
    claimed: BigDecimal,
    reason: &str,
) -> Result<BigDecimal, AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the dispute was about — the claim is the record we chase the \
             client with"
                .into(),
        ));
    }

    let cover: Option<(BigDecimal, BigDecimal, String)> = sqlx::query_as(
        "SELECT max_per_mission, annual_cap, currency
           FROM payment_guarantee_subscriptions
          WHERE user_id = $1 AND cancelled_at IS NULL AND expires_at > NOW()",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let (per_mission, annual_cap, currency) = cover
        .ok_or_else(|| AppError::Validation("no live payment guarantee on this account".into()))?;

    let year = chrono::Datelike::year(&chrono::Utc::now()) as i16;
    let already: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT COALESCE(sum(amount), 0) FROM payment_guarantee_claims
          WHERE user_id = $1 AND counts_for_year = $2 AND status IN ('paid', 'recovered')",
    )
    .bind(user_id)
    .bind(year)
    .fetch_one(db)
    .await?;
    let already = already.unwrap_or_else(|| BigDecimal::from(0));

    let payout = guarantee_payout(&claimed, &per_mission, &annual_cap, &already);
    if !payout.is_positive() {
        return Err(AppError::Validation(format!(
            "this year's cover is used up. The cap is what the subscription sold, and \
             paying past it would be paying out of the next person's premium.\n\
             Already covered this year: {already}."
        )));
    }

    let claim_id: Uuid = sqlx::query_scalar(
        "INSERT INTO payment_guarantee_claims
            (user_id, invoice_id, amount, currency, counts_for_year, reason,
             status, paid_at)
         VALUES ($1,$2,$3,$4,$5,$6,'paid',NOW())
         RETURNING id",
    )
    .bind(user_id)
    .bind(invoice_id)
    .bind(&payout)
    .bind(&currency)
    .bind(year)
    .bind(reason.trim())
    .fetch_one(db)
    .await?;

    let currency: ledger::Currency = currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("guarantee:{claim_id}"),
        user_id,
        payout.clone(),
        BigDecimal::from(0),
        currency,
        "payment_guarantee",
        claim_id,
    )
    .await?;

    Ok(payout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn the_fee_comes_off_the_advance_not_out_of_the_repayment() {
        // The contributor sees the net before agreeing, which is the number
        // they actually care about.
        let (gross, fee, net) = advance_figures(&dec("1000.00"), &dec("50.00"), &dec("4.00"));
        assert_eq!(gross, dec("500.00"));
        assert_eq!(fee, dec("20.00"));
        assert_eq!(net, dec("480.00"));
    }

    #[test]
    fn an_advance_never_exceeds_the_invoice() {
        for percent in ["30.00", "50.00", "90.00"] {
            let (gross, _, _) = advance_figures(&dec("1000.00"), &dec(percent), &dec("4.00"));
            assert!(gross <= dec("1000.00"));
        }
    }

    #[test]
    fn a_paid_invoice_cannot_be_advanced_against() {
        // An advance is money already owed on work delivered. Against
        // anything else it is a loan.
        assert_eq!(
            eligibility("paid", "maitre", 0),
            Eligibility::InvoiceNotAdvanceable
        );
        assert_eq!(
            eligibility("cancelled", "maitre", 0),
            Eligibility::InvoiceNotAdvanceable
        );
    }

    #[test]
    fn the_rank_floor_holds() {
        assert_eq!(
            eligibility("issued", "ranger", 0),
            Eligibility::RankTooLow { needs: "artisan" }
        );
        assert_eq!(eligibility("issued", "artisan", 0), Eligibility::Eligible);
        assert_eq!(eligibility("issued", "doyen", 0), Eligibility::Eligible);
    }

    #[test]
    fn an_unpaid_write_off_stops_the_next_advance() {
        // Advancing again into the same situation helps nobody.
        assert_eq!(
            eligibility("issued", "doyen", 1),
            Eligibility::OutstandingWriteOff
        );
    }

    #[test]
    fn a_guarantee_pays_the_smaller_of_the_claim_and_both_caps() {
        // Under everything: the claim.
        assert_eq!(
            guarantee_payout(&dec("300"), &dec("500"), &dec("1500"), &dec("0")),
            dec("300")
        );
        // Over the per-mission cap: the cap.
        assert_eq!(
            guarantee_payout(&dec("900"), &dec("500"), &dec("1500"), &dec("0")),
            dec("500")
        );
        // Over what is left of the year: what is left.
        assert_eq!(
            guarantee_payout(&dec("500"), &dec("500"), &dec("1500"), &dec("1300")),
            dec("200")
        );
    }

    #[test]
    fn a_spent_annual_cap_pays_nothing_rather_than_a_negative() {
        assert_eq!(
            guarantee_payout(&dec("500"), &dec("500"), &dec("1500"), &dec("1500")),
            dec("0")
        );
        assert_eq!(
            guarantee_payout(&dec("500"), &dec("500"), &dec("1500"), &dec("2000")),
            dec("0")
        );
    }

    #[test]
    fn every_tier_covers_at_least_one_full_mission_a_year() {
        for tier in ["basic", "premium"] {
            let (fee, per_mission, annual) = guarantee_tier(tier).unwrap();
            assert!(fee > 0.0);
            assert!(annual >= per_mission, "{tier} sells a claim it cannot pay");
        }
        assert!(guarantee_tier("platinum").is_none());
    }

    #[test]
    fn the_default_advance_fee_is_inside_the_band() {
        assert!(DEFAULT_ADVANCE_FEE > 0.0);
        assert!(DEFAULT_ADVANCE_FEE <= MAX_ADVANCE_FEE);
    }

    #[test]
    fn every_partnership_kind_is_a_known_one() {
        assert_eq!(PARTNERSHIP_KINDS.len(), 5);
        assert!(PARTNERSHIP_KINDS.contains(&"insurance_health"));
    }
}
