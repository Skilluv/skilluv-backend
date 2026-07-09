//! Paystack + Flutterwave adapters — Phase 4.2 + 4.3.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::{Sha256, Sha512};

use crate::errors::AppError;
use crate::services::psp::{CheckoutParams, CheckoutSession, PaymentProvider, RefundResult, WebhookEvent};

#[allow(dead_code)] // kept for Paystack HMAC-SHA256 verification wiring
type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

// ─── Paystack ─────────────────────────────────────────────────────

const PAYSTACK_API: &str = "https://api.paystack.co";

#[derive(Clone)]
pub struct PaystackConfig {
    pub secret_key: String,
    pub callback_url: String,
}

impl PaystackConfig {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            secret_key: std::env::var("PAYSTACK_SECRET_KEY")
                .ok()
                .filter(|s| !s.is_empty())?,
            callback_url: std::env::var("PAYSTACK_CALLBACK_URL")
                .unwrap_or_else(|_| "https://skilluv.com/enterprise/credits/success".into()),
        })
    }
}

#[derive(Clone)]
pub struct PaystackProvider {
    pub cfg: PaystackConfig,
}

#[derive(Deserialize)]
struct PaystackInitResponse {
    status: bool,
    message: String,
    data: Option<PaystackInitData>,
}

#[derive(Deserialize)]
struct PaystackInitData {
    reference: String,
    authorization_url: String,
}

#[async_trait]
impl PaymentProvider for PaystackProvider {
    fn name(&self) -> &'static str {
        "paystack"
    }

    fn supported_currencies(&self) -> &'static [&'static str] {
        &["NGN", "GHS", "EGP", "ZAR", "USD"]
    }

    fn supported_country_codes(&self) -> &'static [&'static str] {
        &["NG", "GH", "EG", "ZA"]
    }

    async fn create_checkout_session(
        &self,
        params: CheckoutParams<'_>,
    ) -> Result<CheckoutSession, AppError> {
        let client = reqwest::Client::new();
        let mut body = serde_json::Map::new();
        body.insert("email".into(), serde_json::json!(params.customer_email));
        body.insert("amount".into(), serde_json::json!(params.amount_cents));
        body.insert("currency".into(), serde_json::json!(params.currency.to_uppercase()));
        body.insert("callback_url".into(), serde_json::json!(self.cfg.callback_url));
        body.insert(
            "reference".into(),
            serde_json::json!(format!(
                "skl-{}-{}",
                params.pack_slug,
                uuid::Uuid::new_v4().simple()
            )),
        );
        let mut metadata = serde_json::Map::new();
        metadata.insert("pack_slug".into(), serde_json::json!(params.pack_slug));
        metadata.insert("credit_count".into(), serde_json::json!(params.pack_credits));
        metadata.insert(
            "client_reference_id".into(),
            serde_json::json!(params.client_reference_id),
        );
        for (k, v) in &params.metadata {
            metadata.insert(k.to_string(), serde_json::json!(v));
        }
        body.insert("metadata".into(), serde_json::Value::Object(metadata));

        let resp = client
            .post(format!("{PAYSTACK_API}/transaction/initialize"))
            .bearer_auth(&self.cfg.secret_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("paystack init: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "paystack init failed {status}: {text}"
            )));
        }
        let parsed: PaystackInitResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("paystack decode: {e}")))?;
        if !parsed.status {
            return Err(AppError::Internal(format!(
                "paystack refused: {}",
                parsed.message
            )));
        }
        let data = parsed
            .data
            .ok_or(AppError::Internal("paystack response missing data".into()))?;
        Ok(CheckoutSession {
            provider: "paystack",
            session_id: data.reference,
            checkout_url: data.authorization_url,
        })
    }

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent, AppError> {
        // Paystack signs the raw body with HMAC-SHA512 using the secret key, in the
        // `x-paystack-signature` header.
        let mut mac = <HmacSha512 as Mac>::new_from_slice(self.cfg.secret_key.as_bytes())
            .map_err(|_| AppError::Internal("paystack hmac init".into()))?;
        mac.update(payload);
        let expected = hex::encode(mac.finalize().into_bytes());
        if !constant_time_eq(expected.as_bytes(), signature.as_bytes()) {
            return Err(AppError::Unauthorized);
        }
        let raw: serde_json::Value = serde_json::from_slice(payload)
            .map_err(|e| AppError::Internal(format!("paystack payload decode: {e}")))?;
        let event_type = raw
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let data = raw
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        // Paystack uses `data.reference` as the transaction id — use that as the event id.
        let id = data
            .get("reference")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(WebhookEvent {
            id,
            event_type,
            object: data,
        })
    }

    async fn refund(&self, payment_id: &str, amount_cents: Option<i64>) -> Result<RefundResult, AppError> {
        let client = reqwest::Client::new();
        let mut body = serde_json::Map::new();
        body.insert("transaction".into(), serde_json::json!(payment_id));
        if let Some(a) = amount_cents {
            body.insert("amount".into(), serde_json::json!(a));
        }
        let resp = client
            .post(format!("{PAYSTACK_API}/refund"))
            .bearer_auth(&self.cfg.secret_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("paystack refund: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "paystack refund failed {status}: {text}"
            )));
        }
        Ok(RefundResult {
            provider: "paystack",
            refund_id: format!("paystack:{payment_id}"),
        })
    }
}

