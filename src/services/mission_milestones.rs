//! Paying a design mission as it is delivered.
//!
//! ## What the other models could not say
//!
//! A brand identity is one job, handed in three or four times, and the
//! designer carries all of it unpaid until the client is happy with the last
//! round. `fixed_price` says that and means it. `per_deliverable` is the near
//! miss — but a round is not a deliverable, it is the same deliverable again,
//! and calling four rounds four deliverables lets a client pay four times for
//! one job.
//!
//! `milestone_iteration` is the fifth answer: one budget, released in agreed
//! shares as rounds are accepted.
//!
//! ## The invoices exist before the rounds do
//!
//! All of them are raised when the mission is assigned, not one at a time.
//! A designer starting work needs to see the whole schedule — how much each
//! round releases and how much is held to the end — and an enterprise needs
//! to fund the whole job rather than be asked again every fortnight.
//!
//! ## Commission is settled when the second party is known
//!
//! `commission_percent` is frozen at publication so nobody can move it
//! afterwards. That is right, and it is also why the two exceptions cannot
//! both live there: a charity brief is a property of the mission, but a
//! loyalty discount is a property of *whoever takes it*, and at publication
//! nobody knows who that is.
//!
//! So the rate is settled at assignment, when both parties exist, and the
//! reason is written next to it. The loyalty half of that rule already lived
//! in `missions::commission_for`; what this adds is the charity case and the
//! written reason, both of them there rather than in a second rule here.

use bigdecimal::{BigDecimal, FromPrimitive, Zero};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The default when a mission does not name its own.
///
/// A reasonable default and a terrible rule: some jobs front-load the work
/// and some end in a week of production. It exists so a mission can be posted
/// without an argument about percentages, not so every mission uses it.
pub const DEFAULT_SPLIT: &[i32] = &[20, 20, 20, 40];

/// Split a budget into shares that add back up to it.
///
/// The last share takes the remainder rather than its own percentage. Four
/// roundings of a budget that does not divide cleanly leave a few cents
/// stranded in escrow with nothing to release them — and nobody notices until
/// the last mission of the year.
pub fn amounts_for(budget: &BigDecimal, split: &[i32]) -> Vec<BigDecimal> {
    let hundred = BigDecimal::from(100);
    let mut out = Vec::with_capacity(split.len());
    let mut running = BigDecimal::zero();

    for (index, share) in split.iter().enumerate() {
        if index + 1 == split.len() {
            out.push(budget - &running);
            break;
        }
        let share = BigDecimal::from_i32(*share).unwrap_or_else(BigDecimal::zero);
        // Two decimals, because euros have two. `with_scale` truncates, which
        // is the right direction: the remainder lands on the final round
        // rather than being paid twice.
        let amount = (budget * share / &hundred).with_scale(2);
        running += &amount;
        out.push(amount);
    }
    out
}

/// Everything the schedule needs to know, read once.
#[derive(Debug, sqlx::FromRow)]
struct MissionTerms {
    payment_model: String,
    budget_eur: Option<BigDecimal>,
    milestone_split: Option<Vec<i32>>,
    charity_brief: bool,
    assigned_user_id: Option<Uuid>,
}

