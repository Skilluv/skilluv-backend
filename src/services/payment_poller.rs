//! Asking the provider, so a closed browser tab costs nothing.
//!
//! ## The scenario
//!
//! Someone opens a Mobile Money payment, confirms the prompt on their
//! phone, and refreshes the page — or their connection drops, or they
//! simply close the tab because the money has left their account and as far
//! as they are concerned it is done. FedaPay has the payment. If the
//! webhook is lost, delayed, or retried into an endpoint that answered
//! non-2xx ten times and got itself disabled, nothing in the backend ever
//! hears.
//!
//! A webhook is an optimisation. It is not a guarantee, and every provider
//! says so in its own documentation. The guarantee has to be ours.
//!
//! ## What this does
//!
//! Every minute, it takes the payments that are still open and asks their
//! provider what became of them — by our own `merchant_reference`, so it
//! works even when the response that would have given us their id was the
//! thing that got lost. Anything approved goes through the same delivery
//! path a webhook would have taken.
//!
//! ## Why it backs off rather than polling flat out
//!
//! A Mobile Money prompt is answered in seconds or abandoned. A payment
//! twenty minutes old is almost certainly abandoned, and asking about it
//! every minute for a day is a request per minute per abandoned checkout.
//! The interval widens with the payment's age, and stops entirely at the
//! provider's own expiry — FedaPay expires an unfinished transaction after
//! twenty-four hours, and after that there is nothing to ask about.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How long after creation a payment is first asked about.
///
/// Long enough that the prompt has had time to be answered, short enough
/// that a payer who did finish is not left waiting.
const FIRST_CHECK_SECONDS: i64 = 45;

/// FedaPay expires an unfinished transaction after twenty-four hours.
/// Past that the answer cannot change.
const GIVE_UP_HOURS: i64 = 25;

