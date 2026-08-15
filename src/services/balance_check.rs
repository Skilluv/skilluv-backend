//! Comparing our books to the provider's, which nothing was doing.
//!
//! Individual payouts are reconciled: `services::reconciliation` chases any
//! transfer that never confirmed. Individual payments are reconciled: the
//! poller chases any charge that never landed. Nothing compares the totals.
//!
//! That gap matters because the failures the two sweeps catch are the loud
//! ones — a specific transfer, a specific charge. The failure they cannot
//! see is systematic: a fee we never booked, a currency conversion applied
//! twice, a refund the provider took that our books show as still ours.
//! Each of those leaves every individual record looking correct while the
//! totals drift apart, and drift is only visible against a total.
//!
//! ## What it does not do
//!
//! It never corrects anything. A discrepancy in real money is not something
//! a background job should resolve on its own: the right response to "we
//! think we hold 4.2m XOF at FedaPay and FedaPay says 4.1m" is a person
//! looking, not code guessing which number is right and writing entries to
//! make them agree.
//!
//! ## Tolerance
//!
//! Not zero. A payout accepted a second ago is ours and not yet theirs, and
//! a comparison taken across that instant is off by exactly one payout
//! without anything being wrong. The threshold is a share of the position
//! rather than a fixed sum, because a hundred euros of drift on a float of
//! two hundred is an emergency and on a float of two million is timing.

use bigdecimal::BigDecimal;
use sqlx::PgPool;

use crate::errors::AppError;

/// Drift beyond this share of the position is reported.
///
/// One percent. In-flight movements are a small fraction of a healthy
/// float; anything larger is not timing.
const TOLERANCE_PERCENT: i64 = 1;

/// Below this, percentages are meaningless and everything looks like drift.
const IGNORE_BELOW: i64 = 1_000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Position {
    pub provider: String,
    pub currency: String,
    /// What our ledger says is held there.
    pub ours: String,
    /// What the provider says. `None` when they cannot be asked — which is
    /// itself worth reporting, because a position nobody can verify is one
    /// nobody is verifying.
    pub theirs: Option<String>,
    pub drift: Option<String>,
    pub within_tolerance: bool,
}

/// An account whose snapshot and whose entries disagree.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SnapshotDrift {
    pub account_code: String,
    pub snapshot: String,
    pub recomputed: String,
    pub drift: String,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct CheckReport {
    pub positions: Vec<Position>,
    /// Accounts where the running total no longer matches its own entries.
    /// Must always be empty: it means the arithmetic the whole ledger rests
    /// on has gone wrong, which is more serious than any provider drift.
    pub snapshot_drift: Vec<SnapshotDrift>,
    /// Positions outside tolerance. Non-zero means a person should look
    /// today rather than at month end.
    pub drifting: usize,
    /// Positions we could not verify at all.
    pub unverifiable: usize,
}

/// Compare every provider position we hold against the provider.
pub async fn check(db: &PgPool) -> Result<CheckReport, AppError> {
    let ours = crate::services::ledger::provider_positions(db).await?;
    let mut report = CheckReport::default();

    // Our own arithmetic first. Comparing a snapshot against a provider is
    // meaningless if the snapshot has drifted from the entries underneath
    // it, and that failure is the more serious of the two: a provider
    // disagreeing is a reconciliation problem, our own totals disagreeing
    // is every balance in the system being wrong.
    let internal: Vec<SnapshotDrift> = sqlx::query_as(
        "SELECT account_code, snapshot::TEXT AS snapshot,
                recomputed::TEXT AS recomputed, drift::TEXT AS drift
           FROM ledger_verify_balances()",
    )
    .fetch_all(db)
    .await?;

    if !internal.is_empty() {
        metrics::counter!("skilluv_ledger_snapshot_drift_total").increment(internal.len() as u64);
        tracing::error!(
            accounts = internal.len(),
            details = ?internal,
            "a ledger snapshot disagrees with its own entries — every balance              derived from it is suspect"
        );
    }
    report.snapshot_drift = internal;

    for position in ours {
        // `psp:stripe:settlement:EUR` — the provider is the middle segment.
        let provider = position
            .account_code
            .split(':')
            .nth(1)
            .unwrap_or_default()
            .to_string();

        let theirs = remote_balance(&provider, &position.currency).await;

        let (drift, within) = match &theirs {
            Some(theirs) => {
                let drift = &position.balance - theirs;
                let magnitude = drift.abs();
                let allowed = (&position.balance * BigDecimal::from(TOLERANCE_PERCENT)
                    / BigDecimal::from(100))
                .abs();
                let small = position.balance < IGNORE_BELOW;
                (Some(drift), small || magnitude <= allowed)
            }
            // Not "fine". A position nobody can verify is one nobody is
            // verifying, and it is counted separately for that reason.
            None => (None, false),
        };

        if theirs.is_none() {
            report.unverifiable += 1;
        } else if !within {
            report.drifting += 1;
            metrics::counter!(
                "skilluv_ledger_drift_detected_total",
                "provider" => provider.clone(),
                "currency" => position.currency.clone()
            )
            .increment(1);
            tracing::error!(
                provider = %provider,
                currency = %position.currency,
                ours = %position.balance,
                theirs = ?theirs.as_ref().map(ToString::to_string),
                "our books and the provider disagree about how much we hold"
            );
        }

        report.positions.push(Position {
            provider,
            currency: position.currency.clone(),
            ours: position.balance.to_string(),
            theirs: theirs.as_ref().map(ToString::to_string),
            drift: drift.as_ref().map(ToString::to_string),
            within_tolerance: within,
        });
    }

    Ok(report)
}

