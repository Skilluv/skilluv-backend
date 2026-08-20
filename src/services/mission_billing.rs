//! Paying for a mission (migration 0194).
//!
//! ## The shape
//!
//! The enterprise issues an invoice, pays it through the same collection
//! machinery as everything else, and the talent's share lands in their
//! *pending* balance. It becomes withdrawable when the mission closes —
//! which is the client accepting delivery, not the talent declaring it.
//!
//! That window is the whole point of `pending`: a payer who is unhappy has
//! somewhere to raise it while the money is still reversible, and the
//! existing dispute machinery works on exactly that state.
//!
//! ## Why the platform's share is subtracted here and nowhere else
//!
//! `capture_for_recipient` takes a gross and a platform share and splits
//! them in one posting. Computing the split anywhere else would mean two
//! places that must agree on rounding, and they eventually would not.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

/// The mission fields an invoice is priced from.
#[derive(sqlx::FromRow)]
struct PricingTerms {
    payment_model: String,
    budget_eur: Option<BigDecimal>,
    hourly_rate_eur: Option<BigDecimal>,
    commission_percent: BigDecimal,
}

/// What a capture needs to know: the amount, who it is for, and where it came
/// from.
#[derive(sqlx::FromRow)]
struct CaptureContext {
    mission_id: Uuid,
    amount: BigDecimal,
    currency: String,
    commission_percent: BigDecimal,
    assigned_user_id: Option<Uuid>,
    provider: String,
    provider_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub mission_id: Uuid,
    pub sequence: i16,
    pub label: String,
    pub amount: BigDecimal,
    pub currency: String,
    pub commission_percent: BigDecimal,
    pub period_start: Option<chrono::NaiveDate>,
    pub period_end: Option<chrono::NaiveDate>,
    pub hours: Option<BigDecimal>,
    pub status: String,
    pub payment_id: Option<Uuid>,
    pub issued_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueInput {
    pub label: String,
    /// Absent for `fixed_price`, where the mission's budget is the amount and
    /// asking the enterprise to retype it is an invitation to typo it.
    #[serde(default)]
    pub amount: Option<BigDecimal>,
    #[serde(default)]
    pub period_start: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub period_end: Option<chrono::NaiveDate>,
    /// `per_hour` only. The amount is derived from it and the agreed rate,
    /// rather than typed — a rate agreed in writing and an amount typed by
    /// hand is how invoices end up disputed.
    #[serde(default)]
    pub hours: Option<BigDecimal>,
    /// A cost passed through rather than work billed: rented compute, mostly.
    /// Carries no commission and needs a receipt.
    #[serde(default)]
    pub expense_evidence_url: Option<String>,
}

/// Put an amount on the mission's account.
pub async fn issue(db: &PgPool, mission_id: Uuid, input: IssueInput) -> Result<Invoice, AppError> {
    let label = input.label.trim();
    if label.is_empty() {
        return Err(AppError::Validation(
            "an invoice needs a label saying what it covers".into(),
        ));
    }
    crate::validators::check_max_len(label, "label", 200)?;

    let terms: Option<PricingTerms> = sqlx::query_as(
        "SELECT payment_model, budget_eur, hourly_rate_eur, commission_percent
           FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;
    let terms = terms.ok_or_else(|| AppError::NotFound("mission not found".into()))?;
    let PricingTerms {
        payment_model,
        budget_eur: budget,
        hourly_rate_eur: hourly_rate,
        commission_percent: commission,
    } = terms;

    // A reimbursement is not priced by the payment model: it is what the
    // receipt says. And it carries no commission — charging on money the
    // platform is only passing through would mean somebody pays to be repaid,
    // and the more honest they are about their costs the more it costs them.
    let is_reimbursement = input.expense_evidence_url.is_some();
    let commission = if is_reimbursement {
        BigDecimal::from(0)
    } else {
        commission
    };

    let amount = if is_reimbursement {
        input.amount.clone().ok_or_else(|| {
            AppError::Validation("a reimbursement must state what was spent".into())
        })?
    } else {
        match payment_model.as_str() {
            "per_hour" => {
                let hours = input.hours.clone().ok_or_else(|| {
                    AppError::Validation("a per_hour invoice must state the hours".into())
                })?;
                if !hours.is_positive() {
                    return Err(AppError::Validation("hours must be positive".into()));
                }
                let rate = hourly_rate.ok_or_else(|| {
                    AppError::Internal("a per_hour mission with no agreed rate".into())
                })?;
                hours * rate
            }
            "fixed_price" => input.amount.clone().or(budget).ok_or_else(|| {
                AppError::Validation("this mission has no budget to invoice".into())
            })?,
            // A retainer's monthly figure and a per-deliverable amount both live
            // on the invoice: the budget is the agreed unit, and an instalment
            // can legitimately differ from it — a half month, a smaller feature.
            _ => input
                .amount
                .clone()
                .or_else(|| budget.clone())
                .ok_or_else(|| AppError::Validation("this invoice needs an amount".into()))?,
        }
    };

    if !amount.is_positive() {
        return Err(AppError::Validation(
            "an invoice for nothing is not an invoice".into(),
        ));
    }

    let invoice: Invoice = sqlx::query_as(
        r#"
        INSERT INTO mission_invoices
            (mission_id, sequence, label, amount, commission_percent,
             period_start, period_end, hours, kind, expense_evidence_url)
        VALUES (
            $1,
            (SELECT COALESCE(max(sequence), 0) + 1 FROM mission_invoices WHERE mission_id = $1),
            $2, $3, $4, $5, $6, $7,
            CASE WHEN $8::TEXT IS NULL THEN 'work' ELSE 'expense_reimbursement' END,
            $8
        )
        RETURNING id, mission_id, sequence, label, amount, currency,
                  commission_percent, period_start, period_end, hours,
                  status, payment_id, issued_at
        "#,
    )
    .bind(mission_id)
    .bind(label)
    .bind(&amount)
    .bind(&commission)
    .bind(input.period_start)
    .bind(input.period_end)
    .bind(input.hours.as_ref())
    .bind(input.expense_evidence_url.as_deref())
    .fetch_one(db)
    .await
    .map_err(assignment_error)?;

    Ok(invoice)
}

/// The trigger speaks SQL; this says the same in words the enterprise can act
/// on.
fn assignment_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    for marker in ["nobody on it", "invoices are closed"] {
        if let Some(start) = message.find(marker) {
            let sentence: String = message[start..].lines().next().unwrap_or("").into();
            return AppError::Validation(sentence);
        }
    }
    AppError::from(e)
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Invoice, AppError> {
    sqlx::query_as::<_, Invoice>(
        "SELECT id, mission_id, sequence, label, amount, currency,
                commission_percent, period_start, period_end, hours,
                status, payment_id, issued_at
           FROM mission_invoices WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("invoice not found".into()))
}

pub async fn for_mission(db: &PgPool, mission_id: Uuid) -> Result<Vec<Invoice>, AppError> {
    let rows = sqlx::query_as::<_, Invoice>(
        "SELECT id, mission_id, sequence, label, amount, currency,
                commission_percent, period_start, period_end, hours,
                status, payment_id, issued_at
           FROM mission_invoices WHERE mission_id = $1 ORDER BY sequence",
    )
    .bind(mission_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// What the platform keeps on this invoice, rounded to the currency's
/// smallest unit.
///
/// Rounded down, so the split can never credit more than was captured — a
/// posting whose legs do not balance is refused, and the round that produces
/// it would only ever be discovered in production.
pub fn platform_share(amount: &BigDecimal, commission_percent: &BigDecimal) -> BigDecimal {
    (amount * commission_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down)
}

/// The money arrived. Credit the talent's pending balance and the platform's
/// revenue in one posting.
///
/// Called from `fulfilment::deliver`, which claims delivery exactly once, so
/// this runs once per invoice — and the ledger's idempotency key makes a
/// second call harmless anyway.
pub async fn capture(db: &PgPool, invoice_id: Uuid, payment_id: Uuid) -> Result<(), AppError> {
    let row: Option<CaptureContext> = sqlx::query_as(
        "SELECT i.mission_id, i.amount, i.currency, i.commission_percent,
                m.assigned_user_id, p.provider, p.provider_reference
           FROM mission_invoices i
           JOIN missions m ON m.id = i.mission_id
           JOIN payments p ON p.id = $2
          WHERE i.id = $1",
    )
    .bind(invoice_id)
    .bind(payment_id)
    .fetch_optional(db)
    .await?;
    let CaptureContext {
        mission_id,
        amount,
        currency,
        commission_percent: commission,
        assigned_user_id: assignee,
        provider,
        provider_reference: reference,
    } = row.ok_or_else(|| AppError::NotFound("invoice not found".into()))?;

    let Some(recipient) = assignee else {
        // Loud rather than silent: money was taken for a mission with nobody
        // on it, and nothing else would notice.
        return Err(AppError::Internal(format!(
            "invoice {invoice_id} was paid but mission {mission_id} has nobody assigned"
        )));
    };

    let currency: ledger::Currency = currency.parse()?;
    let share = platform_share(&amount, &commission);

    // `capture_for_recipient` takes a &'static str for the provider, because
    // the account it names is a compile-time thing. The set is closed and
    // small; an unknown one is a deployment problem, not a payment problem.
    let provider_static = ledger_provider(&provider)?;

    ledger::capture_for_recipient(
        db,
        provider_static,
        reference.unwrap_or_else(|| format!("invoice:{invoice_id}")),
        recipient,
        amount,
        share.clone(),
        currency,
        "mission_invoice",
        invoice_id,
    )
    .await?;

    sqlx::query(
        "UPDATE mission_invoices
            SET status = 'paid', payment_id = $2, captured_at = NOW()
          WHERE id = $1 AND status = 'issued'",
    )
    .bind(invoice_id)
    .bind(payment_id)
    .execute(db)
    .await?;

    // The marketplace's own line in the revenue ledger. The posting above
    // already credited the platform account; this is the row an accountant
    // reads to see which stream it came from, and without it marketplace
    // revenue would be invisible next to bounties and mentoring.
    if share.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_talent_id, amount_credits, fee_rate_bps, notes)
             VALUES ('mission_marketplace', $1, $2, $3, $4)",
        )
        .bind(recipient)
        .bind(&share)
        .bind(ledger::percent_to_bps(&commission))
        .bind(format!(
            "commission {commission}% sur la facture {invoice_id}"
        ))
        .execute(db)
        .await?;
    }

    Ok(())
}

/// The mission closed. Everything captured becomes withdrawable.
///
/// Driven by the mission's status rather than by each invoice: the client
/// accepting delivery is one event, and releasing the March instalment while
/// holding April's would mean nothing to either party.
pub async fn release_all(db: &PgPool, mission_id: Uuid) -> Result<u64, AppError> {
    let invoices: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mission_invoices WHERE mission_id = $1 AND status = 'paid'",
    )
    .bind(mission_id)
    .fetch_all(db)
    .await?;

