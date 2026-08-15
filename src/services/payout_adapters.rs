//! Adapters wiring the existing rails onto [`PayoutProvider`].
//!
//! The Stripe transfer helper and the three Mobile Money implementations
//! already existed; what was missing was a shared shape and a single place
//! that decides which one to call. These adapters add no payment logic —
//! they translate, and nothing else.
//!
//! Adding FedaPay, FeexPay or PayPal means one more struct here plus a row in
//! `payout_routes`. Nothing in the ledger, nothing in the routes, nothing in
//! the callers.

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use num_traits::ToPrimitive;

use crate::errors::AppError;
use crate::services::ledger::Currency;
use crate::services::mobile_money::{self, PayoutParams, PayoutStatus, ProviderName};
use crate::services::payout::{
    PayoutProvider, PayoutReceipt, PayoutRequest, PayoutState, Rail, Recipient,
};

/// Stripe Connect transfers, for recipients in a country Stripe serves.
pub struct StripePayout {
    pub cfg: crate::services::stripe::StripeConfig,
}

#[async_trait]
impl PayoutProvider for StripePayout {
    fn name(&self) -> &'static str {
        "stripe"
    }

    fn rail(&self) -> Rail {
        Rail::BankAccount
    }

    fn supports(&self, currency: Currency) -> bool {
        // XOF payouts through Stripe exist only via Paystack, on a different
        // integration. Claiming support here would route Benin at Stripe and
        // fail at the API instead of at the routing table, where the message
        // can actually say why.
        matches!(currency, Currency::Eur)
    }

    async fn pay(
        &self,
        request: &PayoutRequest<'_>,
        // Stripe names the recipient from the connected account it already
        // holds, so nothing here is needed.
        _recipient: &Recipient,
    ) -> Result<PayoutReceipt, AppError> {
        let cents = to_minor_units(request.amount, request.currency)?;

        let response = crate::services::stripe::create_transfer(
            &self.cfg,
            request.destination,
            cents,
            &request.currency.as_str().to_lowercase(),
            request.note,
            request.idempotency_key,
        )
        .await?;

        let reference = response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        Ok(PayoutReceipt {
            provider: "stripe".to_string(),
            reference,
            // A Connect transfer is immediate between balances; arrival on
            // the recipient's bank account follows their payout schedule.
            status: PayoutState::Completed,
            message: None,
        })
    }

    async fn status(&self, reference: &str) -> Result<Option<PayoutState>, AppError> {
        let transfer = crate::services::stripe::retrieve_transfer(&self.cfg, reference).await?;
        // A transfer between balances has no failing state of its own; the
        // only way it stops being final is being reversed. An actual bank
        // rejection arrives later, as a `payout.failed` webhook.
        let reversed = transfer
            .get("reversed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        Ok(Some(if reversed {
            PayoutState::Rejected
        } else {
            PayoutState::Completed
        }))
    }
}

/// One Mobile Money operator.
pub struct MomoPayout {
    pub operator: ProviderName,
}

#[async_trait]
impl PayoutProvider for MomoPayout {
    fn name(&self) -> &'static str {
        self.operator.as_str()
    }

    fn rail(&self) -> Rail {
        Rail::MobileMoney
    }

    fn supports(&self, currency: Currency) -> bool {
        matches!(currency, Currency::Xof)
    }

    async fn pay(
        &self,
        request: &PayoutRequest<'_>,
        // The operator identifies the wallet by its number alone.
        _recipient: &Recipient,
    ) -> Result<PayoutReceipt, AppError> {
        let provider = mobile_money::get_provider(self.operator);
        let result = provider
            .initiate_payout(&PayoutParams {
                user_id: request.user_id,
                phone: request.destination,
                currency: request.currency.as_str(),
                amount: request.amount,
                note: request.note,
            })
            .await?;

        Ok(PayoutReceipt {
            provider: self.operator.as_str().to_string(),
            reference: result.provider_txn_id,
            status: match result.status {
                // Mobile Money settles asynchronously: the operator accepts
                // now and confirms later, over a webhook.
                PayoutStatus::Pending => PayoutState::Pending,
                PayoutStatus::Completed => PayoutState::Completed,
                PayoutStatus::Rejected => PayoutState::Rejected,
            },
            message: result.message,
        })
    }
}

