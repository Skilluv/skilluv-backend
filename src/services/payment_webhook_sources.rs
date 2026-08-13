//! One [`Source`] per provider. Adding one is adding an impl here.
//!
//! Each knows three things and nothing else: how the provider signs, where
//! it puts its event id, and which of its event names mean a payout
//! settled or failed. None of them touches the ledger — that is
//! [`crate::services::payment_webhooks::receive`]'s job, and keeping the
//! split is what stops a provider from inventing its own way to move money.

use serde_json::Value;

use crate::errors::AppError;
use crate::services::payment_webhooks::{Event, Source};

/// Stripe: transfers and payouts on Connect accounts.
///
/// `account.updated`, the KYC event, is handled separately in
/// `routes::talent_wallet` and reaches here only to be ignored.
pub struct StripeSource {
    pub webhook_secret: String,
}

impl Source for StripeSource {
    fn name(&self) -> &'static str {
        "stripe"
    }

    fn verify(&self, body: &str, signature: Option<&str>) -> Result<(), AppError> {
        let signature = signature.ok_or(AppError::Unauthorized)?;
        // Five minutes, matching the Connect endpoint. A replayed body older
        // than that is refused even with a valid signature.
        crate::services::stripe::verify_webhook_signature(
            &self.webhook_secret,
            body.as_bytes(),
            signature,
            300,
        )
    }

    fn event_id(&self, payload: &Value) -> Option<String> {
        payload
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn normalise(&self, payload: &Value) -> Result<Event, AppError> {
        let kind = payload
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("stripe event has no type".into()))?;

        let object = payload
            .get("data")
            .and_then(|d| d.get("object"))
            .ok_or_else(|| AppError::Validation("stripe event has no data.object".into()))?;
        let reference = object
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        Ok(match kind {
            // `payout.paid` is the money reaching the recipient's bank.
            // `transfer.created` only moves it between Stripe balances,
            // which `send` already recorded, so it is not a settlement.
            "payout.paid" => Event::PayoutSettled { reference },
            "payout.failed" | "transfer.reversed" => Event::PayoutFailed {
                reason: object
                    .get("failure_message")
                    .or_else(|| object.get("failure_code"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                reference,
            },
            other => Event::Ignored {
                kind: other.to_string(),
            },
        })
    }
}

/// FedaPay: Mobile Money across West Africa.
///
/// Signs with an HMAC-SHA256 of the raw body under the endpoint secret,
/// in `X-FEDAPAY-SIGNATURE`, in the same `t=…,s=…` shape Stripe uses.
pub struct FedaPaySource {
    pub webhook_secret: String,
}

impl Source for FedaPaySource {
    fn name(&self) -> &'static str {
        "fedapay"
    }

    fn verify(&self, body: &str, signature: Option<&str>) -> Result<(), AppError> {
        let signature = signature.ok_or(AppError::Unauthorized)?;
        verify_hmac_sha256(&self.webhook_secret, body, signature)
    }

    fn event_id(&self, payload: &Value) -> Option<String> {
        payload.get("id").and_then(|v| {
            v.as_str()
                .map(str::to_string)
                .or_else(|| Some(v.to_string()))
        })
    }

    fn normalise(&self, payload: &Value) -> Result<Event, AppError> {
        let kind = payload
            .get("name")
            .or_else(|| payload.get("event"))
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("fedapay event has no name".into()))?;

        // FedaPay nests the subject under `entity`, or `data.entity` in
        // some deliveries. Both shapes reach production.
        let entity = payload
            .get("entity")
            .or_else(|| payload.get("data").and_then(|d| d.get("entity")))
            .ok_or_else(|| AppError::Validation("fedapay event has no entity".into()))?;

        // The reference our `payouts` row holds is FedaPay's numeric id,
        // stored as a string by the adapter.
        let reference = entity
            .get("id")
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default();

        Ok(match kind {
            "payout.sent" | "payout.succeeded" => Event::PayoutSettled { reference },
            "payout.failed" | "payout.canceled" => Event::PayoutFailed {
                reason: entity
                    .get("last_error_message")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                reference,
            },
            other => Event::Ignored {
                kind: other.to_string(),
            },
        })
    }
}

