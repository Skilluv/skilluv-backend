//! Multi-PSP abstraction layer — Phase 4.1.
//!
//! Plug-in interface for payment service providers. Each provider (Stripe, Paystack,
//! Flutterwave, …) implements `PaymentProvider`. Selection happens dynamically per
//! enterprise (country + explicit override).

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

// ─── Public types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutSession {
    pub provider: &'static str,
    pub session_id: String,
    pub checkout_url: String,
}

#[derive(Debug, Clone)]
pub struct CheckoutParams<'a> {
    pub pack_slug: &'a str,
    pub pack_credits: i32,
    pub /* amount in the target currency's smallest unit (cents, kobo, …) */ amount_cents: i64,
    pub currency: &'a str,
    pub customer_email: &'a str,
    pub client_reference_id: &'a str,
    pub metadata: Vec<(&'a str, String)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookEvent {
    pub id: String,
    pub event_type: String,
    /// Provider-specific data blob normalised to JSON.
    pub object: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefundResult {
    pub provider: &'static str,
    pub refund_id: String,
}

// ─── Trait ───────────────────────────────────────────────────────

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn supported_currencies(&self) -> &'static [&'static str];
    fn supported_country_codes(&self) -> &'static [&'static str];

    async fn create_checkout_session(
        &self,
        params: CheckoutParams<'_>,
    ) -> Result<CheckoutSession, AppError>;

    /// Verify signature and return the parsed event.
    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent, AppError>;

    async fn refund(&self, payment_id: &str, amount_cents: Option<i64>) -> Result<RefundResult, AppError>;

    /// Optional billing portal URL (Stripe supports it, PSP Africa usually don't).
    async fn customer_portal_url(&self, _customer_id: &str, _return_url: &str) -> Result<Option<String>, AppError> {
        Ok(None)
    }
}

// ─── Registry ────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct PaymentRegistry {
    providers: Vec<Arc<dyn PaymentProvider>>,
}

impl PaymentRegistry {
    pub fn new() -> Self {
        Self { providers: Vec::new() }
    }

    pub fn register(&mut self, provider: Arc<dyn PaymentProvider>) {
        self.providers.push(provider);
    }

    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn PaymentProvider>> {
        self.providers.iter().find(|p| p.name() == name).cloned()
    }

    pub fn all(&self) -> &[Arc<dyn PaymentProvider>] {
        &self.providers
    }

    /// Choose the best provider for a given country + preferred currency.
    /// Falls back to the first registered provider (typically Stripe).
    pub fn resolve_for_country(&self, country_iso2: Option<&str>) -> Option<Arc<dyn PaymentProvider>> {
        if let Some(cc) = country_iso2 {
            let cc_upper = cc.to_uppercase();
            if let Some(p) = self
                .providers
                .iter()
                .find(|p| p.supported_country_codes().iter().any(|s| *s == cc_upper))
                .cloned()
            {
                return Some(p);
            }
        }
        self.providers.first().cloned()
    }
}

/// Mapping (country ISO2 → default provider name). Used by admin dashboards / analytics.
/// The actual dispatch goes through `resolve_for_country` on the registry.
pub const DEFAULT_PROVIDER_BY_COUNTRY: &[(&str, &str)] = &[
    // West Africa francophone (XOF)
    ("SN", "flutterwave"), ("CI", "flutterwave"), ("BJ", "flutterwave"),
    ("BF", "flutterwave"), ("TG", "flutterwave"), ("ML", "flutterwave"),
    ("NE", "flutterwave"), ("GW", "flutterwave"),
    // Central Africa (XAF)
    ("CM", "flutterwave"), ("GA", "flutterwave"), ("CG", "flutterwave"),
    ("TD", "flutterwave"), ("CF", "flutterwave"), ("GQ", "flutterwave"),
    // Maghreb
    ("MA", "flutterwave"), ("TN", "flutterwave"), ("DZ", "flutterwave"),
    // Nigeria + Ghana + Egypt: Paystack
    ("NG", "paystack"), ("GH", "paystack"), ("EG", "paystack"),
    // East Africa
    ("KE", "flutterwave"), ("UG", "flutterwave"), ("TZ", "flutterwave"),
    ("RW", "flutterwave"), ("ET", "flutterwave"),
    // South Africa
    ("ZA", "stripe"),
    // EU/UK/CA/US → Stripe
    ("FR", "stripe"), ("BE", "stripe"), ("CH", "stripe"), ("LU", "stripe"),
    ("DE", "stripe"), ("NL", "stripe"), ("ES", "stripe"), ("IT", "stripe"),
    ("PT", "stripe"), ("IE", "stripe"), ("AT", "stripe"), ("PL", "stripe"),
    ("GB", "stripe"), ("US", "stripe"), ("CA", "stripe"), ("AU", "stripe"),
];