/// A payment still waiting on an answer.
#[derive(sqlx::FromRow)]
struct Open {
    id: Uuid,
    provider: String,
    merchant_reference: Option<String>,
    /// Their identifier for the checkout. Absent exactly when the create
    /// response never reached us, which is the case the recovery path
    /// exists for.
    provider_session_id: Option<String>,
    age_seconds: f64,
    check_count: i32,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct PollReport {
    pub asked: usize,
    pub settled: usize,
    pub failed: usize,
    pub still_open: usize,
    /// Payments whose money arrived and whose delivery had not run. The
    /// number that matters: every one of these is a customer who paid and
    /// received nothing until this ran.
    pub rescued: usize,
}

/// Ask about everything still open, and deliver what has been paid.
pub async fn poll(db: &PgPool) -> Result<PollReport, AppError> {
    let mut report = PollReport::default();

    // Deliveries that were owed and never happened. Checked first and
    // separately from the provider calls: this needs no network, and it is
    // the case where the money is already ours.
    for paid in crate::services::fulfilment::undelivered(db, 60).await? {
        match crate::services::fulfilment::settle_and_deliver(db, paid.id, None).await {
            Ok(true) => report.rescued += 1,
            Ok(false) => {}
            Err(e) => tracing::warn!(payment = %paid.id, error = %e, "delivery still failing"),
        }
    }

    let open: Vec<Open> = sqlx::query_as(
        "SELECT id, provider, merchant_reference, provider_session_id,
                EXTRACT(EPOCH FROM (NOW() - created_at))::float8 AS age_seconds,
                check_count
           FROM payments
          WHERE status = 'pending'
            AND created_at < NOW() - ($1 || ' seconds')::INTERVAL
            AND created_at > NOW() - ($2 || ' hours')::INTERVAL
          ORDER BY created_at
          LIMIT 200",
    )
    .bind(FIRST_CHECK_SECONDS.to_string())
    .bind(GIVE_UP_HOURS.to_string())
    .fetch_all(db)
    .await?;

    for payment in open {
        if !due(payment.age_seconds, payment.check_count) {
            report.still_open += 1;
            continue;
        }

        sqlx::query(
            "UPDATE payments SET check_count = check_count + 1, last_checked_at = NOW()
              WHERE id = $1",
        )
        .bind(payment.id)
        .execute(db)
        .await?;
        report.asked += 1;

        // Both providers can be asked now, by different means and with
        // the same guarantee. Neither goes through a search endpoint:
        // Stripe's own documentation says search can be an hour behind
        // during an incident, which is no basis for deciding whether
        // somebody has paid.
        let lookup = match payment.provider.as_str() {
            "fedapay" => {
                let Some(cfg) = crate::services::fedapay::FedaPayConfig::from_env() else {
                    continue;
                };
                match (
                    payment.merchant_reference.as_deref(),
                    payment.provider_session_id.as_deref(),
                ) {
                    (Some(reference), _) => {
                        crate::services::fedapay::transaction_by_merchant_reference(&cfg, reference)
                            .await
                    }
                    (None, Some(id)) => {
                        crate::services::fedapay::transaction_status(&cfg, id).await
                    }
                    (None, None) => continue,
                }
            }

            "stripe" => {
                let Some(cfg) = crate::services::stripe::StripeConfig::from_env() else {
                    continue;
                };
                stripe_outcome(db, &cfg, &payment).await
            }

            // A provider this deployment cannot ask. Not an error, and not
            // something to count as asked either.
            _ => {
                report.still_open += 1;
                continue;
            }
        };

        match lookup {
            Ok(status) => match status.as_str() {
                // The whole point of this module. `paid` is Stripe's word
                // and the other two are FedaPay's, normalised here rather
                // than in two separate loops.
                "approved" | "transferred" | "paid" => {
                    match crate::services::fulfilment::settle_and_deliver(db, payment.id, None)
                        .await
                    {
                        Ok(true) => {
                            report.settled += 1;
                            tracing::info!(
                                payment = %payment.id,
                                "payment confirmed by polling — no webhook was needed"
                            );
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::error!(payment = %payment.id, error = %e, "delivery failed")
                        }
                    }
                }
                "declined" | "canceled" | "expired" | "failed" => {
                    sqlx::query(
                        "UPDATE payments SET status = 'failed', failure_reason = $2 WHERE id = $1",
                    )
                    .bind(payment.id)
                    .bind(format!("the provider reports this payment as {status}"))
                    .execute(db)
                    .await?;
                    report.failed += 1;
                }
                // `pending`, and anything they add later. Asking again
                // later is the right answer to a word we do not know.
                _ => report.still_open += 1,
            },
            Err(e) => {
                tracing::warn!(
                    payment = %payment.id,
                    error = %e,
                    "could not ask the provider about a payment"
                );
                report.still_open += 1;
            }
        }
    }

    if report.rescued > 0 {
        metrics::counter!("skilluv_payments_rescued_total").increment(report.rescued as u64);
    }
    metrics::gauge!("skilluv_payments_open").set(report.still_open as f64);
    Ok(report)
}

/// Ask Stripe what became of a checkout, recovering the session if needed.
///
/// Two paths, and the second is the interesting one.
///
/// With a session id, retrieving it is strongly consistent and cheap.
///
/// Without one -- the create response was lost, which is exactly what a
/// dropped connection leaves behind -- the original create call is replayed
/// under the same idempotency key. Stripe keeps the first response for that
/// key for twenty-four hours and returns it verbatim, so this is not a
/// second charge: it is asking what our own request produced. It is the
/// counterpart of FedaPay's lookup by merchant reference, and the better of
/// the two, because it does not go through an eventually-consistent index.
async fn stripe_outcome(
    db: &PgPool,
    cfg: &crate::services::stripe::StripeConfig,
    payment: &Open,
) -> Result<String, AppError> {
    let session = match payment.provider_session_id.as_deref() {
        Some(session_id) => {
            crate::services::stripe::retrieve_checkout_session(cfg, session_id).await?
        }
        None => {
            let Some(recovered) = recover_stripe_session(db, cfg, payment).await? else {
                // Past the twenty-four hours Stripe keeps an idempotency
                // key, or missing a detail needed to replay. Nothing left
                // to ask; the row stays as evidence a checkout was opened.
                return Ok("expired".to_string());
            };
            recovered
        }
    };

    // Learned or confirmed, either way worth keeping: the next poll takes
    // the cheap path, and a refund needs the payment intent.
    let session_id = session.get("id").and_then(|v| v.as_str());
    let intent = session.get("payment_intent").and_then(|v| v.as_str());
    if session_id.is_some() || intent.is_some() {
        sqlx::query(
            "UPDATE payments
                SET provider_session_id = COALESCE(provider_session_id, $2),
                    provider_reference = COALESCE(provider_reference, $3)
              WHERE id = $1",
        )
        .bind(payment.id)
        .bind(session_id)
        .bind(intent)
        .execute(db)
        .await?;
    }

    Ok(crate::services::stripe::session_outcome(&session).to_string())
}

/// Replay the create call under its original idempotency key.
///
/// Every parameter must match the first request or Stripe answers
/// `idempotency_error` -- a guard rather than an obstacle, because a
/// mismatch means this is not the same request and must not be treated as
/// one. So the values are read back from the payment row rather than
/// rebuilt from anything that may have moved on since.
async fn recover_stripe_session(
    db: &PgPool,
    cfg: &crate::services::stripe::StripeConfig,
    payment: &Open,
) -> Result<Option<serde_json::Value>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Original {
        idempotency_key: Option<String>,
        merchant_reference: Option<String>,
        amount: bigdecimal::BigDecimal,
        currency: String,
        email: Option<String>,
    }