/// The Mobile Money operators, behind one shared callback secret.
///
/// Orange, MTN and Wave each have their own callback format in production;
/// what they share is that none of them can be polled, so a payout here is
/// only ever resolved by what arrives on this endpoint. The operator is
/// taken from the path, not from the body, because the body is the part an
/// attacker controls.
pub struct MobileMoneySource {
    pub operator: &'static str,
    pub webhook_secret: String,
}

impl Source for MobileMoneySource {
    fn name(&self) -> &'static str {
        self.operator
    }

    fn verify(&self, body: &str, signature: Option<&str>) -> Result<(), AppError> {
        let signature = signature.ok_or(AppError::Unauthorized)?;
        verify_hmac_sha256(&self.webhook_secret, body, signature)
    }

    fn event_id(&self, payload: &Value) -> Option<String> {
        payload
            .get("transaction_id")
            .or_else(|| payload.get("transactionId"))
            .or_else(|| payload.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn normalise(&self, payload: &Value) -> Result<Event, AppError> {
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("mobile money callback has no status".into()))?;
        let reference = payload
            .get("transaction_id")
            .or_else(|| payload.get("transactionId"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Operators disagree on capitalisation and on which word they use
        // for the same outcome, and one of them will add another. Matching
        // lowercase on the whole known set beats matching one spelling.
        Ok(match status.to_ascii_lowercase().as_str() {
            "success" | "successful" | "completed" | "sent" | "paid" => {
                Event::PayoutSettled { reference }
            }
            "failed" | "failure" | "rejected" | "cancelled" | "canceled" | "expired" => {
                Event::PayoutFailed {
                    reason: payload
                        .get("message")
                        .or_else(|| payload.get("reason"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    reference,
                }
            }
            other => Event::Ignored {
                kind: other.to_string(),
            },
        })
    }
}

/// HMAC-SHA256 over the raw body, hex-encoded, compared in constant time.
///
/// Accepts both a bare hex digest and the `t=…,s=…` form, because providers
/// use both and the difference is not worth a second implementation.
fn verify_hmac_sha256(secret: &str, body: &str, signature: &str) -> Result<(), AppError> {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    if secret.is_empty() {
        // A deployment with no secret must reject, never wave events
        // through: an endpoint that accepts anything when misconfigured is
        // worse than one that is down.
        return Err(AppError::Unauthorized);
    }

    let provided = signature
        .split(',')
        .filter_map(|pair| pair.split_once('=').map(|(_, v)| v.trim()))
        .next_back()
        .unwrap_or(signature.trim());

    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .map_err(|_| AppError::Internal("hmac init failed".into()))?;
    mac.update(body.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());

    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

/// Comparison that does not leak how much of the signature was right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The sources this deployment is configured for.
///
/// Same rule as the payout registry: a provider whose secret is absent is
/// not here, so its endpoint answers 404 rather than accepting unverifiable
/// events.
pub fn sources_from_env() -> Vec<Box<dyn Source>> {
    let mut sources: Vec<Box<dyn Source>> = Vec::new();

    if let Ok(secret) = std::env::var("STRIPE_WEBHOOK_SECRET")
        && !secret.is_empty()
    {
        sources.push(Box::new(StripeSource {
            webhook_secret: secret,
        }));
    }

    if let Ok(secret) = std::env::var("FEDAPAY_WEBHOOK_SECRET")
        && !secret.is_empty()
    {
        sources.push(Box::new(FedaPaySource {
            webhook_secret: secret,
        }));
    }

    if let Ok(secret) = std::env::var("MOMO_WEBHOOK_SECRET")
        && !secret.is_empty()
    {
        for operator in ["orange", "mtn", "wave"] {
            sources.push(Box::new(MobileMoneySource {
                operator,
                webhook_secret: secret.clone(),
            }));
        }
    }

    sources
}

#[cfg(test)]
mod unit {
    use super::*;
    use serde_json::json;

    fn hmac_of(secret: &str, body: &str) -> String {
        use hmac::{Hmac, KeyInit, Mac};
        use sha2::Sha256;
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn a_missing_secret_rejects_rather_than_accepts() {
        let body = "{}";
        assert!(verify_hmac_sha256("", body, &hmac_of("", body)).is_err());
    }

    #[test]
    fn a_valid_signature_passes_in_both_shapes() {
        let body = r#"{"name":"payout.sent"}"#;
        let digest = hmac_of("shh", body);
        assert!(verify_hmac_sha256("shh", body, &digest).is_ok());
        assert!(verify_hmac_sha256("shh", body, &format!("t=1,s={digest}")).is_ok());
    }

    #[test]
    fn a_tampered_body_fails() {
        let digest = hmac_of("shh", r#"{"amount":10}"#);
        assert!(verify_hmac_sha256("shh", r#"{"amount":1000}"#, &digest).is_err());
    }

    #[test]
    fn fedapay_reads_both_envelope_shapes() {
        let source = FedaPaySource {
            webhook_secret: "s".into(),
        };
        let flat = json!({ "name": "payout.sent", "entity": { "id": 42 } });
        assert_eq!(
            source.normalise(&flat).unwrap(),
            Event::PayoutSettled {
                reference: "42".into()
            }
        );

        let nested = json!({
            "name": "payout.failed",
            "data": { "entity": { "id": 43, "last_error_message": "unknown number" } }
        });
        assert_eq!(
            source.normalise(&nested).unwrap(),
            Event::PayoutFailed {
                reference: "43".into(),
                reason: Some("unknown number".into())
            }
        );
    }

    #[test]
    fn stripe_does_not_treat_a_transfer_as_an_arrival() {
        let source = StripeSource {
            webhook_secret: "s".into(),
        };
        // A transfer moves money between Stripe balances; `send` already
        // recorded that. Only `payout.paid` means the recipient has it.
        let transfer = json!({
            "id": "evt_1", "type": "transfer.created",
            "data": { "object": { "id": "tr_1" } }
        });
        assert!(matches!(
            source.normalise(&transfer).unwrap(),
            Event::Ignored { .. }
        ));

        let paid = json!({
            "id": "evt_2", "type": "payout.paid",
            "data": { "object": { "id": "po_1" } }
        });
        assert_eq!(
            source.normalise(&paid).unwrap(),
            Event::PayoutSettled {
                reference: "po_1".into()
            }
        );
    }

    #[test]
    fn operators_disagreeing_on_wording_still_land_in_one_place() {
        let source = MobileMoneySource {
            operator: "mtn",
            webhook_secret: "s".into(),
        };
        for word in ["SUCCESS", "successful", "Completed", "sent", "paid"] {
            let payload = json!({ "status": word, "transaction_id": "t1" });
            assert!(
                matches!(
                    source.normalise(&payload).unwrap(),
                    Event::PayoutSettled { .. }
                ),
                "{word} should settle"
            );
        }
        for word in ["FAILED", "rejected", "Cancelled", "expired"] {
            let payload = json!({ "status": word, "transaction_id": "t1" });
            assert!(
                matches!(
                    source.normalise(&payload).unwrap(),
                    Event::PayoutFailed { .. }
                ),
                "{word} should fail"
            );
        }
        // An unknown word is never guessed at.
        let unknown = json!({ "status": "processing", "transaction_id": "t1" });
        assert!(matches!(
            source.normalise(&unknown).unwrap(),
            Event::Ignored { .. }
        ));
    }
}
