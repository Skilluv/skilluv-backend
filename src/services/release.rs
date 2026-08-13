//! Holding money, and letting it go.
//!
//! A capture puts the recipient's share in `pending`. Something has to move
//! it to `available`, and that something is either the payer confirming, or
//! time passing. This module owns both, and the sweep that applies the
//! second.
//!
//! ## Why one sweep rather than a job per flow
//!
//! Every flow that holds money needs releasing. Written per flow, the one
//! that forgets leaves people unpaid, silently, and nobody finds out until
//! someone complains. `pending_releases` is a single queue: a new flow calls
//! [`hold`] and inherits the behaviour, including disputes and the sweep,
//! without writing a scheduled job of its own.
//!
//! ## Why the delay is in a table
//!
//! Seven days for a mentorship session, zero for a merged bounty. That is a
//! product decision about what can be contested, not a technical one, and it
//! changes without a deployment. See migration 0156 for the reasoning behind
//! each value.

use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::{self, Currency};

/// The rule for one kind of transaction.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Window {
    pub subject_type: String,
    pub hold_hours: i32,
    pub payer_can_release_early: bool,
    pub rationale: String,
}

/// The window for a subject type.
///
/// An unknown type is refused rather than defaulted. Guessing would mean
/// either paying out immediately on something that should have been held, or
/// holding money on something that should have been paid — both silent, both
/// discovered by the person who did not get their money.
pub async fn window_for(db: &PgPool, subject_type: &str) -> Result<Window, AppError> {
    sqlx::query_as(
        "SELECT subject_type, hold_hours, payer_can_release_early, rationale
           FROM release_windows WHERE subject_type = $1",
    )
    .bind(subject_type)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Internal(format!(
            "no release window defined for '{subject_type}' — add a row to \
             release_windows saying how long this kind of money is held and why"
        ))
    })
}

/// Everything needed to record a hold.
pub struct Hold<'a> {
    /// The capture that created the money being held.
    pub ledger_transaction_id: Uuid,
    pub beneficiary_id: Uuid,
    pub subject_type: &'a str,
    pub subject_id: Uuid,
    pub amount: &'a BigDecimal,
    pub currency: Currency,
    /// From `release_windows`. Passed in rather than looked up here so the
    /// caller can hold inside a transaction it already owns.
    pub hold_hours: i32,
}

/// Record a hold created by a capture, so the sweep can release it later.
///
/// Called inside the same transaction as the capture: a hold without its
/// ledger entries, or entries without a hold, would each be a way to lose
/// track of money.
pub async fn hold(
    tx: &mut Transaction<'_, Postgres>,
    params: Hold<'_>,
) -> Result<DateTime<Utc>, AppError> {
    let Hold {
        ledger_transaction_id,
        beneficiary_id,
        subject_type,
        subject_id,
        amount,
        currency,
        hold_hours,
    } = params;
    let release_at = Utc::now() + Duration::hours(hold_hours as i64);

    sqlx::query(
        r#"
        INSERT INTO pending_releases
            (ledger_transaction_id, beneficiary_id, subject_type, subject_id,
             amount, currency, release_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (subject_type, subject_id) DO NOTHING
        "#,
    )
    .bind(ledger_transaction_id)
    .bind(beneficiary_id)
    .bind(subject_type)
    .bind(subject_id)
    .bind(amount)
    .bind(currency.as_str())
    .bind(release_at)
    .execute(&mut **tx)
    .await?;

    Ok(release_at)
}

/// A hold, as the sweep and the dispute handlers see it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PendingRelease {
    pub id: Uuid,
    pub beneficiary_id: Uuid,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub amount: BigDecimal,
    pub currency: String,
    pub release_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub disputed_at: Option<DateTime<Utc>>,
}