// ─── Flutterwave ─────────────────────────────────────────────────

const FLUTTERWAVE_API: &str = "https://api.flutterwave.com/v3";

#[derive(Clone)]
pub struct FlutterwaveConfig {
    pub secret_key: String,
    pub secret_hash: String, // Used to verify webhook via `verif-hash` header
    pub redirect_url: String,
}

impl FlutterwaveConfig {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            secret_key: std::env::var("FLUTTERWAVE_SECRET_KEY")
                .ok()
                .filter(|s| !s.is_empty())?,
            secret_hash: std::env::var("FLUTTERWAVE_SECRET_HASH")
                .ok()
                .filter(|s| !s.is_empty())?,
            redirect_url: std::env::var("FLUTTERWAVE_REDIRECT_URL")
                .unwrap_or_else(|_| "https://skilluv.com/enterprise/credits/success".into()),
        })
    }
}

#[derive(Clone)]
pub struct FlutterwaveProvider {
    pub cfg: FlutterwaveConfig,
}

#[derive(Deserialize)]
struct FwInitResponse {
    status: String,
    message: String,
    data: Option<FwInitData>,
}

#[derive(Deserialize)]
struct FwInitData {
    link: String,
}

#[async_trait]
impl PaymentProvider for FlutterwaveProvider {
    fn name(&self) -> &'static str {
        "flutterwave"
    }

    fn supported_currencies(&self) -> &'static [&'static str] {
        &["USD", "EUR", "NGN", "GHS", "KES", "UGX", "TZS", "XOF", "XAF", "MAD", "EGP", "ZAR"]
    }

    fn supported_country_codes(&self) -> &'static [&'static str] {
        &[
            "SN", "CI", "BJ", "BF", "TG", "ML", "NE", "GW", // XOF
            "CM", "GA", "CG", "TD", "CF", "GQ",             // XAF
            "MA", "TN", "DZ",                                // Maghreb
            "KE", "UG", "TZ", "RW", "ET",                    // East Africa
            "NG", "GH",                                       // WA anglophone
        ]
    }

    async fn create_checkout_session(
        &self,
        params: CheckoutParams<'_>,
    ) -> Result<CheckoutSession, AppError> {
        let client = reqwest::Client::new();
        let tx_ref = format!("skl-{}-{}", params.pack_slug, uuid::Uuid::new_v4().simple());
        let mut body = serde_json::Map::new();
        body.insert("tx_ref".into(), serde_json::json!(tx_ref.clone()));
        // Flutterwave expects amount as a decimal number of currency units, not smallest unit.
        let amount = (params.amount_cents as f64) / 100.0;
        body.insert("amount".into(), serde_json::json!(amount));
        body.insert("currency".into(), serde_json::json!(params.currency.to_uppercase()));
        body.insert("redirect_url".into(), serde_json::json!(self.cfg.redirect_url));
        let mut customer = serde_json::Map::new();
        customer.insert("email".into(), serde_json::json!(params.customer_email));
        body.insert("customer".into(), serde_json::Value::Object(customer));
        let mut meta = serde_json::Map::new();
        meta.insert("pack_slug".into(), serde_json::json!(params.pack_slug));
        meta.insert("credit_count".into(), serde_json::json!(params.pack_credits));
        meta.insert(
            "client_reference_id".into(),
            serde_json::json!(params.client_reference_id),
        );
        for (k, v) in &params.metadata {
            meta.insert(k.to_string(), serde_json::json!(v));
        }
        body.insert("meta".into(), serde_json::Value::Object(meta));

        let resp = client
            .post(format!("{FLUTTERWAVE_API}/payments"))
            .bearer_auth(&self.cfg.secret_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("flutterwave init: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "flutterwave init failed {status}: {text}"
            )));
        }
        let parsed: FwInitResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("flutterwave decode: {e}")))?;
        if parsed.status != "success" {
            return Err(AppError::Internal(format!(
                "flutterwave refused: {}",
                parsed.message
            )));
        }
        let data = parsed
            .data
            .ok_or(AppError::Internal("flutterwave response missing data".into()))?;
        Ok(CheckoutSession {
            provider: "flutterwave",
            session_id: tx_ref,
            checkout_url: data.link,
        })
    }

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookEvent, AppError> {
        // Flutterwave doesn't sign the body ; it sends the `secret_hash` value in the
        // `verif-hash` header. Compare constant-time.
        if !constant_time_eq(signature.as_bytes(), self.cfg.secret_hash.as_bytes()) {
            return Err(AppError::Unauthorized);
        }
        let raw: serde_json::Value = serde_json::from_slice(payload)
            .map_err(|e| AppError::Internal(format!("flutterwave payload decode: {e}")))?;
        let event_type = raw
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("charge.completed")
            .to_string();
        let data = raw.get("data").cloned().unwrap_or(serde_json::Value::Null);
        let id = data
            .get("tx_ref")
            .or_else(|| data.get("id"))
            .and_then(|v| v.as_str().map(String::from).or_else(|| v.as_i64().map(|n| n.to_string())))
            .unwrap_or_default();
        Ok(WebhookEvent {
            id,
            event_type,
            object: data,
        })
    }

    async fn refund(&self, payment_id: &str, amount_cents: Option<i64>) -> Result<RefundResult, AppError> {
        let client = reqwest::Client::new();
        let mut body = serde_json::Map::new();
        if let Some(a) = amount_cents {
            body.insert("amount".into(), serde_json::json!((a as f64) / 100.0));
        }
        let resp = client
            .post(format!("{FLUTTERWAVE_API}/transactions/{payment_id}/refund"))
            .bearer_auth(&self.cfg.secret_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("flutterwave refund: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "flutterwave refund failed {status}: {text}"
            )));
        }
        Ok(RefundResult {
            provider: "flutterwave",
            refund_id: format!("flutterwave:{payment_id}"),
        })
    }
}

// ─── Shared helper ────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha512_paystack_expected_hex_length() {
        let mut mac = <HmacSha512 as Mac>::new_from_slice(b"secret").unwrap();
        mac.update(b"body");
        let hex = hex::encode(mac.finalize().into_bytes());
        assert_eq!(hex.len(), 128); // sha512 → 64 bytes → 128 hex chars
    }

    #[test]
    fn ct_eq_matches() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
