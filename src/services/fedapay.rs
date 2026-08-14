//! FedaPay HTTP client — Mobile Money payouts in West Africa.
//!
//! Exists because Stripe cannot pay out to Benin, Togo, Burkina, Niger or
//! Guinea at all, and our own Mobile Money adapters talk to one operator
//! each. FedaPay reaches MTN, Moov, Togocel, Orange and Wave across eight
//! countries behind one credential, which makes it the sensible fallback
//! wherever a direct operator integration is missing.
//!
//! Two calls, because FedaPay separates them:
//!
//! 1. `POST /v1/payouts` creates the payout in `pending`. Nothing has moved.
//! 2. `PUT /v1/payouts/start` releases it.
//!
//! A payout created and never started is money that stays put and silently
//! never arrives, so [`send_payout`] always does both and reports the
//! failure of either.

use serde_json::{Value, json};

use crate::errors::AppError;

const LIVE_API: &str = "https://api.fedapay.com/v1";
const SANDBOX_API: &str = "https://sandbox-api.fedapay.com/v1";

/// Credentials and environment for this deployment.
#[derive(Debug, Clone)]
pub struct FedaPayConfig {
    pub secret_key: String,
    /// Sandbox unless `FEDAPAY_ENV=live`. Defaulting to sandbox is
    /// deliberate: a missing variable must never mean "send real money".
    pub live: bool,
}

impl FedaPayConfig {
    pub fn from_env() -> Option<Self> {
        let secret_key = std::env::var("FEDAPAY_SECRET_KEY")
            .ok()
            .filter(|s| !s.is_empty())?;
        let live = std::env::var("FEDAPAY_ENV")
            .map(|v| v.eq_ignore_ascii_case("live"))
            .unwrap_or(false);
        Some(Self { secret_key, live })
    }

    fn base(&self) -> &'static str {
        if self.live { LIVE_API } else { SANDBOX_API }
    }
}

/// What FedaPay says about a payout.
#[derive(Debug, Clone)]
pub struct FedaPayPayout {
    pub id: i64,
    /// `pending`, `started`, `processing`, `sent` or `failed`.
    pub status: String,
    pub message: Option<String>,
}

/// Create the payout and release it.
///
/// `phone` is in E.164 and `country` is the ISO 3166-1 alpha-2 code,
/// lowercased as FedaPay expects it. `mode` is deliberately not sent:
/// FedaPay derives the operator from the number itself, and a mode we
/// guessed wrong is a payout refused for a reason the recipient cannot act
/// on. Naming the operator is only worth it once we have a reason to
/// override that detection.
pub struct Transfer<'a> {
    /// In the currency's smallest unit — XOF has none, so whole francs.
    pub amount: i64,
    pub currency_iso: &'a str,
    /// E.164, as the operator knows the wallet.
    pub phone: &'a str,
    /// ISO 3166-1 alpha-2. Lowercased before sending.
    pub country: &'a str,
    pub recipient_name: &'a str,
    pub recipient_email: Option<&'a str>,
    pub description: &'a str,
    pub idempotency_key: &'a str,
}

pub async fn send_payout(
    cfg: &FedaPayConfig,
    transfer: &Transfer<'_>,
) -> Result<FedaPayPayout, AppError> {
    let Transfer {
        amount,
        currency_iso,
        phone,
        country,
        recipient_name,
        recipient_email,
        description,
        idempotency_key,
    } = transfer;
    let client = reqwest::Client::new();
    let (firstname, lastname) = split_name(recipient_name);

    let mut customer = json!({
        "firstname": firstname,
        "lastname": lastname,
        "phone_number": { "number": phone, "country": country.to_lowercase() },
    });
    if let Some(email) = *recipient_email {
        customer["email"] = json!(email);
    }

    let created: Value = post(
        &client,
        cfg,
        "/payouts",
        &json!({
            "amount": amount,
            "currency": { "iso": currency_iso },
            "description": description,
            "customer": customer,
        }),
        idempotency_key,
    )
    .await?;

    let payout = unwrap_payout(&created).ok_or_else(|| {
        AppError::Internal(format!("fedapay: unexpected create response: {created}"))
    })?;
    let id = payout
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Internal(format!("fedapay: payout has no id: {payout}")))?;

    // Created but not started is money that never leaves. If this call
    // fails the payout stays `pending` on FedaPay's side, which the caller
    // reverses — and the id is in the error so it can be found there.
    let started: Value = put(
        &client,
        cfg,
        "/payouts/start",
        &json!({ "payouts": [{ "id": id }] }),
        idempotency_key,
    )
    .await
    .map_err(|e| {
        AppError::Internal(format!("fedapay: payout {id} created but not started: {e}"))
    })?;

    let after = unwrap_payout(&started).unwrap_or(payout);
    Ok(FedaPayPayout {
        id,
        status: after
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending")
            .to_string(),
        message: after
            .get("last_error_message")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Open a checkout the payer completes on FedaPay's page.
///
/// Two calls, like the payout side: `POST /transactions` creates it and
/// `POST /transactions/{id}/token` produces the URL to send someone to.
/// A transaction with no token is a charge that exists and that nobody can
/// pay, so this always does both.
pub async fn create_checkout(
    cfg: &FedaPayConfig,
    transfer: &Transfer<'_>,
    callback_url: &str,
) -> Result<(String, String), AppError> {
    let client = reqwest::Client::new();
    let (firstname, lastname) = split_name(transfer.recipient_name);

    let mut customer = json!({
        "firstname": firstname,
        "lastname": lastname,
        "phone_number": {
            "number": transfer.phone,
            "country": transfer.country.to_lowercase(),
        },
    });
    if let Some(email) = transfer.recipient_email {
        customer["email"] = json!(email);
    }

    let created = post(
        &client,
        cfg,
        "/transactions",
        &json!({
            "description": transfer.description,
            "amount": transfer.amount,
            "currency": { "iso": transfer.currency_iso },
            "callback_url": callback_url,
            "customer": customer,
        }),
        transfer.idempotency_key,
    )
    .await?;

    let transaction = created
        .get("v1/transaction")
        .or_else(|| created.get("transaction"))
        .unwrap_or(&created);
    let id = transaction
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Internal(format!("fedapay: transaction has no id: {created}")))?;

    let tokenised = post(
        &client,
        cfg,
        &format!("/transactions/{id}/token"),
        &json!({}),
        transfer.idempotency_key,
    )
    .await?;

    let url = tokenised
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::Internal(format!(
                "fedapay: transaction {id} has no payment url — the payer has nowhere to go: {tokenised}"
            ))
        })?
        .to_string();

    Ok((id.to_string(), url))
}

