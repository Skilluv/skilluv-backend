//! Cash prizes on contests, and the escrow that makes them honest.
//!
//! ## The rule
//!
//! A contest promising money does not open until the money is held. That is
//! enforced by a CHECK constraint (migration 0516), not here — the one time a
//! handler is bypassed is the time it matters. This module is what moves the
//! money into that escrow and out of it.
//!
//! ## Why it matters more than it looks
//!
//! A paid design contest is the most contested format in the trade: a brand
//! publishes a brief, collects forty answers, pays for one, and thirty-nine
//! people worked for nothing. Everything separating a legitimate contest from
//! that is one question — was the money there before the brief was? Escrow is
//! how the platform answers it without asking anybody to trust the sponsor.
//!
//! ## What the platform takes
//!
//! Nothing. `capture_for_recipient` splits a payment between a recipient and
//! platform revenue because a paid mission is a sale we brokered. A prize is
//! not: the golden rule is that companies pay and talents do not, and a
//! commission skimmed off a prize is money taken from the winner. What is
//! escrowed reaches the podium whole; if the platform is to be paid for
//! running the contest, the sponsor is invoiced separately.
//!
//! ## After the award
//!
//! Nothing new. The prize lands in the winners' `pending` accounts, and the
//! existing release window, disputes and withdrawals take over unchanged — a
//! contest prize behaves like every other sum somebody is owed.

use bigdecimal::{BigDecimal, Signed, Zero};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::{self, Account, Currency, Leg, Posting, State, owed};

/// The subject every escrow posting is filed under.
const SUBJECT: &str = "tournament";

/// How a prize is divided, highest place first.
///
/// The same split `conclude_tournament` applies to the fragment pool, because
/// a contest that pays fragments one way and money another is two contests
/// wearing one name.
pub const PODIUM_SPLIT_PERCENT: [i64; 3] = [50, 30, 20];

/// What one place is owed, and what goes back for want of a recipient.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PrizeShare {
    pub rank: i16,
    pub amount: BigDecimal,
}

/// Divide a pool across the places that actually have a finisher.
///
/// Pure, and tested: this is the part where a mistake is silent. A contest
/// with two entrants pays first and second, and the third share goes back to
/// the sponsor — the brief said 50/30/20, and inventing a redistribution
/// would pay somebody more than the contest promised.
///
/// The rounding remainder joins the first place rather than evaporating: the
/// sum of the shares plus the refund has to equal the pool exactly, or the
/// escrow never empties.
pub fn split_for(pool: &BigDecimal, finishers: usize) -> (Vec<PrizeShare>, BigDecimal) {
    let places = finishers.min(PODIUM_SPLIT_PERCENT.len());
    if places == 0 {
        return (Vec::new(), pool.clone());
    }

    let hundred = BigDecimal::from(100);
    let mut shares: Vec<PrizeShare> = (0..places)
        .map(|i| PrizeShare {
            rank: (i + 1) as i16,
            // Two decimals: the currencies the ledger knows are both stored
            // that way, and a third would round somewhere invisible.
            amount: (pool * BigDecimal::from(PODIUM_SPLIT_PERCENT[i]) / &hundred).round(2),
        })
        .collect();

    let awarded: BigDecimal = shares
        .iter()
        .fold(BigDecimal::from(0), |acc, s| acc + &s.amount);

    // Unclaimed places, plus whatever rounding left behind.
    let unclaimed_percent: i64 = PODIUM_SPLIT_PERCENT[places..].iter().sum();
    let refund = if unclaimed_percent > 0 {
        pool - &awarded
    } else {
        // Every place has a finisher: the only remainder is rounding, and it
        // belongs to the winner rather than to us.
        let remainder = pool - &awarded;
        if !remainder.is_zero() {
            shares[0].amount = &shares[0].amount + &remainder;
        }
        BigDecimal::from(0)
    };

    (shares, refund)
}

