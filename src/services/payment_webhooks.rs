//! One way in for what payment providers tell us.
//!
//! Every provider announces the same handful of facts — the payout arrived,
//! the payout failed, the money came in, the customer disputed it — in its
//! own vocabulary, its own envelope and its own signature scheme. Handling
//! that per provider is how a codebase ends up with four webhook routes
//! that each update the ledger slightly differently.
//!
//! So the vocabulary is normalised at the edge:
//!
//! * [`Source`] — what a provider must be able to do: prove the request is
//!   theirs, and say what it means in our words.
//! * [`Event`] — our words. Four facts, no provider vocabulary.
//! * [`receive`] — the one entry point. Verifies, stores, applies, and is
//!   safe to call with the same event any number of times.
//!
//! ## Store before applying
//!
//! The raw body is written to `payment_webhook_events` before anything acts
//! on it, and it is never edited afterwards. When our books and a
//! provider's statement disagree — which is a matter of when, not if — the
//! argument is settled by what they actually sent, not by our reading of
//! it. It is also what makes redelivery free: the unique key rejects the
//! second copy before any money moves.
//!
//! ## A bad signature is stored, never applied
//!
//! Dropping an unsigned event silently hides an attempt; applying one is
//! the attempt succeeding. It is recorded with `signature_verified` false,
//! answered with 401, and left for a human to look at.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::{self, Currency};
use crate::services::payout::PayoutState;

/// What a provider told us, in our vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A payout we sent reached the recipient.
    PayoutSettled { reference: String },
    /// A payout we sent did not arrive, and will not.
    PayoutFailed {
        reference: String,
        reason: Option<String>,
    },
    /// Something happened that we understood, and that requires nothing.
    /// Recorded, acknowledged, not acted on.
    Ignored { kind: String },
}

impl Event {
    /// The label stored on the event row, for reading the log later.
    pub fn kind(&self) -> String {
        match self {
            Event::PayoutSettled { .. } => "payout.settled".into(),
            Event::PayoutFailed { .. } => "payout.failed".into(),
            Event::Ignored { kind } => format!("ignored:{kind}"),
        }
    }
}

/// A provider that sends us webhooks.
///
/// Two responsibilities and no more: prove the request came from them, and
/// translate it. Nothing here touches the ledger — the point of the split
/// is that a new provider cannot invent its own way of moving money.
pub trait Source: Send + Sync {
    /// Must match `PayoutProvider::name()` where both exist, since that is
    /// how an event finds the payout it is about.
    fn name(&self) -> &'static str;

    /// Whether this body really came from the provider.
    ///
    /// `signature` is whichever header the provider signs with, already
    /// extracted by the route. Returning `Ok(())` on an absent secret is
    /// forbidden — a deployment with no secret configured must reject, not
    /// wave events through.
    fn verify(&self, body: &str, signature: Option<&str>) -> Result<(), AppError>;

    /// The provider's own id for this delivery, used to deduplicate.
    ///
    /// `None` where the provider sends none; the caller then hashes the
    /// body, which deduplicates identical redeliveries and nothing else.
    fn event_id(&self, payload: &Value) -> Option<String>;

    /// What it means, in our vocabulary.
    fn normalise(&self, payload: &Value) -> Result<Event, AppError>;
}

/// What [`receive`] did, so the route can answer honestly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Understood and acted on.
    Applied(String),
    /// Seen before. Stored once, applied once, acknowledged every time.
    Duplicate,
    /// Understood, nothing to do.
    Ignored,
}

/// Take one webhook delivery, from raw body to settled books.
///
/// Idempotent by construction: the unique key on
/// `(provider, provider_event_id)` means the second delivery of the same
/// event never reaches the ledger.
pub async fn receive(
    db: &PgPool,
    source: &dyn Source,
    body: &str,
    signature: Option<&str>,
) -> Result<Outcome, AppError> {
    let payload: Value = serde_json::from_str(body)
        .map_err(|e| AppError::Validation(format!("webhook body is not JSON: {e}")))?;

    let event_id = source
        .event_id(&payload)
        .unwrap_or_else(|| fingerprint(body));

    // Verification failure is recorded before it is refused, so an attempt
    // leaves a trace rather than vanishing into a 401.
    if let Err(e) = source.verify(body, signature) {
        store(db, source.name(), &event_id, None, &payload, false).await?;
        metrics::counter!(
            "skilluv_payment_webhook_rejected_total",
            "provider" => source.name().to_string()
        )
        .increment(1);
        tracing::warn!(
            provider = source.name(),
            event_id = %event_id,
            error = %e,
            "payment webhook failed signature verification — stored, not applied"
        );
        return Err(AppError::Unauthorized);
    }

    // Normalisation failing is not a reason to lose the event: an envelope
    // we cannot read today is exactly what we will want to read when
    // something is wrong.
    let event = match source.normalise(&payload) {
        Ok(event) => event,
        Err(e) => {
            store(db, source.name(), &event_id, None, &payload, true).await?;
            tracing::warn!(
                provider = source.name(),
                event_id = %event_id,
                error = %e,
                "payment webhook could not be interpreted — stored unprocessed"
            );
            return Ok(Outcome::Ignored);
        }
    };

    let stored_id = store(
        db,
        source.name(),
        &event_id,
        Some(&event.kind()),
        &payload,
        true,
    )
    .await?;
    let Some(stored_id) = stored_id else {
        // Already seen. The provider gets its 200 and nothing moves.
        metrics::counter!(
            "skilluv_payment_webhook_duplicate_total",
            "provider" => source.name().to_string()
        )
        .increment(1);
        return Ok(Outcome::Duplicate);
    };

    let outcome = apply(db, source.name(), &event).await;

    match &outcome {
        Ok(_) => {
            sqlx::query("UPDATE payment_webhook_events SET processed_at = NOW() WHERE id = $1")
                .bind(stored_id)
                .execute(db)
                .await?;
        }
        Err(e) => {
            // Left unprocessed on purpose: the sweep retries it, and an
            // event that silently failed to apply is the exact hole this
            // module exists to close.
            sqlx::query("UPDATE payment_webhook_events SET processing_error = $2 WHERE id = $1")
                .bind(stored_id)
                .bind(e.to_string())
                .execute(db)
                .await?;
        }
    }

    outcome
}