/// What the provider says it is holding for us.
///
/// `None` where they cannot be asked. FedaPay publishes no balance
/// endpoint, so its position is unverifiable from here and says so, rather
/// than being quietly treated as agreeing.
async fn remote_balance(provider: &str, currency: &str) -> Option<BigDecimal> {
    match provider {
        "stripe" => {
            let cfg = crate::services::stripe::StripeConfig::from_env()?;
            crate::services::stripe::available_balance(&cfg, currency)
                .await
                .ok()
                .flatten()
        }
        _ => None,
    }
}

/// Run it daily, and say something only when there is something to say.
///
/// Daily rather than hourly: drift is a slow failure, and the value of
/// finding it an hour sooner does not justify a request per hour per
/// provider forever.
pub fn start_balance_check(db: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(24 * 3600));
        // The first tick fires immediately, which is useful: a deployment
        // that starts with drift should say so at boot rather than
        // tomorrow.
        loop {
            ticker.tick().await;
            match check(&db).await {
                Ok(report) => {
                    if !report.snapshot_drift.is_empty() {
                        // Louder than provider drift, and told separately:
                        // this is our own arithmetic disagreeing with
                        // itself, which no reconciliation can explain.
                        let _ = crate::services::notify::send(
                            crate::services::notify::Ctx::db_only(&db),
                            crate::services::notify::Recipient::Capability("admin"),
                            "admin.reconciliation_drift",
                        )
                        .arg("provider", "the ledger itself")
                        .arg(
                            "amount",
                            format!("{} account(s)", report.snapshot_drift.len()),
                        )
                        .payload(serde_json::json!({ "accounts": report.snapshot_drift }))
                        .execute()
                        .await;
                    }
                    if report.drifting > 0 {
                        // The kind was seeded for this and had no sender.
                        let _ = crate::services::notify::send(
                            crate::services::notify::Ctx::db_only(&db),
                            crate::services::notify::Recipient::Capability("admin"),
                            "admin.reconciliation_drift",
                        )
                        .arg("provider", "one or more providers")
                        .arg("amount", format!("{} position(s)", report.drifting))
                        .payload(serde_json::json!({ "positions": report.positions }))
                        .execute()
                        .await;
                    }
                    if report.unverifiable > 0 {
                        tracing::warn!(
                            count = report.unverifiable,
                            "positions that cannot be checked against the provider — \
                             these are verified by hand or not at all"
                        );
                    }
                }
                Err(e) => tracing::error!(error = %e, "balance check failed"),
            }
        }
    });
}

#[cfg(test)]
mod unit {
    use super::*;
    use std::str::FromStr;

    /// The tolerance rule, as the loop applies it.
    fn within(ours: &str, theirs: &str) -> bool {
        let ours = BigDecimal::from_str(ours).unwrap();
        let theirs = BigDecimal::from_str(theirs).unwrap();
        let magnitude = (&ours - &theirs).abs();
        let allowed = (&ours * BigDecimal::from(TOLERANCE_PERCENT)) / BigDecimal::from(100);
        ours < IGNORE_BELOW || magnitude <= allowed
    }

    #[test]
    fn a_payout_in_flight_is_not_an_incident() {
        // Five thousand francs mid-transfer against a four-million float is
        // timing, not a problem, and paging someone for it teaches them to
        // ignore the page.
        assert!(within("4000000", "3995000"));
    }

    #[test]
    fn real_drift_is_not_absorbed_by_the_tolerance() {
        // Two percent of the float is not timing.
        assert!(!within("4000000", "3920000"));
    }

    #[test]
    fn a_small_float_is_not_measured_in_percentages() {
        // On five hundred francs, one percent is five — every rounding
        // looks like drift, and the alarm is useless.
        assert!(within("500", "450"));
    }

    #[test]
    fn drift_in_either_direction_counts() {
        // Holding more than the provider says is as wrong as holding less,
        // and is usually the more interesting of the two.
        assert!(!within("4000000", "4200000"));
    }
}
