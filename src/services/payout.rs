//! Payout orchestration — one way to send money out, whatever the rail.
//!
//! `services::psp` already abstracts *collection*: a `PaymentProvider` trait,
//! a registry, adapters for Stripe, Paystack and Flutterwave. None of it is
//! reachable — `PaymentRegistry` is never constructed, and the checkout route
//! calls Stripe directly. And it only ever covered money coming in.
//!
//! Sending money out had no abstraction at all. `stripe_withdraw` and
//! `momo_withdraw` are two parallel implementations of the same flow in one
//! file, each with its own idea of what happens when the provider refuses.
//! Adding PayPal, FedaPay, Wave or FeexPay meant a third and a fourth.
//!
//! This module is the missing half:
//!
//! * [`PayoutProvider`] — what a rail must be able to do.
//! * [`Route`] and [`routes`] — which rail serves which country and currency,
//!   as data. Adding a country is a row, not a branch.
//! * [`send`] — the one entry point. Records the movement in the ledger,
//!   asks the provider, and reverses the record if the provider says no.
//!
//! ## Why the routing is data
//!
//! No provider covers the world. Stripe cannot pay out to Benin; Mobile
//! Money cannot pay a mentor in France. Which rail reaches whom depends on
//! the recipient's country and the currency, changes as providers open
//! markets, and is exactly the kind of knowledge that rots when compiled in.
//! `psp::DEFAULT_PROVIDER_BY_COUNTRY` is the earlier attempt: a const array,
//! covering only collection, referencing adapters nobody registers.

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::{self, Currency};

/// Where the money is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rail {
    /// Bank account or debit card, through a connected account.
    BankAccount,
    /// Mobile Money wallet, identified by a phone number.
    MobileMoney,
}

/// Everything a provider needs to pay one person.
#[derive(Debug, Clone)]
pub struct PayoutRequest<'a> {
    pub user_id: Uuid,
    pub amount: &'a BigDecimal,
    pub currency: Currency,
    /// Account identifier on the destination rail: a connected account id, a
    /// phone number in E.164. Opaque to this module.
    pub destination: &'a str,
    /// Short human-readable reason, shown to the recipient where the rail
    /// supports it.
    pub note: &'a str,
    /// Stable key so a retried request cannot pay twice.
    pub idempotency_key: &'a str,
}

/// Who the money is for, as a rail needs to name them.
///
/// Resolved once by [`send`] rather than carried in [`PayoutRequest`]: the
/// caller has a `user_id` and no reason to know that FedaPay wants a name
/// split in two while Stripe wants neither. A provider that does not care
/// ignores it.
#[derive(Debug, Clone)]
pub struct Recipient {
    /// Display name, or the username where there is none. Never empty.
    pub name: String,
    pub email: Option<String>,
    /// ISO 3166-1 alpha-2, from the account's declared residency.
    pub country: Option<String>,
}

/// What the provider answered.
#[derive(Debug, Clone, Serialize)]
pub struct PayoutReceipt {
    pub provider: String,
    /// Provider-side identifier, for reconciliation against their statement.
    pub reference: String,
    pub status: PayoutState,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutState {
    /// Accepted, settlement confirmed later — the normal Mobile Money case.
    Pending,
    /// Confirmed by the provider.
    Completed,
    /// Refused. The ledger movement is reversed by [`send`].
    Rejected,
}

/// One way of sending money out.
///
/// Deliberately narrower than `psp::PaymentProvider`: a rail that can only
/// pay out implements this and nothing else, and one that does both
/// implements both. Splitting by capability rather than by vendor is what
/// keeps FeexPay — collection only, today — from having to pretend it can
/// pay out.
#[async_trait]
pub trait PayoutProvider: Send + Sync {
    /// Stable identifier. Also the ledger account segment, so renaming one
    /// orphans its balance history.
    fn name(&self) -> &'static str;

    fn rail(&self) -> Rail;

    fn supports(&self, currency: Currency) -> bool;

    async fn pay(
        &self,
        request: &PayoutRequest<'_>,
        recipient: &Recipient,
    ) -> Result<PayoutReceipt, AppError>;

    /// Ask the provider what became of a payout it accepted earlier.
    ///
    /// `Ok(None)` means this rail cannot be polled and only ever answers
    /// over a callback — the Mobile Money operators, today. That is not a
    /// failure, but it does mean an unconfirmed payout there can only be
    /// escalated to a human, never resolved automatically, which is why
    /// [`crate::services::reconciliation`] treats the two differently.
    ///
    /// The default is `Ok(None)`, so a new adapter that has not thought
    /// about reconciliation cannot silently claim a payout succeeded.
    async fn status(&self, _reference: &str) -> Result<Option<PayoutState>, AppError> {
        Ok(None)
    }
}

/// Every provider name that may become a ledger account.
///
/// The ledger takes `&'static str` for an account segment on purpose: an
/// account code assembled from runtime data is one a typo or a hostile
/// webhook can create, and a ledger with accounts nobody meant to open is
/// a ledger that no longer balances against anything.
///
/// A provider string arriving from a webhook or from a database row is
/// matched back to this list before it can move money.
const KNOWN_PROVIDERS: [&str; 5] = ["stripe", "fedapay", "orange", "mtn", "wave"];

/// Turn a runtime provider name into one the ledger will accept.
///
/// `None` for anything unknown, which the callers report rather than
/// guessing at — a payout attributed to the wrong provider account is a
/// reconciliation that never balances again.
pub fn canonical_provider(name: &str) -> Option<&'static str> {
    KNOWN_PROVIDERS
        .iter()
        .find(|known| **known == name)
        .copied()
}