/// Read back what became of a payout.
///
/// The reconciliation sweep calls this for payouts whose callback never
/// arrived, which is the majority of the ones it looks at.
pub async fn payout_status(cfg: &FedaPayConfig, id: &str) -> Result<String, AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/payouts/{id}", cfg.base()))
        .bearer_auth(&cfg.secret_key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("fedapay payout {id}: {e}")))?;
    let body = decode(resp, "/payouts/{id}").await?;
    let payout = unwrap_payout(&body)
        .ok_or_else(|| AppError::Internal(format!("fedapay: unexpected payout body: {body}")))?;
    Ok(payout
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending")
        .to_string())
}

/// FedaPay wraps its objects under a versioned key, and lists under the
/// plural of it. Both shapes reach here, from create and from start.
fn unwrap_payout(body: &Value) -> Option<&Value> {
    if let Some(one) = body.get("v1/payout") {
        return Some(one);
    }
    if let Some(first) = body.get("v1/payouts").and_then(|v| v.as_array()?.first()) {
        return Some(first);
    }
    // A bare object, should the wrapper ever be dropped.
    body.get("id").map(|_| body)
}

/// First word is the given name, the rest is the family name.
///
/// Crude, and correct enough for a field FedaPay only uses to label the
/// transfer. A single-word name gives the same value twice rather than an
/// empty `lastname`, which their API rejects.
fn split_name(full: &str) -> (String, String) {
    let trimmed = full.trim();
    if trimmed.is_empty() {
        return ("Skilluv".into(), "Talent".into());
    }
    match trimmed.split_once(char::is_whitespace) {
        Some((first, rest)) => (first.to_string(), rest.trim().to_string()),
        None => (trimmed.to_string(), trimmed.to_string()),
    }
}

async fn post(
    client: &reqwest::Client,
    cfg: &FedaPayConfig,
    path: &str,
    body: &Value,
    idempotency_key: &str,
) -> Result<Value, AppError> {
    let resp = client
        .post(format!("{}{path}", cfg.base()))
        .bearer_auth(&cfg.secret_key)
        .header("X-Idempotency-Key", idempotency_key)
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("fedapay {path}: {e}")))?;
    decode(resp, path).await
}

async fn put(
    client: &reqwest::Client,
    cfg: &FedaPayConfig,
    path: &str,
    body: &Value,
    idempotency_key: &str,
) -> Result<Value, AppError> {
    let resp = client
        .put(format!("{}{path}", cfg.base()))
        .bearer_auth(&cfg.secret_key)
        .header("X-Idempotency-Key", idempotency_key)
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("fedapay {path}: {e}")))?;
    decode(resp, path).await
}

async fn decode(resp: reqwest::Response, path: &str) -> Result<Value, AppError> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        // The body carries the reason — a phone number the operator does not
        // know, a balance too low. Losing it leaves nothing to act on.
        return Err(AppError::Internal(format!(
            "fedapay {path} failed {status}: {text}"
        )));
    }
    serde_json::from_str(&text)
        .map_err(|e| AppError::Internal(format!("fedapay {path} decode: {e}: {text}")))
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn sandbox_unless_explicitly_live() {
        let sandboxed = FedaPayConfig {
            secret_key: "k".into(),
            live: false,
        };
        assert_eq!(sandboxed.base(), SANDBOX_API);
        let live = FedaPayConfig {
            secret_key: "k".into(),
            live: true,
        };
        assert_eq!(live.base(), LIVE_API);
    }

    #[test]
    fn a_name_always_yields_two_non_empty_parts() {
        for name in [
            "Awa Diallo",
            "Awa",
            "  Awa   Diallo  ",
            "",
            "Jean Marc Kouassi",
        ] {
            let (first, last) = split_name(name);
            assert!(!first.is_empty(), "{name:?} gave an empty firstname");
            assert!(!last.is_empty(), "{name:?} gave an empty lastname");
        }
        assert_eq!(
            split_name("Jean Marc Kouassi"),
            ("Jean".to_string(), "Marc Kouassi".to_string())
        );
    }

    #[test]
    fn both_response_shapes_are_understood() {
        let single = json!({ "v1/payout": { "id": 23, "status": "pending" } });
        assert_eq!(unwrap_payout(&single).unwrap()["id"], 23);

        let list = json!({ "v1/payouts": [{ "id": 24, "status": "sent" }] });
        assert_eq!(unwrap_payout(&list).unwrap()["status"], "sent");

        let bare = json!({ "id": 25, "status": "failed" });
        assert_eq!(unwrap_payout(&bare).unwrap()["id"], 25);

        assert!(unwrap_payout(&json!({ "error": "nope" })).is_none());
    }
}