pub fn default_provider_name_for_country(country_iso2: &str) -> &'static str {
    let cc = country_iso2.to_uppercase();
    DEFAULT_PROVIDER_BY_COUNTRY
        .iter()
        .find(|(c, _)| *c == cc)
        .map(|(_, p)| *p)
        .unwrap_or("stripe")
}

// ─── Stripe adapter (wraps existing services::stripe) ────────────

pub mod stripe_adapter {
    use super::*;

    #[derive(Clone)]
    pub struct StripeProvider {
        pub cfg: crate::services::stripe::StripeConfig,
    }

    #[async_trait]
    impl PaymentProvider for StripeProvider {
        fn name(&self) -> &'static str {
            "stripe"
        }

        fn supported_currencies(&self) -> &'static [&'static str] {
            &["EUR", "USD", "GBP", "CAD", "AUD", "ZAR", "CHF"]
        }

        fn supported_country_codes(&self) -> &'static [&'static str] {
            &[
                "FR", "BE", "CH", "LU", "DE", "NL", "ES", "IT", "PT", "IE", "AT", "PL",
                "GB", "US", "CA", "AU", "ZA",
            ]
        }

        async fn create_checkout_session(
            &self,
            params: CheckoutParams<'_>,
        ) -> Result<CheckoutSession, AppError> {
            // Delegate to the concrete Stripe helper. The pack argument is reconstructed
            // from the abstract params so this adapter stays stateless.
            let pack = crate::services::stripe::Pack {
                slug: leak(params.pack_slug.to_string()),
                credits: params.pack_credits,
                price_eur_cents: params.amount_cents, // pack's price in the target currency
                stripe_price_lookup_key: leak(format!("skilluv_credits_{}", params.pack_slug)),
            };
            let session = crate::services::stripe::create_checkout_session(
                &self.cfg,
                &pack,
                params.customer_email,
                params.client_reference_id,
                &params.metadata.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
            )
            .await?;
            Ok(CheckoutSession {
                provider: "stripe",
                session_id: session.session_id,
                checkout_url: session.checkout_url,
            })
        }

        fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent, AppError> {
            crate::services::stripe::verify_webhook_signature(
                &self.cfg.webhook_secret,
                payload,
                signature,
                300,
            )?;
            let event: crate::services::stripe::WebhookEvent = serde_json::from_slice(payload)
                .map_err(|e| AppError::Internal(format!("stripe webhook decode: {e}")))?;
            Ok(WebhookEvent {
                id: event.id,
                event_type: event.event_type,
                object: event.data.object,
            })
        }

        async fn refund(&self, _payment_id: &str, _amount_cents: Option<i64>) -> Result<RefundResult, AppError> {
            // Stripe refunds are usually issued from the dashboard for MVP ; the API endpoint
            // is /v1/refunds. Left as TODO for the automated flow.
            Err(AppError::Validation(
                "Automated Stripe refunds not implemented — issue via dashboard".into(),
            ))
        }

        async fn customer_portal_url(&self, customer_id: &str, return_url: &str) -> Result<Option<String>, AppError> {
            let url = crate::services::stripe::create_billing_portal_session(
                &self.cfg,
                customer_id,
                return_url,
            )
            .await?;
            Ok(Some(url))
        }
    }

    /// Leak a String into a &'static str. Only used to bridge from dynamic pack slugs
    /// to the static-str API of the legacy `Pack` struct. Cost: one small heap leak per
    /// checkout ; acceptable at the volumes we target and simpler than refactoring Pack.
    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
}