async fn find_hold(
    db: &PgPool,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Option<PendingRelease>, AppError> {
    let row = sqlx::query_as(
        "SELECT id, beneficiary_id, subject_type, subject_id, amount, currency,
                release_at, released_at, disputed_at
           FROM pending_releases
          WHERE subject_type = $1 AND subject_id = $2",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// The payer confirmed early: release now instead of waiting.
///
/// Refused when the window forbids it — some holds exist for reasons the
/// payer cannot waive.
pub async fn release_early(
    db: &PgPool,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<bool, AppError> {
    let window = window_for(db, subject_type).await?;
    if !window.payer_can_release_early {
        return Err(AppError::Validation(format!(
            "funds for '{subject_type}' cannot be released early: {}",
            window.rationale
        )));
    }
    release_now(db, subject_type, subject_id).await
}

/// Move a hold to `available`. Returns `false` when there was nothing to do.
///
/// Idempotent through the ledger's own key, so a retry, a double click, or
/// the sweep racing an early confirmation all settle on one release.
pub async fn release_now(
    db: &PgPool,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<bool, AppError> {
    let Some(hold) = find_hold(db, subject_type, subject_id).await? else {
        return Ok(false);
    };
    if hold.released_at.is_some() {
        return Ok(false);
    }
    if hold.disputed_at.is_some() {
        return Err(AppError::Validation(
            "these funds are disputed — resolve the dispute before releasing".into(),
        ));
    }

    let currency: Currency = hold.currency.parse()?;
    ledger::release(
        db,
        hold.beneficiary_id,
        hold.amount.clone(),
        currency,
        subject_type,
        subject_id,
    )
    .await?;

    sqlx::query("UPDATE pending_releases SET released_at = NOW() WHERE id = $1")
        .bind(hold.id)
        .execute(db)
        .await?;

    metrics::counter!(
        "skilluv_funds_released_total",
        "subject_type" => subject_type.to_string()
    )
    .increment(1);

    Ok(true)
}

/// Freeze a hold while a complaint is examined.
///
/// Only possible before release. Afterwards the money is the recipient's to
/// withdraw, which is exactly what the window exists to prevent happening
/// too early.
pub async fn dispute(db: &PgPool, subject_type: &str, subject_id: Uuid) -> Result<bool, AppError> {
    let Some(hold) = find_hold(db, subject_type, subject_id).await? else {
        return Ok(false);
    };
    if hold.released_at.is_some() {
        return Err(AppError::Validation(
            "these funds were already released — a claw-back is a refund, \
             not a hold"
                .into(),
        ));
    }
    if hold.disputed_at.is_some() {
        return Ok(false);
    }

    let currency: Currency = hold.currency.parse()?;
    ledger::hold_dispute(
        db,
        hold.beneficiary_id,
        hold.amount.clone(),
        currency,
        subject_type,
        subject_id,
    )
    .await?;

    sqlx::query("UPDATE pending_releases SET disputed_at = NOW() WHERE id = $1")
        .bind(hold.id)
        .execute(db)
        .await?;

    Ok(true)
}

/// What one sweep did.
#[derive(Debug, Default, Serialize)]
pub struct SweepReport {
    pub examined: usize,
    pub released: usize,
    /// Holds that failed to release. Non-empty means someone is owed money
    /// the system meant to hand over and did not.
    pub failed: Vec<String>,
}

/// Release everything whose window has closed.
///
/// Run on a schedule. Failures are collected rather than aborting the sweep:
/// one recipient's problem must not stop everyone else from being paid.
pub async fn sweep(db: &PgPool) -> Result<SweepReport, AppError> {
    let due: Vec<PendingRelease> = sqlx::query_as(
        "SELECT id, beneficiary_id, subject_type, subject_id, amount, currency,
                release_at, released_at, disputed_at
           FROM pending_releases
          WHERE released_at IS NULL
            AND disputed_at IS NULL
            AND release_at <= NOW()
          ORDER BY release_at
          LIMIT 500",
    )
    .fetch_all(db)
    .await?;

    let mut report = SweepReport {
        examined: due.len(),
        ..Default::default()
    };

    for hold in due {
        match release_now(db, &hold.subject_type, hold.subject_id).await {
            Ok(true) => report.released += 1,
            Ok(false) => {}
            Err(e) => {
                report
                    .failed
                    .push(format!("{}:{}: {e}", hold.subject_type, hold.subject_id));
                tracing::error!(
                    subject_type = %hold.subject_type,
                    subject_id = %hold.subject_id,
                    beneficiary = %hold.beneficiary_id,
                    amount = %hold.amount,
                    error = %e,
                    "failed to release funds whose hold has expired — the \
                     beneficiary is owed money and cannot reach it"
                );
            }
        }
    }

    if !report.failed.is_empty() {
        metrics::counter!("skilluv_release_sweep_failures_total")
            .increment(report.failed.len() as u64);
    }
    if report.released > 0 {
        tracing::info!(
            released = report.released,
            examined = report.examined,
            "release sweep completed"
        );
    }

    Ok(report)
}

/// Holds that are late: due, undisputed, and still not released.
///
/// The queue a human works when the sweep has been failing. Empty is the
/// only acceptable state.
pub async fn overdue(db: &PgPool) -> Result<Vec<PendingRelease>, AppError> {
    let rows = sqlx::query_as(
        "SELECT id, beneficiary_id, subject_type, subject_id, amount, currency,
                release_at, released_at, disputed_at
           FROM pending_releases
          WHERE released_at IS NULL
            AND disputed_at IS NULL
            AND release_at <= NOW() - INTERVAL '1 hour'
          ORDER BY release_at",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}