/// Move the books to match what the provider said.
///
/// Only two facts move money, and both are guarded by the payout's current
/// state: a payout already settled is not settled twice, and one already
/// failed is not reversed twice.
async fn apply(db: &PgPool, provider: &str, event: &Event) -> Result<Outcome, AppError> {
    match event {
        Event::Ignored { .. } => Ok(Outcome::Ignored),

        Event::PayoutSettled { reference } => {
            let settled = sqlx::query(
                "UPDATE payouts
                    SET status = 'sent', settled_at = NOW()
                  WHERE provider = $1 AND provider_reference = $2
                    AND status = 'pending'",
            )
            .bind(provider)
            .bind(reference)
            .execute(db)
            .await?;

            if settled.rows_affected() == 0 {
                // Either already settled, or about a payout we never
                // recorded. The second is a real problem — money left the
                // provider that our books know nothing about — so it is
                // said out loud rather than swallowed.
                warn_unknown(db, provider, reference, "settled").await;
                return Ok(Outcome::Ignored);
            }

            metrics::counter!(
                "skilluv_payout_settled_total",
                "provider" => provider.to_string()
            )
            .increment(1);
            Ok(Outcome::Applied("payout.settled".into()))
        }

        Event::PayoutFailed { reference, reason } => {
            let payout = find_pending(db, provider, reference).await?;
            let Some(payout) = payout else {
                warn_unknown(db, provider, reference, "failed").await;
                return Ok(Outcome::Ignored);
            };

            // A provider name that is not one we know cannot be turned into
            // a ledger account. Refused rather than guessed at: crediting
            // the wrong provider account is a reconciliation that never
            // balances again.
            let ledger_provider = crate::services::payout::canonical_provider(provider)
                .ok_or_else(|| {
                    AppError::Validation(format!(
                        "cannot reverse a payout for unknown provider {provider}"
                    ))
                })?;

            // The money never arrived, so it goes back to where it was: the
            // recipient's available balance. This is the whole reason the
            // module exists — before it, a failed Mobile Money payout left
            // the balance debited and the money nowhere.
            ledger::reverse_withdrawal(
                db,
                payout.user_id,
                payout.amount.clone(),
                payout.currency,
                ledger_provider,
                &payout.idempotency_key,
            )
            .await?;

            sqlx::query(
                "UPDATE payouts
                    SET status = 'failed', settled_at = NOW(), failure_reason = $2
                  WHERE id = $1",
            )
            .bind(payout.id)
            .bind(
                reason
                    .clone()
                    .unwrap_or_else(|| "the provider reported a failure without a reason".into()),
            )
            .execute(db)
            .await?;

            metrics::counter!(
                "skilluv_payout_failed_total",
                "provider" => provider.to_string()
            )
            .increment(1);
            tracing::error!(
                provider = provider,
                reference = %reference,
                user = %payout.user_id,
                amount = %payout.amount,
                "payout failed at the provider — funds returned to the recipient"
            );
            Ok(Outcome::Applied("payout.failed".into()))
        }
    }
}

/// A pending payout, enough of it to reverse.
pub struct PendingPayout {
    pub id: Uuid,
    pub user_id: Uuid,
    pub amount: bigdecimal::BigDecimal,
    pub currency: Currency,
    pub idempotency_key: String,
}

