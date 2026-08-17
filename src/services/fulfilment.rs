//! Delivering what was paid for, from wherever the news arrives.
//!
//! ## The bug this exists to make impossible
//!
//! The payer opens a checkout, confirms on their phone, and closes the tab.
//! The provider has the money. Nothing in the backend hears, because
//! fulfilment hung off a webhook and the webhook was lost, retried into a
//! rate-limited endpoint, or never sent. The payment is real and the order
//! does not exist, and the only person who knows is the customer.
//!
//! It is not a FedaPay problem. The Stripe flow here had the same shape:
//! `checkout.session.completed` was the single road to delivery.
//!
//! ## Three roads, one destination
//!
//! * The **webhook**, when it arrives — fast, and not to be relied on.
//! * The **poller**, which asks the provider about anything still pending.
//!   It runs on a timer and does not care whether a browser is open.
//! * The **return page**, if the payer does come back — which triggers a
//!   check rather than performing the delivery itself, so a forged
//!   `?status=approved` buys nothing.
//!
//! All three end here, and this is idempotent: `fulfilled_at` is stamped
//! once and every later call is a no-op. Two roads arriving at the same
//! moment cost one delivery, not two.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// What a payment bought, and who for.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Paid {
    pub id: Uuid,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub payer_id: Option<Uuid>,
    pub payer_enterprise_id: Option<Uuid>,
    pub amount: bigdecimal::BigDecimal,
    pub currency: String,
    pub fulfilled_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Mark a payment as paid, then deliver what it bought.
///
/// Safe to call from anywhere, any number of times, in any order relative
/// to the other roads. Returns `true` when this call is the one that
/// delivered.
pub async fn settle_and_deliver(
    db: &PgPool,
    payment_id: Uuid,
    provider_reference: Option<&str>,
) -> Result<bool, AppError> {
    // Claim the delivery with the update itself. Two callers arriving
    // together — a webhook and the poller, which is the normal case rather
    // than the rare one — race here, and exactly one wins.
    let claimed: Option<Paid> = sqlx::query_as(
        "UPDATE payments
            SET status = 'succeeded',
                succeeded_at = COALESCE(succeeded_at, NOW()),
                provider_reference = COALESCE($2, provider_reference),
                fulfilled_at = NOW()
          WHERE id = $1
            AND fulfilled_at IS NULL
      RETURNING id, subject_type, subject_id, payer_id, payer_enterprise_id,
                amount, currency, fulfilled_at",
    )
    .bind(payment_id)
    .bind(provider_reference)
    .fetch_optional(db)
    .await?;

    let Some(paid) = claimed else {
        // Already delivered. Not an error and not worth a log line: it is
        // the expected outcome of having more than one road.
        return Ok(false);
    };

    // If delivery fails, the stamp is rolled back so another road retries.
    // Leaving it stamped would mean the money is taken, the order does not
    // exist, and nothing will ever look again.
    if let Err(e) = deliver(db, &paid).await {
        sqlx::query("UPDATE payments SET fulfilled_at = NULL WHERE id = $1")
            .bind(payment_id)
            .execute(db)
            .await?;
        tracing::error!(
            payment = %payment_id,
            subject = %paid.subject_type,
            error = %e,
            "payment succeeded but delivery failed — unstamped so the sweep retries"
        );
        return Err(e);
    }

    metrics::counter!(
        "skilluv_payments_fulfilled_total",
        "subject" => paid.subject_type.clone()
    )
    .increment(1);
    tracing::info!(
        payment = %payment_id,
        subject = %paid.subject_type,
        subject_id = %paid.subject_id,
        "delivered"
    );
    Ok(true)
}