/// FedaPay, reaching Mobile Money across West Africa on one credential.
///
/// This struct is the entire cost of adding a provider, and it is here to
/// prove that. What it took: this impl, one line in [`registry_from_env`],
/// and rows in `payout_routes` saying where it applies. No caller changed,
/// no branch was added anywhere, and the ledger never heard of it.
pub struct FedaPayPayout {
    pub cfg: crate::services::fedapay::FedaPayConfig,
}

#[async_trait]
impl PayoutProvider for FedaPayPayout {
    fn name(&self) -> &'static str {
        "fedapay"
    }

    fn rail(&self) -> Rail {
        Rail::MobileMoney
    }

    fn supports(&self, currency: Currency) -> bool {
        matches!(currency, Currency::Xof)
    }

    async fn pay(
        &self,
        request: &PayoutRequest<'_>,
        recipient: &Recipient,
    ) -> Result<PayoutReceipt, AppError> {
        // The country decides which operator FedaPay hands the number to, so
        // an absent one is a refusal we can explain rather than a transfer
        // sent into the wrong network.
        let country = recipient.country.as_deref().ok_or_else(|| {
            AppError::Validation(
                "a country of residence is required before withdrawing to Mobile Money".into(),
            )
        })?;

        let amount = to_minor_units(request.amount, request.currency)?;
        let payout = crate::services::fedapay::send_payout(
            &self.cfg,
            &crate::services::fedapay::Transfer {
                amount,
                currency_iso: request.currency.as_str(),
                phone: request.destination,
                country,
                recipient_name: &recipient.name,
                recipient_email: recipient.email.as_deref(),
                description: request.note,
                idempotency_key: request.idempotency_key,
            },
        )
        .await?;

        Ok(PayoutReceipt {
            provider: "fedapay".to_string(),
            reference: payout.id.to_string(),
            status: match payout.status.as_str() {
                "sent" => PayoutState::Completed,
                "failed" => PayoutState::Rejected,
                // `pending`, `started`, `processing` — accepted, settling.
                _ => PayoutState::Pending,
            },
            message: payout.message,
        })
    }

    async fn status(&self, reference: &str) -> Result<Option<PayoutState>, AppError> {
        let status = crate::services::fedapay::payout_status(&self.cfg, reference).await?;
        Ok(Some(match status.as_str() {
            "sent" => PayoutState::Completed,
            "failed" => PayoutState::Rejected,
            // `pending`, `started`, `processing` — still in flight.
            _ => PayoutState::Pending,
        }))
    }
}

/// Convert a decimal amount to the unit the provider expects.
///
/// EUR has cents; XOF has no minor unit, so a fractional amount would be
/// silently truncated by the provider. Rejecting it here means the error
/// names the cause instead of a payout arriving short.
fn to_minor_units(amount: &BigDecimal, currency: Currency) -> Result<i64, AppError> {
    let scaled = match currency {
        Currency::Eur => amount * BigDecimal::from(100),
        Currency::Xof => {
            if amount.fractional_digit_count() > 0 && amount != &amount.with_scale(0) {
                return Err(AppError::Validation(
                    "XOF has no minor unit — amount must be a whole number of francs".into(),
                ));
            }
            amount.clone()
        }
    };
    scaled
        .to_i64()
        .ok_or_else(|| AppError::Validation("amount too large for this provider".into()))
}

/// Build the registry from whatever this deployment is configured for.
///
/// A provider whose credentials are absent is simply not registered, so a
/// route pointing at it is skipped with a message rather than failing at the
/// provider's API with a credentials error.
pub fn registry_from_env() -> crate::services::payout::PayoutRegistry {
    use std::sync::Arc;

    let mut registry = crate::services::payout::PayoutRegistry::new();

    if let Some(cfg) = crate::services::stripe::StripeConfig::from_env() {
        registry.register(Arc::new(StripePayout { cfg }));
    }

    if let Some(cfg) = crate::services::fedapay::FedaPayConfig::from_env() {
        registry.register(Arc::new(FedaPayPayout { cfg }));
    }

    // The Mobile Money implementations read their own credentials when
    // called and fall back to a sandbox stub, so they are always available.
    for operator in [ProviderName::Orange, ProviderName::Mtn, ProviderName::Wave] {
        registry.register(Arc::new(MomoPayout { operator }));
    }

    registry
}