/// A rule saying which provider serves a destination.
///
/// Ordered by `priority`, so a cheaper or more direct rail can be preferred
/// without deleting the fallback.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Route {
    /// ISO 3166-1 alpha-2, or NULL meaning "anywhere not matched above".
    pub country: Option<String>,
    pub currency: String,
    pub rail: String,
    pub provider: String,
    pub priority: i16,
    pub enabled: bool,
}

/// Routes for a destination, best first.
///
/// A country-specific rule wins over the catch-all, and within a tier the
/// lower priority wins. Returning a list rather than one provider is what
/// allows a caller to fall back when the first refuses.
pub async fn routes(
    db: &PgPool,
    country: Option<&str>,
    currency: Currency,
    rail: Rail,
) -> Result<Vec<Route>, AppError> {
    let rail_str = match rail {
        Rail::BankAccount => "bank_account",
        Rail::MobileMoney => "mobile_money",
    };
    let rows = sqlx::query_as(
        r#"
        SELECT country, currency, rail, provider, priority, enabled
          FROM payout_routes
         WHERE enabled = TRUE
           AND currency = $2
           AND rail = $3
           AND (country = $1 OR country IS NULL)
         ORDER BY (country IS NULL), priority
        "#,
    )
    .bind(country.map(str::to_uppercase))
    .bind(currency.as_str())
    .bind(rail_str)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Registry of the providers this deployment can actually use.
///
/// Populated at startup from configuration, so a provider whose credentials
/// are absent is simply not there — rather than present and failing on the
/// first real payout.
#[derive(Default)]
pub struct PayoutRegistry {
    providers: Vec<std::sync::Arc<dyn PayoutProvider>>,
}

impl PayoutRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: std::sync::Arc<dyn PayoutProvider>) {
        self.providers.push(provider);
    }

    pub fn get(&self, name: &str) -> Option<std::sync::Arc<dyn PayoutProvider>> {
        self.providers.iter().find(|p| p.name() == name).cloned()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }

    /// First configured provider able to serve this destination.
    ///
    /// A route naming a provider this deployment has no credentials for is
    /// skipped rather than fatal: the table is shared across environments,
    /// and staging is not expected to hold every production secret.
    pub async fn resolve(
        &self,
        db: &PgPool,
        country: Option<&str>,
        currency: Currency,
        rail: Rail,
    ) -> Result<std::sync::Arc<dyn PayoutProvider>, AppError> {
        let candidates = routes(db, country, currency, rail).await?;
        for route in &candidates {
            if let Some(provider) = self.get(&route.provider)
                && provider.supports(currency)
            {
                return Ok(provider);
            }
        }
        Err(AppError::Validation(format!(
            "no payout provider configured for {} in {} ({:?}). Routes matched: {}",
            currency.as_str(),
            country.unwrap_or("an unspecified country"),
            rail,
            if candidates.is_empty() {
                "none".to_string()
            } else {
                candidates
                    .iter()
                    .map(|r| r.provider.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )))
    }
}

/// Send money out, recording it.
///
/// Order matters and is the point of this function:
///
/// 1. The ledger movement is posted **first**, under the caller's
///    idempotency key. A replay stops here.
/// 2. The provider is asked.
/// 3. If it refuses, the movement is reversed by a compensating entry.
///
/// Asking the provider first would leave money sent and unrecorded whenever
/// the process died in between — the failure mode that is impossible to
/// reconcile afterwards. Recording first can only leave a movement that is
/// reversed, which is visible and correctable.
pub async fn send(
    db: &PgPool,
    provider: &dyn PayoutProvider,
    request: PayoutRequest<'_>,
) -> Result<PayoutReceipt, AppError> {
    let recipient = recipient_of(db, request.user_id).await?;

    let posted = ledger::withdraw(
        db,
        request.user_id,
        request.amount.clone(),
        request.currency,
        provider.name(),
        // The provider reference is unknown until it answers; the ledger
        // transaction is updated below once it does.
        String::new(),
        request.idempotency_key.to_string(),
    )
    .await?;

    if posted.was_replay() {
        return Ok(PayoutReceipt {
            provider: provider.name().to_string(),
            reference: String::new(),
            status: PayoutState::Pending,
            message: Some("already recorded under this idempotency key — not sent again".into()),
        });
    }

    // The lifecycle row goes in before the provider is asked, for the same
    // reason the ledger movement does: a process that dies mid-call must
    // leave something for the sweep to find. A payout nobody recorded is
    // one nobody will ever chase.
    let payout_id: Uuid = sqlx::query_scalar(
        "INSERT INTO payouts
            (user_id, ledger_transaction_id, provider, rail, amount, currency,
             destination_masked, idempotency_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(request.user_id)
    .bind(posted.transaction_id())
    .bind(provider.name())
    .bind(match provider.rail() {
        Rail::BankAccount => "bank_account",
        Rail::MobileMoney => "mobile_money",
    })
    .bind(request.amount)
    .bind(request.currency.as_str())
    .bind(mask_destination(request.destination))
    .bind(request.idempotency_key)
    .fetch_one(db)
    .await?;

    match provider.pay(&request, &recipient).await {
        Ok(receipt) if receipt.status != PayoutState::Rejected => {
            sqlx::query("UPDATE ledger_transactions SET provider_reference = $2 WHERE id = $1")
                .bind(posted.transaction_id())
                .bind(&receipt.reference)
                .execute(db)
                .await?;

            // `Completed` here means the provider settled synchronously.
            // Most rails answer `Pending` and confirm over a webhook, which
            // is what the sweep and the webhook handler exist for.
            let settled = receipt.status == PayoutState::Completed;
            sqlx::query(
                "UPDATE payouts
                    SET provider_reference = $2,
                        status = CASE WHEN $3 THEN 'sent' ELSE 'pending' END,
                        settled_at = CASE WHEN $3 THEN NOW() ELSE NULL END
                  WHERE id = $1",
            )
            .bind(payout_id)
            .bind(&receipt.reference)
            .bind(settled)
            .execute(db)
            .await?;

            Ok(receipt)
        }
        Ok(receipt) => {
            reverse(db, provider, &request, receipt.message.as_deref()).await?;
            fail_payout(db, payout_id, receipt.message.as_deref()).await?;
            Ok(receipt)
        }
        Err(e) => {
            reverse(db, provider, &request, Some(&e.to_string())).await?;
            fail_payout(db, payout_id, Some(&e.to_string())).await?;
            Err(e)
        }
    }
}

/// Mark a payout failed, with the provider's own words.
async fn fail_payout(db: &PgPool, payout_id: Uuid, reason: Option<&str>) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE payouts
            SET status = 'failed', settled_at = NOW(), failure_reason = $2
          WHERE id = $1",
    )
    .bind(payout_id)
    // The column is NOT NULL when the status is `failed`, and a refusal
    // without a message is common enough that a placeholder beats a
    // constraint violation swallowing the whole failure path.
    .bind(reason.unwrap_or("the provider refused without giving a reason"))
    .execute(db)
    .await?;
    Ok(())
}

/// Keep enough of a destination to recognise it, not enough to use it.
///
/// A phone number or an account id in a table an operator browses is a
/// privacy problem waiting to become a data-protection one. The last four
/// characters are what a person recognises as their own.
fn mask_destination(destination: &str) -> String {
    let chars: Vec<char> = destination.chars().collect();
    if chars.len() <= 4 {
        return "•".repeat(chars.len());
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}{tail}", "•".repeat(chars.len().min(20) - 4))
}

/// Who this payout is for, as the rails need to name them.
///
/// The residency country is preferred over the profile country: it is the
/// one the person declared for payment purposes, and the one the routing
/// already used to pick this provider.
async fn recipient_of(db: &PgPool, user_id: Uuid) -> Result<Recipient, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        display_name: Option<String>,
        username: String,
        email: Option<String>,
        country_iso2: Option<String>,
        residency_country: Option<String>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT u.display_name, u.username, u.email, u.country_iso2, w.residency_country
           FROM users u
           LEFT JOIN talent_wallets w ON w.user_id = u.id
          WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Err(AppError::NotFound("user".into()));
    };
    let (profile_country, residency) = (row.country_iso2, row.residency_country);

    Ok(Recipient {
        // A rail that must print a name gets one either way.
        name: row
            .display_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(row.username),
        email: row.email,
        country: residency.or(profile_country),
    })
}

/// Put the money back after a refusal, and say so loudly.
async fn reverse(
    db: &PgPool,
    provider: &dyn PayoutProvider,
    request: &PayoutRequest<'_>,
    reason: Option<&str>,
) -> Result<(), AppError> {
    ledger::reverse_withdrawal(
        db,
        request.user_id,
        request.amount.clone(),
        request.currency,
        provider.name(),
        request.idempotency_key,
    )
    .await?;

    metrics::counter!(
        "skilluv_payout_rejected_total",
        "provider" => provider.name().to_string(),
        "currency" => request.currency.as_str().to_string()
    )
    .increment(1);

    tracing::error!(
        user = %request.user_id,
        provider = provider.name(),
        amount = %request.amount,
        currency = request.currency.as_str(),
        reason = reason.unwrap_or("unspecified"),
        "payout refused — funds returned to the recipient's available balance"
    );
    Ok(())
}