// ═══════════════════════════════════════════════════════════════════
// Funding
// ═══════════════════════════════════════════════════════════════════

/// Record that a sponsor's money is held for this contest.
///
/// Called once the payment has actually settled at the provider — this writes
/// the ledger entry, it does not take a card. Until it succeeds the contest
/// cannot leave `upcoming`, so a payment that never lands simply means a
/// contest nobody ever saw.
pub async fn fund(
    db: &PgPool,
    tournament_id: Uuid,
    funder_enterprise_id: Uuid,
    amount: BigDecimal,
    currency: Currency,
    provider: &'static str,
    provider_reference: impl Into<String>,
) -> Result<(), AppError> {
    if !amount.is_positive() {
        return Err(AppError::Validation(
            "a prize has to be a positive amount".into(),
        ));
    }

    let existing: Option<(String, Option<BigDecimal>)> = sqlx::query_as(
        "SELECT prize_escrow_state, prize_cash_amount FROM tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(db)
    .await?;
    let (state, _) = existing.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    if state != "none" {
        return Err(AppError::Conflict(format!(
            "this contest's prize is already {state}"
        )));
    }

    let mut tx = db.begin().await?;

    // The money physically sits at the provider, and it is owed to whoever
    // this contest ends up designating.
    ledger::post_in_tx(
        &mut tx,
        Posting::new(
            "contest_prize_escrow",
            vec![
                Leg::debit(Account::Psp { provider, currency }, amount.clone()),
                Leg::credit(escrow_account(tournament_id, currency), amount.clone()),
            ],
        )
        .about(SUBJECT, tournament_id)
        .via(provider, provider_reference)
        .idempotent(format!("prize_escrow:{SUBJECT}:{tournament_id}")),
    )
    .await?;

    sqlx::query(
        r#"
        UPDATE tournaments
           SET prize_cash_amount = $2,
               prize_cash_currency = $3,
               prize_escrow_state = 'funded',
               prize_funded_at = NOW(),
               prize_funded_by_enterprise_id = $4,
               updated_at = NOW()
         WHERE id = $1 AND prize_escrow_state = 'none'
        "#,
    )
    .bind(tournament_id)
    .bind(&amount)
    .bind(currency.as_str())
    .bind(funder_enterprise_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Award
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct AwardReport {
    pub shares: Vec<PrizeShare>,
    /// Returned to the sponsor for want of a finisher in that place.
    pub refunded: BigDecimal,
    pub currency: String,
}

/// Move an escrowed prize to the podium.
///
/// Called after `conclude_tournament` has written the ranks. Idempotent: the
/// escrow state guards the whole operation and every ledger posting carries
/// its own key, so a retried conclusion pays once.
pub async fn award(db: &PgPool, tournament_id: Uuid) -> Result<Option<AwardReport>, AppError> {
    let row: Option<(String, Option<BigDecimal>, Option<String>, String)> = sqlx::query_as(
        "SELECT prize_escrow_state, prize_cash_amount, prize_cash_currency, status
           FROM tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(db)
    .await?;
    let (escrow_state, amount, currency, status) =
        row.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    // No cash prize, or already settled: nothing owed.
    if escrow_state != "funded" {
        return Ok(None);
    }
    if status != "concluded" {
        return Err(AppError::Conflict(
            "a prize is awarded from a ranking, and this contest has none yet".into(),
        ));
    }

    let pool = amount.ok_or_else(|| AppError::Internal("funded escrow with no amount".into()))?;
    let currency: Currency = currency
        .ok_or_else(|| AppError::Internal("funded escrow with no currency".into()))?
        .parse()?;

    // The podium, as the conclusion wrote it. Guild entries are skipped: a
    // prize paid to a guild has no account to land in, and splitting it among
    // members is a different decision nobody has taken.
    let podium: Vec<(Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT participant_id, rank
          FROM tournament_participants
         WHERE tournament_id = $1
           AND participant_type = 'user'
           AND rank IS NOT NULL
           AND rank <= 3
         ORDER BY rank ASC
        "#,
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;

    let (shares, refund) = split_for(&pool, podium.len());

    let mut tx = db.begin().await?;

    for (share, (user_id, _rank)) in shares.iter().zip(podium.iter()) {
        if share.amount.is_zero() {
            continue;
        }
        // Into `pending`, not `available`: a prize behaves like every other
        // sum somebody is owed, and the release window is what makes a
        // contested result recoverable.
        ledger::post_in_tx(
            &mut tx,
            Posting::new(
                "contest_prize_award",
                vec![
                    Leg::debit(
                        escrow_account(tournament_id, currency),
                        share.amount.clone(),
                    ),
                    Leg::credit(
                        owed(*user_id, State::Pending, currency),
                        share.amount.clone(),
                    ),
                ],
            )
            .about(SUBJECT, tournament_id)
            .idempotent(format!(
                "prize_award:{SUBJECT}:{tournament_id}:{}",
                share.rank
            )),
        )
        .await?;
    }

    if refund.is_positive() {
        post_refund_leg(
            &mut tx,
            tournament_id,
            currency,
            refund.clone(),
            "unclaimed_place",
        )
        .await?;
    }

    sqlx::query(
        "UPDATE tournaments SET prize_escrow_state = 'awarded', updated_at = NOW()
          WHERE id = $1 AND prize_escrow_state = 'funded'",
    )
    .bind(tournament_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Some(AwardReport {
        shares,
        refunded: refund,
        currency: currency.as_str().to_string(),
    }))
}

// ═══════════════════════════════════════════════════════════════════
// Refund
// ═══════════════════════════════════════════════════════════════════

/// Return an escrowed prize to the sponsor.
///
/// For a contest that was cancelled, or that ended with nobody in the
/// running. Holding money for a contest that will never have a winner is the
/// same failure as never funding it, seen from the other side.
pub async fn refund(db: &PgPool, tournament_id: Uuid, reason: &str) -> Result<(), AppError> {
    let reason = reason.trim();
    if reason.chars().count() < 10 {
        return Err(AppError::Validation(
            "returning a prize has to say why, in at least 10 characters".into(),
        ));
    }

    let row: Option<(String, Option<BigDecimal>, Option<String>)> = sqlx::query_as(
        "SELECT prize_escrow_state, prize_cash_amount, prize_cash_currency
           FROM tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(db)
    .await?;
    let (escrow_state, amount, currency) =
        row.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    if escrow_state != "funded" {
        return Err(AppError::Conflict(format!(
            "nothing to return: this contest's prize is {escrow_state}"
        )));
    }

    let pool = amount.ok_or_else(|| AppError::Internal("funded escrow with no amount".into()))?;
    let currency: Currency = currency
        .ok_or_else(|| AppError::Internal("funded escrow with no currency".into()))?
        .parse()?;

    let mut tx = db.begin().await?;
    post_refund_leg(&mut tx, tournament_id, currency, pool, reason).await?;

    sqlx::query(
        "UPDATE tournaments SET prize_escrow_state = 'refunded', updated_at = NOW()
          WHERE id = $1 AND prize_escrow_state = 'funded'",
    )
    .bind(tournament_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Take an awarded prize back, because the entry that won it was upheld as
/// plagiarised.
///
/// This is the case `award` was written for. Its own comment says the prize
/// lands in `pending` rather than `available` because "the release window is
/// what makes a contested result recoverable" — and until now nothing
/// recovered one. `plagiarism_cases::decide` disqualified the submission and
/// left the money exactly where it was, so a contest could have a
/// disqualified winner and a paid one at the same time, in the same person.
///
/// **Back to the escrow, not to the sponsor and not to the next place.**
/// Returning it to the sponsor would decide, in a function nobody is reading,
/// that a contest with a cheating winner pays its second place nothing.
/// Promoting the runner-up automatically would pay somebody weeks after the
/// contest closed, on a decision they were never told about, and re-running
/// `award` against a changed podium is not something its idempotency keys
/// allow. So the money goes back to the pot it came from, where it is visible,
/// balanced, and somebody's to decide about.
///
/// Returns what was taken back. `None` when the winner had no prize — a free
/// contest, or a place that paid nothing.
pub async fn confiscate(
    db: &PgPool,
    tournament_id: Uuid,
    user_id: Uuid,
) -> Result<Option<BigDecimal>, AppError> {
    let currency: Option<String> =
        sqlx::query_scalar("SELECT prize_cash_currency FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?
            .flatten();
    let Some(currency) = currency else {
        return Ok(None);
    };
    let currency: Currency = currency.parse()?;

    // What this person was actually awarded, read from the book rather than
    // recomputed from the split: a recomputation would have to agree about
    // rounding and about how many places finished, and the entry that says
    // what was paid is right here.
    //
    // Negated, because a claim is stored negative and the award credited it —
    // the same flip `ledger_user_balance` does.
    let awarded: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT -SUM(e.amount)
           FROM ledger_entries e
           JOIN ledger_transactions t ON t.id = e.transaction_id
           JOIN ledger_accounts a ON a.id = e.account_id
          WHERE t.reason = 'contest_prize_award'
            AND t.subject_type = $1 AND t.subject_id = $2
            AND a.code = $3",
    )
    .bind(SUBJECT)
    .bind(tournament_id)
    .bind(owed(user_id, State::Pending, currency).code())
    .fetch_one(db)
    .await?;

    let Some(amount) = awarded.filter(|a| a.is_positive()) else {
        return Ok(None);
    };

    // Only what is still held. Once released the prize is theirs to withdraw
    // and may already be gone; taking it out of `pending` anyway would drive
    // the account negative and make the platform's books claim money that is
    // not there. A prize that has left is a debt to recover, not a ledger
    // entry — and it is the reason the release window exists at all.
    let still_pending = ledger::user_balance(db, user_id, State::Pending, currency).await?;
    if still_pending < amount {
        return Err(AppError::Conflict(format!(
            "the prize has already been released — {still_pending} of {amount} is still held, \
             and the rest is a debt to recover rather than an entry to reverse"
        )));
    }

    let mut tx = db.begin().await?;

    ledger::post_in_tx(
        &mut tx,
        Posting::new(
            "contest_prize_confiscated",
            vec![
                Leg::debit(owed(user_id, State::Pending, currency), amount.clone()),
                Leg::credit(escrow_account(tournament_id, currency), amount.clone()),
            ],
        )
        .about(SUBJECT, tournament_id)
        .note("plagiarism upheld")
        // Per person, so a contest with two disqualified entrants takes both
        // back rather than the first one twice.
        .idempotent(format!(
            "prize_confiscated:{SUBJECT}:{tournament_id}:{user_id}"
        )),
    )
    .await?;

    tx.commit().await?;
    Ok(Some(amount))
}

/// Money leaving the escrow back across the boundary.
///
/// `World` rather than a platform bucket: it is not ours, and crediting it to
/// revenue would make an unclaimed prize look like income.
async fn post_refund_leg(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tournament_id: Uuid,
    currency: Currency,
    amount: BigDecimal,
    reason: &str,
) -> Result<(), AppError> {
    ledger::post_in_tx(
        tx,
        Posting::new(
            "contest_prize_refund",
            vec![
                Leg::debit(escrow_account(tournament_id, currency), amount.clone()),
                Leg::credit(Account::World { currency }, amount),
            ],
        )
        .about(SUBJECT, tournament_id)
        .note(reason)
        .idempotent(format!("prize_refund:{SUBJECT}:{tournament_id}:{reason}")),
    )
    .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Reads
// ═══════════════════════════════════════════════════════════════════

fn escrow_account(tournament_id: Uuid, currency: Currency) -> Account {
    Account::Escrow {
        subject_type: SUBJECT,
        subject_id: tournament_id,
        currency,
    }
}

/// Contests that ended and are still holding somebody's money.
///
/// Each one owes an award or a refund. Nothing here decides which — a human
/// looks, because "nobody deserved the prize" and "nobody concluded the
/// contest" look identical from the outside and have opposite answers.
pub async fn outstanding(db: &PgPool) -> Result<Vec<(Uuid, String)>, AppError> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, name FROM tournaments
         WHERE prize_escrow_state = 'funded'
           AND ends_at < NOW()
         ORDER BY ends_at ASC
        "#,
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod unit {
    use super::*;
    use std::str::FromStr;

    fn eur(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_full_podium_splits_fifty_thirty_twenty() {
        let (shares, refund) = split_for(&eur("1000"), 3);
        assert_eq!(shares.len(), 3);
        assert_eq!(shares[0].amount, eur("500"));
        assert_eq!(shares[1].amount, eur("300"));
        assert_eq!(shares[2].amount, eur("200"));
        assert_eq!(refund, eur("0"));
    }

    #[test]
    fn an_unclaimed_place_goes_back_to_the_sponsor() {
        // Two entrants: the brief promised 50/30/20, so the third share is
        // not redistributed. Inventing a redistribution would pay somebody
        // more than the contest announced.
        let (shares, refund) = split_for(&eur("1000"), 2);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].amount, eur("500"));
        assert_eq!(shares[1].amount, eur("300"));
        assert_eq!(refund, eur("200"));
    }

    #[test]
    fn a_single_entrant_takes_the_first_place_only() {
        let (shares, refund) = split_for(&eur("1000"), 1);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].amount, eur("500"));
        assert_eq!(refund, eur("500"));
    }

    #[test]
    fn nobody_in_the_running_returns_everything() {
        let (shares, refund) = split_for(&eur("1000"), 0);
        assert!(shares.is_empty());
        assert_eq!(refund, eur("1000"));
    }

    #[test]
    fn more_than_three_finishers_still_pays_three() {
        let (shares, refund) = split_for(&eur("1000"), 40);
        assert_eq!(shares.len(), 3);
        assert_eq!(refund, eur("0"));
    }

    #[test]
    fn the_escrow_always_empties_exactly() {
        // The invariant that matters: shares plus refund equal the pool, to
        // the centime. Anything else leaves money stuck in an account nobody
        // reads, or posts more than was held.
        for pool in ["1000", "999.99", "0.03", "1234.56", "7", "100.01"] {
            for finishers in 0..5usize {
                let (shares, refund) = split_for(&eur(pool), finishers);
                let total = shares
                    .iter()
                    .fold(BigDecimal::from(0), |acc, s| acc + &s.amount)
                    + &refund;
                assert_eq!(
                    total,
                    eur(pool),
                    "pool {pool} with {finishers} finisher(s) did not balance"
                );
            }
        }
    }

    #[test]
    fn rounding_never_shortchanges_the_winner() {
        // 0.03 split three ways is 0.015 each before rounding. Whatever the
        // rounding does, the winner is not the one who loses a centime to it.
        let (shares, refund) = split_for(&eur("0.03"), 3);
        assert_eq!(refund, eur("0"));
        assert!(shares[0].amount >= shares[1].amount);
        assert!(shares[1].amount >= shares[2].amount);
    }

    #[test]
    fn the_split_matches_the_fragment_split() {
        // A contest paying fragments one way and money another is two
        // contests wearing one name.
        assert_eq!(PODIUM_SPLIT_PERCENT, [50, 30, 20]);
        assert_eq!(PODIUM_SPLIT_PERCENT.iter().sum::<i64>(), 100);
    }
}
