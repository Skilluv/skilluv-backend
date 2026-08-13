//! Catching the payouts nobody told us about.
//!
//! Webhooks are the fast path and they are not reliable. A callback is lost,
//! an endpoint is down for an hour, an operator never sends one at all.
//! Anything built only on callbacks eventually holds a payout that is
//! `pending` forever: the recipient's balance is debited, the money is
//! somewhere, and no query in the system can say where.
//!
//! So this asks. Three outcomes, in order of preference:
//!
//! 1. **The provider answers.** Stripe and FedaPay can be polled, and the
//!    answer becomes the same [`Event`] a callback would have produced —
//!    routed through the same apply path, so a payout resolved by polling
//!    is indistinguishable from one resolved by a callback.
//! 2. **The provider cannot be polled.** The Mobile Money operators only
//!    ever push. Past a deadline, a human is asked instead.
//! 3. **The books and the provider disagree.** Reported, never
//!    auto-corrected: a discrepancy of real money is not something a
//!    background job should resolve on its own.
//!
//! Run from `main` on a timer, gated on an environment variable so a
//! deployment that is not the one holding the credentials does not poll.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::payment_webhooks::{self, Event};
use crate::services::payout::PayoutRegistry;

/// How long a payout may sit unconfirmed before the sweep starts asking.
///
/// Mobile Money normally settles in seconds and Stripe within a business
/// day, so an hour is well past "still in flight" without being so eager
/// that the sweep hammers a provider over a transfer that is simply slow.
const QUIET_PERIOD_MINUTES: i64 = 60;

/// When an unpollable payout stops being a delay and becomes an incident.
///
/// Three days covers a weekend, which is when an operator outage is least
/// likely to be noticed and most likely to happen.
const ESCALATE_AFTER_HOURS: i64 = 72;

/// How many times to ask before asking a person instead.
const MAX_CHECKS: i32 = 12;

/// What one sweep did.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SweepReport {
    pub checked: usize,
    pub settled: usize,
    pub failed: usize,
    /// Asked about and still pending. Normal, if it keeps decreasing.
    pub still_pending: usize,
    /// Handed to a human: too old, too often asked, or on a rail that
    /// cannot be asked at all.
    pub escalated: usize,
    /// Webhook events that were stored but failed to apply, retried here.
    pub replayed_events: usize,
}

/// Ask about every payout still unconfirmed, and retry what failed to apply.
pub async fn sweep(db: &PgPool, registry: &PayoutRegistry) -> Result<SweepReport, AppError> {
    let mut report = SweepReport {
        replayed_events: replay_unprocessed_events(db).await?,
        ..Default::default()
    };

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        provider: String,
        provider_reference: Option<String>,
        check_count: i32,
        age_hours: f64,
    }

    let stale: Vec<Row> = sqlx::query_as(
        "SELECT id, provider, provider_reference, check_count,
                EXTRACT(EPOCH FROM (NOW() - created_at)) / 3600 AS age_hours
           FROM payouts
          WHERE status = 'pending'
            AND created_at < NOW() - ($1 || ' minutes')::INTERVAL
            AND (last_checked_at IS NULL
                 OR last_checked_at < NOW() - ($1 || ' minutes')::INTERVAL)
          ORDER BY created_at
          LIMIT 200",
    )
    .bind(QUIET_PERIOD_MINUTES.to_string())
    .fetch_all(db)
    .await?;

    for row in stale {
        report.checked += 1;

        sqlx::query(
            "UPDATE payouts
                SET last_checked_at = NOW(), check_count = check_count + 1
              WHERE id = $1",
        )
        .bind(row.id)
        .execute(db)
        .await?;

        // No reference means the provider never answered, so there is
        // nothing to ask about. Straight to a human.
        let Some(reference) = row.provider_reference.as_deref() else {
            escalate(
                db,
                &row.provider,
                row.id,
                "the provider never returned a reference",
            )
            .await?;
            report.escalated += 1;
            continue;
        };

        let Some(provider) = registry.get(&row.provider) else {
            // The deployment running the sweep does not hold this
            // provider's credentials. Not an error — but not something to
            // count as checked either.
            tracing::debug!(
                provider = %row.provider,
                "sweep skipped a payout for an unconfigured provider"
            );
            continue;
        };

        match provider.status(reference).await {
            Ok(Some(state)) => {
                let event = payment_webhooks::state_to_event(state, reference);
                match &event {
                    Event::PayoutSettled { .. } => report.settled += 1,
                    Event::PayoutFailed { .. } => report.failed += 1,
                    Event::Ignored { .. } => report.still_pending += 1,
                }
                payment_webhooks::apply_event(db, &row.provider, &event).await?;
            }

            // This rail only ever pushes. Waiting is the only option, and
            // past the deadline waiting is no longer an option.
            Ok(None) => {
                if row.age_hours > ESCALATE_AFTER_HOURS as f64 {
                    escalate(
                        db,
                        &row.provider,
                        row.id,
                        "this rail cannot be polled and has sent no callback",
                    )
                    .await?;
                    report.escalated += 1;
                } else {
                    report.still_pending += 1;
                }
            }

            Err(e) => {
                tracing::warn!(
                    provider = %row.provider,
                    reference = %reference,
                    error = %e,
                    "could not read a payout's status from its provider"
                );
                if row.check_count + 1 >= MAX_CHECKS {
                    escalate(
                        db,
                        &row.provider,
                        row.id,
                        &format!("asked {MAX_CHECKS} times without a usable answer: {e}"),
                    )
                    .await?;
                    report.escalated += 1;
                } else {
                    report.still_pending += 1;
                }
            }
        }
    }

    if report.escalated > 0 || report.failed > 0 {
        tracing::warn!(
            checked = report.checked,
            settled = report.settled,
            failed = report.failed,
            escalated = report.escalated,
            "payout reconciliation finished with unresolved payouts"
        );
    }

    metrics::gauge!("skilluv_payouts_unreconciled").set(report.still_pending as f64);
    metrics::counter!("skilluv_payout_reconciliation_escalated_total")
        .increment(report.escalated as u64);

    Ok(report)
}