/// Do the thing the money bought.
///
/// One place, switching on what was bought rather than on which provider
/// reported it. A new payment method reaches every flow without touching
/// any of them.
async fn deliver(db: &PgPool, paid: &Paid) -> Result<(), AppError> {
    match paid.subject_type.as_str() {
        "mentorship_session" => {
            // No `updated_at`: the table has never had that column, and
            // writing it meant every mentorship payment succeeded and then
            // failed to deliver — money taken, session left pending, and
            // the row handed to the sweep to retry forever.
            sqlx::query(
                "UPDATE mentorship_sessions
                    SET status = 'paid'
                  WHERE id = $1 AND status = 'pending'",
            )
            .bind(paid.subject_id)
            .execute(db)
            .await?;
            Ok(())
        }

        "certification_purchase" => {
            sqlx::query(
                "UPDATE certification_attempts
                    SET status = 'paid'
                  WHERE id = $1 AND status = 'pending'",
            )
            .bind(paid.subject_id)
            .execute(db)
            .await?;

            // A certification sale never reached the books. The platform is
            // the seller and nobody else is owed anything, which is why it
            // was skipped — and why it should not have been: this is the
            // account an accountant reads first, and it was understated by
            // every certification ever sold.
            //
            // Recorded as revenue captured for the platform: the money is at
            // the provider, and all of it is ours.
            crate::services::ledger::capture_platform_revenue(
                db,
                &paid.currency,
                paid.amount.clone(),
                "certification_purchase",
                paid.subject_id,
                format!("certification:{}", paid.id),
            )
            .await?;
            Ok(())
        }

        "credit_pack" => {
            let Some(enterprise_id) = paid.payer_enterprise_id else {
                return Err(AppError::Internal(
                    "a credit pack was paid for with no enterprise to credit".into(),
                ));
            };

            // How many credits, as agreed at checkout. Read from the
            // payment rather than from the pack table: a price or size
            // change between paying and delivering must not change what
            // someone receives.
            let credits: Option<i32> =
                sqlx::query_scalar("SELECT credits_purchased FROM payments WHERE id = $1")
                    .bind(paid.id)
                    .fetch_optional(db)
                    .await?
                    .flatten();
            let credits = credits.ok_or_else(|| {
                AppError::Internal(
                    "a credit pack was paid for without recording how many credits".into(),
                )
            })?;

            crate::services::credits::grant(
                db,
                crate::services::credits::GrantInput {
                    enterprise_id,
                    amount: &crate::services::credits::dec(&credits.to_string()),
                    reason: "pack_purchase",
                    // The payment is the idempotency anchor: this function
                    // runs once because `fulfilled_at` is claimed once, and
                    // the reference here is what an accountant follows back.
                    related_payment_id: Some(paid.id),
                    related_promo_code_id: None,
                    notes: Some(&format!("{credits} credit(s), payment {}", paid.id)),
                    actor_user_id: paid.payer_id,
                    expires_at: None,
                },
            )
            .await?;
            Ok(())
        }

        "mission_invoice" => {
            // The talent's share lands in `pending`, not `available`: the
            // client accepting delivery is what releases it, and until then
            // an unhappy payer has somewhere to raise it while the money is
            // still reversible.
            crate::services::mission_billing::capture(db, paid.subject_id, paid.id).await?;
            Ok(())
        }

        other => {
            // Loud, not silent. A payment for something nothing delivers is
            // money taken for nothing, and the only way anyone finds out is
            // if this says so.
            Err(AppError::Internal(format!(
                "payment for '{other}' has no delivery — money was taken and nothing was given"
            )))
        }
    }
}

/// Payments that took money and delivered nothing.
///
/// The query an operator wants at nine in the morning, and the one the
/// sweep runs every minute.
pub async fn undelivered(db: &PgPool, older_than_seconds: i64) -> Result<Vec<Paid>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, subject_type, subject_id, payer_id, payer_enterprise_id,
                amount, currency, fulfilled_at
           FROM payments
          WHERE status = 'succeeded'
            AND fulfilled_at IS NULL
            AND succeeded_at < NOW() - ($1 || ' seconds')::INTERVAL
          ORDER BY succeeded_at
          LIMIT 200",
    )
    .bind(older_than_seconds.to_string())
    .fetch_all(db)
    .await?)
}