pub(crate) async fn find_pending(
    db: &PgPool,
    provider: &str,
    reference: &str,
) -> Result<Option<PendingPayout>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        user_id: Uuid,
        amount: bigdecimal::BigDecimal,
        currency: String,
        idempotency_key: Option<String>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT id, user_id, amount, currency, idempotency_key
           FROM payouts
          WHERE provider = $1 AND provider_reference = $2 AND status = 'pending'",
    )
    .bind(provider)
    .bind(reference)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| PendingPayout {
        id: r.id,
        user_id: r.user_id,
        amount: r.amount,
        currency: if r.currency == "XOF" {
            Currency::Xof
        } else {
            Currency::Eur
        },
        // Reversal needs a key of its own; an absent one would only mean a
        // row written before this column existed.
        idempotency_key: r.idempotency_key.unwrap_or_else(|| r.id.to_string()),
    }))
}

/// A provider talking about a payout we have no record of.
///
/// Never silent: it means either an event for another environment sharing
/// the credential, or money that moved outside our books. Both need eyes.
async fn warn_unknown(db: &PgPool, provider: &str, reference: &str, what: &str) {
    let known: Option<String> = sqlx::query_scalar(
        "SELECT status FROM payouts WHERE provider = $1 AND provider_reference = $2",
    )
    .bind(provider)
    .bind(reference)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    match known {
        // Already in a final state: a redelivery of something handled.
        Some(status) => tracing::debug!(
            provider = provider,
            reference = %reference,
            status = %status,
            "webhook about an already-{what} payout — nothing to do"
        ),
        None => {
            metrics::counter!(
                "skilluv_payment_webhook_orphan_total",
                "provider" => provider.to_string()
            )
            .increment(1);
            tracing::error!(
                provider = provider,
                reference = %reference,
                "provider reports a {what} payout we have no record of"
            );
        }
    }
}

/// Write the event down. `None` means it was already there.
async fn store(
    db: &PgPool,
    provider: &str,
    event_id: &str,
    kind: Option<&str>,
    payload: &Value,
    signature_verified: bool,
) -> Result<Option<Uuid>, AppError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO payment_webhook_events
            (provider, provider_event_id, kind, payload, signature_verified)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider, provider_event_id) DO NOTHING
         RETURNING id",
    )
    .bind(provider)
    .bind(event_id)
    .bind(kind)
    .bind(payload)
    .bind(signature_verified)
    .fetch_optional(db)
    .await?;
    Ok(id)
}

/// A stand-in event id for providers that send none.
///
/// Deduplicates byte-identical redeliveries, which is all it claims to do:
/// two genuinely distinct events with the same body are indistinguishable,
/// and for payout callbacks — which name a reference — they do not occur.
fn fingerprint(body: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(body.as_bytes());
    format!("sha256:{}", hex::encode(digest))
}

/// Map a provider's own status vocabulary onto ours.
///
/// Shared because the reconciliation sweep asks the same question over the
/// API that the webhook answers unprompted, and the two must not disagree.
pub fn state_to_event(state: PayoutState, reference: &str) -> Event {
    match state {
        PayoutState::Completed => Event::PayoutSettled {
            reference: reference.to_string(),
        },
        PayoutState::Rejected => Event::PayoutFailed {
            reference: reference.to_string(),
            reason: Some("the provider reports this payout as failed".into()),
        },
        PayoutState::Pending => Event::Ignored {
            kind: "still_pending".into(),
        },
    }
}

/// Apply an event that did not arrive over a webhook.
///
/// The reconciliation sweep polls providers and produces the same [`Event`]
/// values a callback would. Routing both through one function is what keeps
/// a payout resolved by polling indistinguishable from one resolved by a
/// callback.
pub async fn apply_event(db: &PgPool, provider: &str, event: &Event) -> Result<Outcome, AppError> {
    apply(db, provider, event).await
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn a_fingerprint_is_stable_and_distinguishes_bodies() {
        assert_eq!(fingerprint("{\"a\":1}"), fingerprint("{\"a\":1}"));
        assert_ne!(fingerprint("{\"a\":1}"), fingerprint("{\"a\":2}"));
        assert!(fingerprint("{}").starts_with("sha256:"));
    }

    #[test]
    fn every_event_carries_a_readable_kind() {
        assert_eq!(
            Event::PayoutSettled {
                reference: "x".into()
            }
            .kind(),
            "payout.settled"
        );
        assert_eq!(
            Event::PayoutFailed {
                reference: "x".into(),
                reason: None
            }
            .kind(),
            "payout.failed"
        );
        assert_eq!(
            Event::Ignored {
                kind: "charge.refunded".into()
            }
            .kind(),
            "ignored:charge.refunded"
        );
    }

    #[test]
    fn a_polled_status_becomes_the_same_event_a_callback_would() {
        assert_eq!(
            state_to_event(PayoutState::Completed, "r1"),
            Event::PayoutSettled {
                reference: "r1".into()
            }
        );
        assert!(matches!(
            state_to_event(PayoutState::Rejected, "r1"),
            Event::PayoutFailed { .. }
        ));
        assert!(matches!(
            state_to_event(PayoutState::Pending, "r1"),
            Event::Ignored { .. }
        ));
    }
}