    let mut released = 0u64;
    for id in invoices {
        if release_one(db, id).await? {
            released += 1;
        }
    }
    Ok(released)
}

/// One captured invoice becomes withdrawable.
///
/// Split out of `release_all` because `milestone_iteration` releases as it
/// goes: an accepted round pays its own instalment, and the rest stays in
/// escrow until its round is accepted too. Two code paths releasing money
/// would be two chances to release it twice, so there is one.
///
/// Returns false for an invoice that was not captured. That is not an error:
/// money nobody has put up cannot be released, and paying the designer out of
/// the platform's pocket would be the alternative.
pub async fn release_one(db: &PgPool, invoice_id: Uuid) -> Result<bool, AppError> {
    let row: Option<(Uuid, BigDecimal, String, BigDecimal)> = sqlx::query_as(
        "SELECT m.assigned_user_id, i.amount, i.currency, i.commission_percent
           FROM mission_invoices i
           JOIN missions m ON m.id = i.mission_id
          WHERE i.id = $1 AND i.status = 'paid' AND m.assigned_user_id IS NOT NULL",
    )
    .bind(invoice_id)
    .fetch_optional(db)
    .await?;

    let Some((recipient, amount, currency, commission)) = row else {
        return Ok(false);
    };

    let currency: ledger::Currency = currency.parse()?;
    // The recipient's share, not the gross: the platform's half was never in
    // their pending balance to release.
    let theirs = amount.clone() - platform_share(&amount, &commission);

    ledger::release(
        db,
        recipient,
        theirs,
        currency,
        "mission_invoice",
        invoice_id,
    )
    .await?;

    let done = sqlx::query(
        "UPDATE mission_invoices SET status = 'released', released_at = NOW()
          WHERE id = $1 AND status = 'paid'",
    )
    .bind(invoice_id)
    .execute(db)
    .await?;

    Ok(done.rows_affected() > 0)
}

/// The provider names the ledger knows.
fn ledger_provider(name: &str) -> Result<&'static str, AppError> {
    match name {
        "stripe" => Ok("stripe"),
        "fedapay" => Ok("fedapay"),
        other => Err(AppError::Internal(format!(
            "the ledger has no account for provider '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn the_standard_rate_is_fifteen_percent() {
        assert_eq!(
            platform_share(&dec("1000.00"), &dec("15.00")),
            dec("150.00")
        );
    }

    #[test]
    fn the_featured_rate_is_ten() {
        assert_eq!(
            platform_share(&dec("1000.00"), &dec("10.00")),
            dec("100.00")
        );
    }

    #[test]
    fn the_share_is_rounded_down_so_the_legs_balance() {
        // 15% of 33.33 is 4.9995. Rounding up would credit the platform half
        // a centime that was never captured, and the posting would be
        // refused — in production, on somebody's invoice.
        assert_eq!(platform_share(&dec("33.33"), &dec("15.00")), dec("4.99"));
    }

    #[test]
    fn a_free_mission_costs_nothing() {
        assert_eq!(platform_share(&dec("500.00"), &dec("0.00")), dec("0.00"));
    }
}
