//! One impl per way of taking money.
//!
//! Same shape as `payout_adapters`, and for the same reason: opening a
//! corridor should be a struct here plus a row in `collection_routes`, not
//! an edit to three route modules.

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use num_traits::ToPrimitive;

use crate::errors::AppError;
use crate::services::collect::{Checkout, CollectionProvider, CollectionRequest, Method};
use crate::services::ledger::Currency;

/// Stripe Checkout — cards, in a currency Stripe settles.
pub struct StripeCollect {
    pub cfg: crate::services::stripe::StripeConfig,
}

#[async_trait]
impl CollectionProvider for StripeCollect {
    fn name(&self) -> &'static str {
        "stripe"
    }

    fn supports(&self, currency: Currency, method: Method) -> bool {
        // XOF is not a Stripe settlement currency, and a card is the only
        // thing this integration presents. Claiming more would route a
        // Beninese payer here and fail at the API rather than in the
        // routing table, where the message can say why.
        matches!(currency, Currency::Eur) && matches!(method, Method::Card)
    }

    async fn start(&self, request: &CollectionRequest<'_>) -> Result<Checkout, AppError> {
        let minor = to_minor_units(request.amount, request.currency)?;
        let reference = format!("{}:{}", request.subject_type, request.subject_id);
        let session = crate::services::stripe::create_payment_checkout(
            &self.cfg,
            &crate::services::stripe::PaymentCheckout {
                amount_minor: minor,
                currency: request.currency.as_str(),
                description: request.description,
                customer_email: request.payer_email,
                client_reference_id: &reference,
                success_url: request.success_url,
                cancel_url: request.cancel_url,
                idempotency_key: request.idempotency_key,
            },
        )
        .await?;

        let session_id = session
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal("stripe checkout has no id".into()))?
            .to_string();
        let redirect_url = session
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::Internal(
                    "stripe checkout has no url — the payer has nowhere to go".into(),
                )
            })?
            .to_string();

        Ok(Checkout {
            provider: "stripe".to_string(),
            session_id,
            redirect_url,
        })
    }

    async fn refund(
        &self,
        provider_reference: &str,
        amount: Option<&BigDecimal>,
        currency: Currency,
        reason: &str,
    ) -> Result<String, AppError> {
        let minor = amount.map(|a| to_minor_units(a, currency)).transpose()?;
        // Stripe accepts three reasons and nothing else, so ours travels as
        // `requested_by_customer` — which a dispute settled for the payer
        // is — and the detail stays in our own records.
        let _ = reason;
        let refund = crate::services::stripe::create_refund(
            &self.cfg,
            provider_reference,
            minor,
            Some("requested_by_customer"),
        )
        .await?;
        Ok(refund.id)
    }
}

/// FedaPay — Mobile Money and local cards across the franc zone.
///
/// The reason this module exists. Stripe cannot take Mobile Money in Benin,
/// and Mobile Money is how most people there hold money.
pub struct FedaPayCollect {
    pub cfg: crate::services::fedapay::FedaPayConfig,
    /// Where FedaPay sends the payer back to, and where its callback lands.
    pub callback_url: String,
}