    let original: Option<Original> = sqlx::query_as(
        "SELECT p.idempotency_key, p.merchant_reference, p.amount, p.currency, u.email
           FROM payments p
           LEFT JOIN users u ON u.id = p.payer_id
          WHERE p.id = $1",
    )
    .bind(payment.id)
    .fetch_optional(db)
    .await?;

    let Some(original) = original else {
        return Ok(None);
    };
    let (Some(key), Some(reference), Some(email)) = (
        original.idempotency_key,
        original.merchant_reference,
        original.email,
    ) else {
        return Ok(None);
    };

    use num_traits::ToPrimitive;
    let minor = match original.currency.as_str() {
        // XOF has no subdivision.
        "XOF" => original.amount.to_i64().unwrap_or(0),
        _ => (original.amount * bigdecimal::BigDecimal::from(100))
            .to_i64()
            .unwrap_or(0),
    };

    let recovered = crate::services::stripe::recover_checkout_session(
        cfg,
        &crate::services::stripe::PaymentCheckout {
            amount_minor: minor,
            currency: &original.currency,
            description: "Skilluv",
            customer_email: &email,
            client_reference_id: &reference,
            success_url: &cfg.success_url,
            cancel_url: &cfg.cancel_url,
            idempotency_key: &key,
        },
    )
    .await;

    match recovered {
        Ok(session) => Ok(Some(session)),
        Err(e) => {
            // `idempotency_error` means the parameters no longer match the
            // original. Recovery is impossible and guessing would be worse:
            // a human reconciles this one.
            tracing::error!(
                payment = %payment.id,
                error = %e,
                "could not recover a Stripe session -- reconcile this payment by hand"
            );
            Ok(None)
        }
    }
}

/// Whether this payment is due for another question.
///
/// Widening intervals: seconds at first, minutes after that. A prompt is
/// answered quickly or not at all, so the value of asking drops fast while
/// the cost of asking does not.
fn due(age_seconds: f64, check_count: i32) -> bool {
    let wait = match check_count {
        0 => 0.0,        // the first check, already gated by the query
        1..=4 => 60.0,   // the first few minutes: once a minute
        5..=10 => 300.0, // then every five
        _ => 1800.0,     // then every half hour until it expires
    };
    age_seconds >= FIRST_CHECK_SECONDS as f64 + wait * (check_count.max(1) - 1) as f64
}

/// Run it on a timer.
///
/// Every minute. Not behind a feature flag, for the same reason the release
/// sweep is not: a deployment that forgets to enable it looks healthy while
/// taking money and delivering nothing.
pub fn start_payment_poller(db: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            ticker.tick().await;
            match poll(&db).await {
                Ok(report) if report.asked > 0 || report.rescued > 0 => {
                    tracing::info!(
                        asked = report.asked,
                        settled = report.settled,
                        failed = report.failed,
                        rescued = report.rescued,
                        still_open = report.still_open,
                        "payment poll"
                    );
                }
                Ok(_) => {}
                Err(e) => tracing::error!(
                    error = %e,
                    "payment poll failed — payments confirmed at the provider stayed unconfirmed here"
                ),
            }
        }
    });
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn the_first_question_comes_quickly_and_the_rest_slow_down() {
        // A payer who confirmed in ten seconds should not wait minutes.
        assert!(due(FIRST_CHECK_SECONDS as f64, 0));
        assert!(
            !due(10.0, 0),
            "before the prompt has had time to be answered"
        );

        // Then the gap widens: a payment nobody has answered in ten minutes
        // is almost certainly abandoned, and asking every minute for a day
        // is a request per minute per abandoned checkout.
        assert!(!due(60.0, 3), "still inside the previous wait");
        assert!(due(600.0, 3));
        assert!(!due(600.0, 12), "half-hourly by then");
        assert!(due(20_000.0, 12));
    }
}