/// Retry the events that were stored but failed to apply.
///
/// An event that could not be applied when it arrived is left unprocessed
/// on purpose, precisely so this can pick it up. Without it, a transient
/// database error during a webhook would lose a settlement permanently.
async fn replay_unprocessed_events(db: &PgPool) -> Result<usize, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        provider: String,
        kind: Option<String>,
        payload: serde_json::Value,
    }

    let pending: Vec<Row> = sqlx::query_as(
        "SELECT id, provider, kind, payload
           FROM payment_webhook_events
          WHERE processed_at IS NULL
            AND signature_verified = TRUE
            AND kind IS NOT NULL
            AND received_at > NOW() - INTERVAL '30 days'
          ORDER BY received_at
          LIMIT 100",
    )
    .fetch_all(db)
    .await?;

    let mut replayed = 0;
    for row in pending {
        // Rebuilt from the stored `kind` and payload rather than
        // re-normalised: the source that understood it may no longer be
        // configured on this deployment, and the meaning was already
        // decided when it arrived.
        let Some(event) = event_from_stored(row.kind.as_deref(), &row.payload) else {
            continue;
        };

        match payment_webhooks::apply_event(db, &row.provider, &event).await {
            Ok(_) => {
                sqlx::query("UPDATE payment_webhook_events SET processed_at = NOW() WHERE id = $1")
                    .bind(row.id)
                    .execute(db)
                    .await?;
                replayed += 1;
            }
            Err(e) => {
                tracing::warn!(
                    event = %row.id,
                    provider = %row.provider,
                    error = %e,
                    "stored webhook event still cannot be applied"
                );
            }
        }
    }
    Ok(replayed)
}

/// Rebuild an event from what was stored about it.
fn event_from_stored(kind: Option<&str>, payload: &serde_json::Value) -> Option<Event> {
    let reference = payload
        .pointer("/entity/id")
        .or_else(|| payload.pointer("/data/entity/id"))
        .or_else(|| payload.pointer("/data/object/id"))
        .or_else(|| payload.get("transaction_id"))
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
        })?;

    match kind? {
        "payout.settled" => Some(Event::PayoutSettled { reference }),
        "payout.failed" => Some(Event::PayoutFailed {
            reference,
            reason: Some("replayed from a stored provider event".into()),
        }),
        _ => None,
    }
}

/// Hand a payout to a person, once.
///
/// The notification kind was seeded for exactly this and had no sender —
/// `admin.payout_needs_replay` existed in the catalogue and nothing ever
/// emitted it.
async fn escalate(db: &PgPool, provider: &str, payout_id: Uuid, why: &str) -> Result<(), AppError> {
    // Once per payout: the sweep runs on a timer, and a queue that
    // re-announces the same stuck payout every hour is a queue people mute.
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM notifications
              WHERE kind = 'admin.payout_needs_replay'
                AND payload->>'payout_id' = $1::text
         )",
    )
    .bind(payout_id)
    .fetch_one(db)
    .await
    .unwrap_or(false);
    if already {
        return Ok(());
    }

    tracing::error!(
        provider = provider,
        payout = %payout_id,
        reason = why,
        "payout unresolved — escalating to an operator"
    );

    crate::services::notify::send(
        crate::services::notify::Ctx::db_only(db),
        crate::services::notify::Recipient::Capability("admin"),
        "admin.payout_needs_replay",
    )
    .arg("count", "1")
    .payload(serde_json::json!({
        "payout_id": payout_id,
        "provider": provider,
        "reason": why,
    }))
    .execute()
    .await?;

    Ok(())
}

#[cfg(test)]
mod unit {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_stored_event_is_rebuilt_from_every_envelope_we_store() {
        let fedapay = json!({ "entity": { "id": 42 } });
        assert_eq!(
            event_from_stored(Some("payout.settled"), &fedapay),
            Some(Event::PayoutSettled {
                reference: "42".into()
            })
        );

        let stripe = json!({ "data": { "object": { "id": "po_1" } } });
        assert!(matches!(
            event_from_stored(Some("payout.failed"), &stripe),
            Some(Event::PayoutFailed { .. })
        ));

        let momo = json!({ "transaction_id": "t1" });
        assert_eq!(
            event_from_stored(Some("payout.settled"), &momo),
            Some(Event::PayoutSettled {
                reference: "t1".into()
            })
        );
    }

    #[test]
    fn an_event_we_never_understood_is_not_guessed_at() {
        let payload = json!({ "entity": { "id": 1 } });
        assert_eq!(event_from_stored(None, &payload), None);
        assert_eq!(event_from_stored(Some("ignored:whatever"), &payload), None);
        // No reference means nothing to act on.
        assert_eq!(event_from_stored(Some("payout.settled"), &json!({})), None);
    }
}
