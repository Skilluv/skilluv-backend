//! Stripe integration — Phase 3 (3.8).
//!
//! Thin HTTP wrapper. We don't pull the `async-stripe` crate (heavy, generated SDK) ;
//! we only need 3 endpoints: create Checkout Session, verify webhook signature,
//! create Billing Portal session.

use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::errors::AppError;

type HmacSha256 = Hmac<Sha256>;

const STRIPE_API: &str = "https://api.stripe.com/v1";

// ─── Packs (single source of truth for pricing) ───────────────────

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Pack {
    pub slug: &'static str,
    pub credits: i32,
    pub price_eur_cents: i64,
    pub stripe_price_lookup_key: &'static str,
}

/// Default catalogue (cf. docs/monetization-strategy.md section 3).
pub const PACKS: &[Pack] = &[
    Pack {
        slug: "pack_1",
        credits: 1,
        price_eur_cents: 3_900,
        stripe_price_lookup_key: "skilluv_credits_pack_1",
    },
    Pack {
        slug: "pack_5",
        credits: 5,
        price_eur_cents: 16_900,
        stripe_price_lookup_key: "skilluv_credits_pack_5",
    },
    Pack {
        slug: "pack_20",
        credits: 20,
        price_eur_cents: 59_900,
        stripe_price_lookup_key: "skilluv_credits_pack_20",
    },
    Pack {
        slug: "pack_100",
        credits: 100,
        price_eur_cents: 249_900,
        stripe_price_lookup_key: "skilluv_credits_pack_100",
    },
];

pub fn pack_by_slug(slug: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.slug == slug)
}

// ─── Config ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct StripeConfig {
    pub secret_key: String,
    pub webhook_secret: String,
    pub success_url: String,
    pub cancel_url: String,
}

impl StripeConfig {
    pub fn from_env() -> Option<Self> {
        let secret_key = std::env::var("STRIPE_SECRET_KEY")
            .ok()
            .filter(|s| !s.is_empty())?;
        let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")
            .ok()
            .filter(|s| !s.is_empty())?;
        let success_url = std::env::var("STRIPE_SUCCESS_URL").unwrap_or_else(|_| {
            "https://skilluv.com/enterprise/credits/success?session_id={CHECKOUT_SESSION_ID}".into()
        });
        let cancel_url = std::env::var("STRIPE_CANCEL_URL")
            .unwrap_or_else(|_| "https://skilluv.com/enterprise/credits/canceled".into());
        Some(Self {
            secret_key,
            webhook_secret,
            success_url,
            cancel_url,
        })
    }
}

// ─── Create Checkout Session ─────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CheckoutSessionResponse {
    pub id: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutSession {
    pub session_id: String,
    pub checkout_url: String,
}

pub async fn create_checkout_session(
    cfg: &StripeConfig,
    pack: &Pack,
    customer_email: &str,
    client_reference_id: &str,
    metadata_pairs: &[(&str, String)],
) -> Result<CheckoutSession, AppError> {
    let client = reqwest::Client::new();
    let mut form: Vec<(String, String)> = Vec::new();
    form.push(("mode".into(), "payment".into()));
    form.push(("success_url".into(), cfg.success_url.clone()));
    form.push(("cancel_url".into(), cfg.cancel_url.clone()));
    form.push(("client_reference_id".into(), client_reference_id.into()));
    form.push(("customer_email".into(), customer_email.into()));
    form.push(("currency".into(), "eur".into()));
    form.push(("automatic_tax[enabled]".into(), "true".into()));
    form.push(("billing_address_collection".into(), "required".into()));
    // Single line item: 1 unit of the pack, dynamically priced (no need to pre-create Stripe prices).
    form.push(("line_items[0][quantity]".into(), "1".into()));
    form.push(("line_items[0][price_data][currency]".into(), "eur".into()));
    form.push((
        "line_items[0][price_data][unit_amount]".into(),
        pack.price_eur_cents.to_string(),
    ));
    form.push((
        "line_items[0][price_data][product_data][name]".into(),
        format!("Skilluv — {} crédit(s)", pack.credits),
    ));
    form.push((
        "line_items[0][price_data][product_data][description]".into(),
        format!(
            "Pack de {} crédit(s) — chacun débloque 1 prise de contact talent.",
            pack.credits
        ),
    ));
    form.push((
        "line_items[0][price_data][tax_behavior]".into(),
        "exclusive".into(),
    ));
    form.push(("metadata[pack_slug]".into(), pack.slug.into()));
    form.push(("metadata[credit_count]".into(), pack.credits.to_string()));
    for (k, v) in metadata_pairs {
        form.push((format!("metadata[{k}]"), v.clone()));
    }

    let resp = client
        .post(format!("{STRIPE_API}/checkout/sessions"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe send failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe checkout failed {status}: {body}"
        )));
    }
    let parsed: CheckoutSessionResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("stripe decode failed: {e}")))?;
    Ok(CheckoutSession {
        session_id: parsed.id,
        checkout_url: parsed.url.unwrap_or_default(),
    })
}