/// Settle the commission and raise the whole schedule of invoices.
///
/// Called when a mission is assigned. Does nothing for the other payment
/// models: they raise invoices their own way, and this must not be a second
/// path into the same table.
///
/// Idempotent. Assignment can be retried, and a second call must not double
/// the schedule.
pub async fn schedule_on_assignment(db: &PgPool, mission_id: Uuid) -> Result<usize, AppError> {
    let terms: Option<MissionTerms> = sqlx::query_as(
        "SELECT payment_model, budget_eur, milestone_split, charity_brief, assigned_user_id
           FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;

    let Some(terms) = terms else {
        return Ok(0);
    };
    if terms.payment_model != "milestone_iteration" {
        return Ok(0);
    }
    let (Some(budget), Some(split), Some(assignee)) = (
        terms.budget_eur,
        terms.milestone_split,
        terms.assigned_user_id,
    ) else {
        return Ok(0);
    };

    // Already scheduled. The schema cannot express "one schedule per
    // mission" — invoices are legitimately many — so the check is here.
    let existing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission_invoices WHERE mission_id = $1",
    )
    .bind(mission_id)
    .fetch_one(db)
    .await?;
    if existing > 0 {
        return Ok(0);
    }

    let commission =
        crate::services::missions::commission_for(db, assignee, terms.charity_brief).await?;

    let mut tx = db.begin().await?;

    sqlx::query(
        "UPDATE missions SET commission_percent = $2, commission_reason = $3 WHERE id = $1",
    )
    .bind(mission_id)
    .bind(BigDecimal::from_f64(commission.percent).unwrap_or_else(BigDecimal::zero))
    .bind(commission.reason)
    .execute(&mut *tx)
    .await?;

    let amounts = amounts_for(&budget, &split);
    let rounds = amounts.len();

    for (index, amount) in amounts.into_iter().enumerate() {
        let round = index + 1;
        let label = if round == rounds {
            "Solde à l'acceptation finale".to_string()
        } else {
            format!("Jalon {round} — round accepté")
        };

        sqlx::query(
            "INSERT INTO mission_invoices
                 (mission_id, sequence, label, amount, currency, commission_percent, status)
             -- `issued`, not a draft: the whole schedule is handed to the
             -- enterprise at once, and an invoice nobody has been shown is
             -- an invoice nobody will pay.
             VALUES ($1, $2, $3, $4, 'EUR', $5, 'issued')",
        )
        .bind(mission_id)
        .bind(round as i16)
        .bind(&label)
        .bind(&amount)
        .bind(BigDecimal::from_f64(commission.percent).unwrap_or_else(BigDecimal::zero))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(rounds)
}

/// Release the share an accepted round has earned.
///
/// Only what the client has already paid for: an invoice still `issued` is
/// money nobody has put up, and releasing it would pay a designer out of the
/// platform's pocket. The round is recorded either way — the
/// designer earned it, and an unfunded schedule is the enterprise's problem
/// to fix rather than the designer's to discover at the end.
pub async fn release_for_round(
    db: &PgPool,
    mission_id: Uuid,
    round: i16,
) -> Result<bool, AppError> {
    let model: Option<String> =
        sqlx::query_scalar("SELECT payment_model FROM missions WHERE id = $1")
            .bind(mission_id)
            .fetch_optional(db)
            .await?;
    if model.as_deref() != Some("milestone_iteration") {
        return Ok(false);
    }

    let invoice: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mission_invoices
          WHERE mission_id = $1 AND sequence = $2 AND status = 'paid'",
    )
    .bind(mission_id)
    .bind(round)
    .fetch_optional(db)
    .await?;

    let Some(invoice) = invoice else {
        return Ok(false);
    };

    crate::services::mission_billing::release_one(db, invoice).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn eur(value: &str) -> BigDecimal {
        BigDecimal::from_str(value).unwrap()
    }

    #[test]
    fn the_shares_add_back_up_to_the_budget() {
        // The failure this guards against is quiet: a few cents stranded in
        // escrow with nothing to release them, unnoticed until somebody
        // reconciles the year.
        for budget in ["2000.00", "1999.99", "333.33", "1000.01"] {
            let amounts = amounts_for(&eur(budget), DEFAULT_SPLIT);
            let total: BigDecimal = amounts.iter().sum();
            assert_eq!(total, eur(budget), "budget {budget} did not add up");
        }
    }

    #[test]
    fn the_remainder_lands_on_the_last_round_not_the_first() {
        let amounts = amounts_for(&eur("1000.01"), DEFAULT_SPLIT);
        assert_eq!(amounts[0], eur("200.00"));
        // 1000.01 - 200.00 - 200.00 - 200.00
        assert_eq!(amounts[3], eur("400.01"));
    }

    #[test]
    fn a_split_that_is_not_four_rounds_still_works() {
        // Some jobs front-load the work. The default is a default, not a
        // rule, and the schema allows two to ten shares.
        let amounts = amounts_for(&eur("900.00"), &[50, 50]);
        assert_eq!(amounts, vec![eur("450.00"), eur("450.00")]);
    }
}