#[async_trait]
impl CollectionProvider for FedaPayCollect {
    fn name(&self) -> &'static str {
        "fedapay"
    }

    fn supports(&self, currency: Currency, method: Method) -> bool {
        matches!(currency, Currency::Xof) && matches!(method, Method::MobileMoney | Method::Card)
    }

    async fn start(&self, request: &CollectionRequest<'_>) -> Result<Checkout, AppError> {
        // The country places the phone number on an operator's network.
        // Without it FedaPay cannot tell an MTN Benin number from an MTN
        // Côte d'Ivoire one, and the payer is sent to the wrong operator.
        let country = request.payer_country.ok_or_else(|| {
            AppError::Validation(
                "a country is required to pay by Mobile Money — it decides which operator".into(),
            )
        })?;
        let phone = request.payer_phone.ok_or_else(|| {
            AppError::Validation(
                "a phone number in E.164 is required to pay by Mobile Money".into(),
            )
        })?;

        let amount = to_minor_units(request.amount, request.currency)?;
        let (session_id, redirect_url) = crate::services::fedapay::create_checkout(
            &self.cfg,
            &crate::services::fedapay::Transfer {
                amount,
                currency_iso: request.currency.as_str(),
                phone,
                country,
                recipient_name: request.payer_name,
                recipient_email: Some(request.payer_email),
                description: request.description,
                idempotency_key: request.idempotency_key,
            },
            &self.callback_url,
        )
        .await?;

        Ok(Checkout {
            provider: "fedapay".to_string(),
            session_id,
            redirect_url,
        })
    }

    async fn refund(
        &self,
        provider_reference: &str,
        _amount: Option<&BigDecimal>,
        _currency: Currency,
        _reason: &str,
    ) -> Result<String, AppError> {
        // FedaPay documents no refund endpoint; refunds are made from their
        // dashboard. Returning an error rather than `Ok` is the whole point
        // of the trait's contract: a silent success here would mean the
        // books say refunded and the payer was never credited, which is the
        // one accounting error a customer notices before we do.
        Err(AppError::Internal(format!(
            "fedapay exposes no refund API — refund transaction {provider_reference} from the \
             FedaPay dashboard, then mark the payment refunded"
        )))
    }
}

/// Convert to the unit the provider expects.
///
/// EUR has cents; XOF has none, so a fractional amount would be silently
/// truncated. Rejecting it here means the error names the cause instead of
/// a payment arriving short.
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

/// The ways this deployment can actually take money.
pub fn registry_from_env() -> crate::services::collect::CollectionRegistry {
    use std::sync::Arc;

    let mut registry = crate::services::collect::CollectionRegistry::new();

    if let Some(cfg) = crate::services::stripe::StripeConfig::from_env() {
        registry.register(Arc::new(StripeCollect { cfg }));
    }

    if let Some(cfg) = crate::services::fedapay::FedaPayConfig::from_env() {
        let base = std::env::var("FRONTEND_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::config::PUBLIC_SITE_URL.to_string());
        registry.register(Arc::new(FedaPayCollect {
            cfg,
            callback_url: format!("{}/payment/return", base.trim_end_matches('/')),
        }));
    }

    registry
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn nobody_claims_a_corridor_it_cannot_serve() {
        let stripe = StripeCollect {
            cfg: crate::services::stripe::StripeConfig {
                secret_key: "sk".into(),
                webhook_secret: "wh".into(),
                success_url: "s".into(),
                cancel_url: "c".into(),
            },
        };
        assert!(stripe.supports(Currency::Eur, Method::Card));
        // The two that would route a Beninese payer into a dead end.
        assert!(!stripe.supports(Currency::Xof, Method::MobileMoney));
        assert!(!stripe.supports(Currency::Xof, Method::Card));

        let fedapay = FedaPayCollect {
            cfg: crate::services::fedapay::FedaPayConfig {
                secret_key: "sk".into(),
                live: false,
            },
            callback_url: "https://skill-uv.com/payment/return".into(),
        };
        assert!(fedapay.supports(Currency::Xof, Method::MobileMoney));
        assert!(!fedapay.supports(Currency::Eur, Method::Card));
    }

    #[test]
    fn xof_refuses_a_fraction_of_a_franc() {
        use std::str::FromStr;
        let whole = BigDecimal::from_str("5000").unwrap();
        assert_eq!(to_minor_units(&whole, Currency::Xof).unwrap(), 5000);

        let fractional = BigDecimal::from_str("5000.50").unwrap();
        assert!(
            to_minor_units(&fractional, Currency::Xof).is_err(),
            "the franc CFA has no subdivision; truncating it pays someone short"
        );

        let euros = BigDecimal::from_str("42.50").unwrap();
        assert_eq!(to_minor_units(&euros, Currency::Eur).unwrap(), 4250);
    }
}