// ─── Webhook signature verification ───────────────────────────────

/// Verify a Stripe webhook signature header (`Stripe-Signature`).
/// Implements the v1 scheme as documented: https://stripe.com/docs/webhooks/signatures
pub fn verify_webhook_signature(
    webhook_secret: &str,
    payload: &[u8],
    sig_header: &str,
    tolerance_secs: i64,
) -> Result<(), AppError> {
    let mut timestamp: Option<i64> = None;
    let mut signatures: Vec<&str> = Vec::new();

    for pair in sig_header.split(',') {
        let mut kv = pair.splitn(2, '=');
        let k = kv.next().unwrap_or("").trim();
        let v = kv.next().unwrap_or("").trim();
        match k {
            "t" => timestamp = v.parse::<i64>().ok(),
            "v1" => signatures.push(v),
            _ => {}
        }
    }
    let ts = timestamp.ok_or_else(|| AppError::Unauthorized)?;
    let now = Utc::now().timestamp();
    if (now - ts).abs() > tolerance_secs {
        return Err(AppError::Unauthorized);
    }
    let signed_payload = format!("{ts}.{}", String::from_utf8_lossy(payload));
    let mut mac = <HmacSha256 as Mac>::new_from_slice(webhook_secret.as_bytes())
        .map_err(|_| AppError::Internal("hmac init failed".into()))?;
    mac.update(signed_payload.as_bytes());
    let expected = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected);
    if signatures
        .iter()
        .any(|s| constant_time_eq(s.as_bytes(), expected_hex.as_bytes()))
    {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ─── Webhook event shape ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: WebhookData,
}

#[derive(Debug, Deserialize)]
pub struct WebhookData {
    pub object: serde_json::Value,
}

// ─── Customer Portal (manage invoices, payment methods) ───────────

#[derive(Debug, Deserialize)]
struct PortalResponse {
    pub url: String,
}

// ─── Refund (Phase 5 - close the loop) ───────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RefundResponse {
    pub id: String,
    pub status: String,
    pub amount: i64,
}

/// Émet un refund Stripe. `payment_intent_id` ou `charge_id` acceptés (Stripe
/// accepte les deux via le paramètre `payment_intent`). Si `amount_cents` est
/// `None`, refund total.
pub async fn create_refund(
    cfg: &StripeConfig,
    payment_intent_id: &str,
    amount_cents: Option<i64>,
    reason: Option<&str>,
) -> Result<RefundResponse, AppError> {
    let client = reqwest::Client::new();
    let mut form: Vec<(&str, String)> = vec![("payment_intent", payment_intent_id.to_string())];
    if let Some(a) = amount_cents {
        form.push(("amount", a.to_string()));
    }
    if let Some(r) = reason {
        // Stripe reasons: 'duplicate' | 'fraudulent' | 'requested_by_customer'
        form.push(("reason", r.to_string()));
    }
    let resp = client
        .post(format!("{STRIPE_API}/refunds"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe refund failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe refund failed {status}: {body}"
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("stripe refund decode: {e}")))
}

// ─── Stripe Connect (Phase 5.11 - mentorship payouts) ────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectAccount {
    pub id: String,
    #[serde(default)]
    pub details_submitted: bool,
    #[serde(default)]
    pub charges_enabled: bool,
    #[serde(default)]
    pub payouts_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountLink {
    pub url: String,
    pub expires_at: i64,
}

/// Crée un compte Stripe Connect Express pour un mentor.
pub async fn create_connect_account(
    cfg: &StripeConfig,
    email: &str,
    country: &str,
) -> Result<ConnectAccount, AppError> {
    let client = reqwest::Client::new();
    let form = [
        ("type", "express"),
        ("country", country),
        ("email", email),
        ("capabilities[transfers][requested]", "true"),
        ("capabilities[card_payments][requested]", "true"),
    ];
    let resp = client
        .post(format!("{STRIPE_API}/accounts"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe connect create: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe connect create failed {status}: {body}"
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("connect decode: {e}")))
}

/// URL d'onboarding pour un compte Connect Express.
pub async fn create_account_link(
    cfg: &StripeConfig,
    account_id: &str,
    refresh_url: &str,
    return_url: &str,
) -> Result<AccountLink, AppError> {
    let client = reqwest::Client::new();
    let form = [
        ("account", account_id),
        ("refresh_url", refresh_url),
        ("return_url", return_url),
        ("type", "account_onboarding"),
    ];
    let resp = client
        .post(format!("{STRIPE_API}/account_links"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe account link: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe account link failed {status}: {body}"
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("link decode: {e}")))
}

/// Récupère l'état d'un compte Connect (charges/payouts enabled).
pub async fn retrieve_connect_account(
    cfg: &StripeConfig,
    account_id: &str,
) -> Result<ConnectAccount, AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{STRIPE_API}/accounts/{account_id}"))
        .basic_auth(&cfg.secret_key, Some(""))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe retrieve: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe retrieve failed {status}: {body}"
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("retrieve decode: {e}")))
}

/// Transfer de fonds vers un compte Connect. Utilisé pour libérer la part
/// mentor 80% après completion.
pub async fn create_transfer(
    cfg: &StripeConfig,
    destination_account: &str,
    amount_cents: i64,
    currency: &str,
    description: &str,
) -> Result<serde_json::Value, AppError> {
    let client = reqwest::Client::new();
    let form = [
        ("amount", amount_cents.to_string()),
        ("currency", currency.to_lowercase()),
        ("destination", destination_account.to_string()),
        ("description", description.to_string()),
    ];
    let resp = client
        .post(format!("{STRIPE_API}/transfers"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe transfer: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe transfer failed {status}: {body}"
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("transfer decode: {e}")))
}

// ─── Subscriptions (Phase 4.6) ───────────────────────────────────

/// Crée un checkout Stripe pour un abonnement récurrent (mode subscription).
/// `price_lookup_key` référence le prix Stripe pré-créé pour ce pack sub.
pub async fn create_subscription_checkout(
    cfg: &StripeConfig,
    price_lookup_key: &str,
    customer_email: &str,
    client_reference_id: &str,
    metadata: &[(&str, String)],
) -> Result<CheckoutSession, AppError> {
    let client = reqwest::Client::new();
    let mut form: Vec<(String, String)> = vec![
        ("mode".into(), "subscription".into()),
        ("customer_email".into(), customer_email.into()),
        ("client_reference_id".into(), client_reference_id.into()),
        ("success_url".into(), cfg.success_url.clone()),
        ("cancel_url".into(), cfg.cancel_url.clone()),
        ("line_items[0][quantity]".into(), "1".into()),
    ];
    // Lookup par lookup_key : Stripe résoudra vers le price_id correspondant.
    form.push(("line_items[0][price_data][currency]".into(), "eur".into()));
    form.push((
        "line_items[0][price_data][recurring][interval]".into(),
        "month".into(),
    ));
    form.push((
        "line_items[0][price_data][product_data][name]".into(),
        price_lookup_key.to_string(),
    ));
    for (k, v) in metadata {
        form.push((format!("metadata[{k}]"), v.clone()));
        form.push((format!("subscription_data[metadata][{k}]"), v.clone()));
    }
    let form_pairs: Vec<(&str, &str)> =
        form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let resp = client
        .post(format!("{STRIPE_API}/checkout/sessions"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&form_pairs)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe subscription checkout: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe sub checkout failed {status}: {body}"
        )));
    }
    let parsed: CheckoutSessionResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("stripe sub decode: {e}")))?;
    Ok(CheckoutSession {
        session_id: parsed.id,
        checkout_url: parsed.url.unwrap_or_default(),
    })
}

pub async fn create_billing_portal_session(
    cfg: &StripeConfig,
    customer_id: &str,
    return_url: &str,
) -> Result<String, AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{STRIPE_API}/billing_portal/sessions"))
        .basic_auth(&cfg.secret_key, Some(""))
        .form(&[("customer", customer_id), ("return_url", return_url)])
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("stripe portal failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "stripe portal failed {status}: {body}"
        )));
    }
    let parsed: PortalResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("stripe portal decode failed: {e}")))?;
    Ok(parsed.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_have_unique_slugs() {
        let mut seen = std::collections::HashSet::new();
        for p in PACKS {
            assert!(seen.insert(p.slug), "duplicate pack slug: {}", p.slug);
        }
    }

    #[test]
    fn pack_lookup_works() {
        assert_eq!(pack_by_slug("pack_5").unwrap().credits, 5);
        assert!(pack_by_slug("nope").is_none());
    }

    #[test]
    fn webhook_sig_valid() {
        let secret = "whsec_test";
        let payload = br#"{"id":"evt_test","type":"checkout.session.completed"}"#;
        let ts = Utc::now().timestamp();
        let signed = format!("{ts}.{}", String::from_utf8_lossy(payload));
        let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        let sig_hex = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={ts},v1={sig_hex}");
        assert!(verify_webhook_signature(secret, payload, &header, 300).is_ok());
    }

    #[test]
    fn webhook_sig_invalid_rejected() {
        let payload = b"{}";
        let header = "t=999,v1=deadbeef";
        assert!(verify_webhook_signature("whsec_test", payload, header, 300).is_err());
    }

    #[test]
    fn webhook_sig_expired_rejected() {
        let secret = "s";
        let ts = Utc::now().timestamp() - 999_999;
        let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        let signed = format!("{ts}.{{}}");
        mac.update(signed.as_bytes());
        let sig_hex = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={ts},v1={sig_hex}");
        assert!(verify_webhook_signature(secret, b"{}", &header, 300).is_err());
    }
}
