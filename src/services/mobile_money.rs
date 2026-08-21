//! Mobile money payouts: Orange, MTN, Wave.
//!
//! ## What was wrong with this file
//!
//! Every one of the three providers returned a synthetic transaction id and
//! `Pending`, logged the word "stub", and called nothing. Orange checked
//! whether its credentials existed and then took the same branch either way.
//!
//! A payout recorded as initiated that never happened is the worst failure
//! this codebase can have. The contributor sees money on its way, the
//! reconciliation shows an outstanding transfer, and nobody finds out until
//! somebody asks where their fee went — weeks later, with a transaction id
//! that no provider has ever heard of.
//!
//! So the shape is inverted. A provider with credentials makes the call. A
//! provider without them returns [`PayoutStatus::Unconfigured`] and an error
//! the caller cannot mistake for success. There is no third state where
//! something looks initiated and is not.
//!
//! ## Idempotency
//!
//! Every request carries a key derived from the payout it settles, so a
//! retried call after a timeout does not send the money twice. Providers
//! differ in how they take it — a header, a field — which is why each
//! implementation names its own rather than sharing one.
//!
//! ## Compliance
//!
//! Below 100 000 XOF a verified phone is enough. Above it the payout is
//! refused here rather than at the provider, because their refusal arrives
//! after the ledger has already moved.

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use serde::Serialize;
use uuid::Uuid;

use crate::errors::AppError;

/// Above this, a verified phone is not enough on its own.
pub const KYC_LITE_CEILING_XOF: i64 = 100_000;

/// How long to wait on a provider before giving up.
///
/// Short, because the caller is holding a ledger transaction open. A timeout
/// is retried with the same idempotency key, which is safe; a long hang is
/// not.
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(20);

/// Providers supported by the public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProviderName {
    Orange,
    Mtn,
    Wave,
}

impl ProviderName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Orange => "orange",
            Self::Mtn => "mtn",
            Self::Wave => "wave",
        }
    }

    /// The environment variables this provider needs to be live.
    ///
    /// Named here so an operator can be told exactly what is missing rather
    /// than "not configured".
    pub fn required_env(&self) -> &'static [&'static str] {
        match self {
            Self::Orange => &["ORANGE_MONEY_API_KEY", "ORANGE_MONEY_BASE_URL"],
            Self::Mtn => &[
                "MTN_MOMO_API_KEY",
                "MTN_MOMO_SUBSCRIPTION_KEY",
                "MTN_MOMO_BASE_URL",
            ],
            Self::Wave => &["WAVE_API_KEY", "WAVE_BASE_URL"],
        }
    }
}

impl FromStr for ProviderName {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, AppError> {
        match s.to_lowercase().as_str() {
            "orange" | "orange_money" => Ok(Self::Orange),
            "mtn" | "mtn_momo" => Ok(Self::Mtn),
            "wave" => Ok(Self::Wave),
            _ => Err(AppError::Validation(format!(
                "unsupported provider '{s}' (expected orange, mtn, wave)"
            ))),
        }
    }
}

/// What a provider said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutStatus {
    /// Accepted by the provider. The money is on its way and a webhook or a
    /// poll will confirm it.
    Pending,
    /// Confirmed in the same call. Rare; some providers do it for small
    /// amounts.
    Completed,
    /// The provider refused.
    Failed,
    /// This deployment holds no credentials for that provider. Never
    /// returned as an `Ok`: the caller gets an error, because a payout that
    /// did not happen must not look like one that did.
    Unconfigured,
}

/// The result of a payout attempt.
#[derive(Debug, Clone, Serialize)]
pub struct PayoutResult {
    pub provider: ProviderName,
    /// The provider's own reference. Never synthesised: a reference we
    /// invented is a reference nobody can reconcile against.
    pub provider_txn_id: String,
    pub status: PayoutStatus,
    pub message: Option<String>,
}

pub struct PayoutParams<'a> {
    pub user_id: Uuid,
    pub phone: &'a str,
    pub amount: &'a BigDecimal,
    pub currency: &'a str,
    pub note: &'a str,
    /// A stable key for this payout, so a retry after a timeout cannot send
    /// the money twice. Supplied by the caller, which is the layer that
    /// knows what is being settled.
    pub idempotency_key: &'a str,
}

#[async_trait]
pub trait MobileMoneyProvider: Send + Sync {
    fn name(&self) -> ProviderName;
    async fn initiate_payout(&self, params: &PayoutParams<'_>) -> Result<PayoutResult, AppError>;
}

/// Whether a phone number is in the shape every one of these providers wants.
pub fn validate_e164(phone: &str) -> Result<(), AppError> {
    let ok = phone.starts_with('+')
        && phone.len() >= 8
        && phone.len() <= 16
        && phone[1..].chars().all(|c| c.is_ascii_digit());

    if ok {
        Ok(())
    } else {
        Err(AppError::Validation(
            "the number has to be in international form, starting with + and digits only".into(),
        ))
    }
}

/// Whether an amount clears the light-KYC ceiling.
///
/// Checked here rather than at the provider: their refusal arrives after the
/// ledger has already moved, and unwinding a posted transfer is harder than
/// not making it.
pub fn within_kyc_lite(amount: &BigDecimal, currency: &str, kyc_full: bool) -> bool {
    if kyc_full {
        return true;
    }
    if currency.to_uppercase() != "XOF" {
        // Anything not in XOF is outside what light KYC was defined for.
        return false;
    }
    use bigdecimal::num_traits::ToPrimitive;
    amount.to_i64().unwrap_or(i64::MAX) <= KYC_LITE_CEILING_XOF
}

/// Read a required credential, or say precisely what is missing.
fn credential(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The error a caller gets when a provider is not configured.
///
/// An error rather than a pending result, and this is the whole point of the
/// rewrite: the previous version returned something indistinguishable from a
/// real payout.
fn unconfigured(provider: ProviderName) -> AppError {
    let missing: Vec<&str> = provider
        .required_env()
        .iter()
        .copied()
        .filter(|name| credential(name).is_none())
        .collect();

    tracing::error!(
        provider = provider.as_str(),
        missing = ?missing,
        "mobile money payout attempted on a provider this deployment cannot reach"
    );
    metrics::counter!(
        "skilluv_mobile_money_unconfigured_total",
        "provider" => provider.as_str()
    )
    .increment(1);

    AppError::Internal(format!(
        "{} payouts are not configured on this deployment (missing: {}). No money has \
         moved and nothing has been recorded as sent.",
        provider.as_str(),
        missing.join(", ")
    ))
}

/// One HTTP client, built per call.
///
/// Payouts are rare and the timeout matters more than the connection reuse.
fn http() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .timeout(PROVIDER_TIMEOUT)
        .build()
        .map_err(|e| AppError::Internal(format!("could not build an HTTP client: {e}")))
}

/// Turn a provider's response into a result, or into an error that says what
/// happened.
async fn read_response(
    provider: ProviderName,
    response: reqwest::Response,
    id_field: &str,
) -> Result<PayoutResult, AppError> {
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .unwrap_or_else(|_| serde_json::json!({}));

    if !status.is_success() {
        let detail = body
            .get("message")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("no detail given");

        tracing::error!(
            provider = provider.as_str(),
            status = status.as_u16(),
            detail = detail,
            "mobile money provider refused a payout"
        );

        return Err(AppError::Internal(format!(
            "{} refused the payout ({}): {detail}",
            provider.as_str(),
            status.as_u16()
        )));
    }

    // The provider's own reference. Without it there is nothing to reconcile
    // against, so its absence is a failure rather than a warning.
    let txn_id = body
        .get(id_field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Internal(format!(
                "{} accepted the payout but returned no reference — it cannot be \
                 reconciled, so it is treated as failed",
                provider.as_str()
            ))
        })?
        .to_string();

    let completed = body
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("successful") || s.eq_ignore_ascii_case("completed"))
        .unwrap_or(false);

    Ok(PayoutResult {
        provider,
        provider_txn_id: txn_id,
        status: if completed {
            PayoutStatus::Completed
        } else {
            PayoutStatus::Pending
        },
        message: None,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Orange Money
// ═══════════════════════════════════════════════════════════════════

pub struct OrangeMoneyProvider;

#[async_trait]
impl MobileMoneyProvider for OrangeMoneyProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Orange
    }

    async fn initiate_payout(&self, params: &PayoutParams<'_>) -> Result<PayoutResult, AppError> {
        validate_e164(params.phone)?;
        if params.currency.to_uppercase() != "XOF" {
            return Err(AppError::Validation(
                "Orange Money settles in XOF only".into(),
            ));
        }

        let (Some(key), Some(base)) = (
            credential("ORANGE_MONEY_API_KEY"),
            credential("ORANGE_MONEY_BASE_URL"),
        ) else {
            return Err(unconfigured(ProviderName::Orange));
        };

        let response = http()?
            .post(format!("{}/cashout", base.trim_end_matches('/')))
            .bearer_auth(key)
            // The idempotency key is the payout it settles. A retry after a
            // timeout reaches the same record rather than sending twice.
            .header("X-Idempotency-Key", params.idempotency_key.to_string())
            .json(&serde_json::json!({
                "msisdn": params.phone,
                "amount": params.amount.to_string(),
                "currency": params.currency.to_uppercase(),
                "reference": params.idempotency_key.to_string(),
                "description": params.note,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Orange Money did not answer: {e}")))?;

        read_response(ProviderName::Orange, response, "transactionId").await
    }
}

// ═══════════════════════════════════════════════════════════════════
// MTN MoMo
// ═══════════════════════════════════════════════════════════════════

pub struct MtnMobileMoneyProvider;

#[async_trait]
impl MobileMoneyProvider for MtnMobileMoneyProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Mtn
    }

    async fn initiate_payout(&self, params: &PayoutParams<'_>) -> Result<PayoutResult, AppError> {
        validate_e164(params.phone)?;

        let (Some(key), Some(subscription), Some(base)) = (
            credential("MTN_MOMO_API_KEY"),
            credential("MTN_MOMO_SUBSCRIPTION_KEY"),
            credential("MTN_MOMO_BASE_URL"),
        ) else {
            return Err(unconfigured(ProviderName::Mtn));
        };

        let response = http()?
            .post(format!(
                "{}/disbursement/v1_0/transfer",
                base.trim_end_matches('/')
            ))
            .bearer_auth(key)
            .header("Ocp-Apim-Subscription-Key", subscription)
            // MTN takes the idempotency key as its reference id header.
            .header("X-Reference-Id", params.idempotency_key.to_string())
            .header(
                "X-Target-Environment",
                credential("MTN_MOMO_TARGET_ENVIRONMENT").unwrap_or_else(|| "mtnbenin".into()),
            )
            .json(&serde_json::json!({
                "amount": params.amount.to_string(),
                "currency": params.currency.to_uppercase(),
                "externalId": params.idempotency_key.to_string(),
                "payee": { "partyIdType": "MSISDN", "partyId": params.phone },
                "payerMessage": params.note,
                "payeeNote": params.note,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("MTN MoMo did not answer: {e}")))?;

        // MTN answers 202 with an empty body and expects the caller to poll
        // on the reference it was given. That reference is ours, which is
        // why it is safe to use as the transaction id.
        if response.status() == reqwest::StatusCode::ACCEPTED {
            return Ok(PayoutResult {
                provider: ProviderName::Mtn,
                provider_txn_id: params.idempotency_key.to_string(),
                status: PayoutStatus::Pending,
                message: None,
            });
        }

        read_response(ProviderName::Mtn, response, "referenceId").await
    }
}

// ═══════════════════════════════════════════════════════════════════
// Wave
// ═══════════════════════════════════════════════════════════════════

pub struct WaveProvider;

#[async_trait]
impl MobileMoneyProvider for WaveProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Wave
    }

    async fn initiate_payout(&self, params: &PayoutParams<'_>) -> Result<PayoutResult, AppError> {
        validate_e164(params.phone)?;

        let (Some(key), Some(base)) = (credential("WAVE_API_KEY"), credential("WAVE_BASE_URL"))
        else {
            return Err(unconfigured(ProviderName::Wave));
        };

        let response = http()?
            .post(format!("{}/v1/payout", base.trim_end_matches('/')))
            .bearer_auth(key)
            .header("Idempotency-Key", params.idempotency_key.to_string())
            .json(&serde_json::json!({
                "mobile": params.phone,
                "amount": params.amount.to_string(),
                "currency": params.currency.to_uppercase(),
                "client_reference": params.idempotency_key.to_string(),
                "name": params.note,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Wave did not answer: {e}")))?;

        read_response(ProviderName::Wave, response, "id").await
    }
}

pub fn get_provider(name: ProviderName) -> Box<dyn MobileMoneyProvider> {
    match name {
        ProviderName::Orange => Box::new(OrangeMoneyProvider),
        ProviderName::Mtn => Box::new(MtnMobileMoneyProvider),
        ProviderName::Wave => Box::new(WaveProvider),
    }
}

/// Which providers this deployment can actually reach.
///
/// For the boot-time report and for an admin page: an operator should be able
/// to see that Wave is dark before a contributor discovers it.
pub fn configured_providers() -> Vec<ProviderName> {
    [ProviderName::Orange, ProviderName::Mtn, ProviderName::Wave]
        .into_iter()
        .filter(|p| p.required_env().iter().all(|v| credential(v).is_some()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_number_has_to_be_international() {
        assert!(validate_e164("+22990112233").is_ok());
        assert!(validate_e164("90112233").is_err());
        assert!(validate_e164("+229 90 11 22 33").is_err());
        assert!(validate_e164("+").is_err());
        assert!(validate_e164("").is_err());
    }

    #[test]
    fn light_kyc_stops_at_its_ceiling() {
        assert!(within_kyc_lite(&dec("99999"), "XOF", false));
        assert!(within_kyc_lite(&dec("100000"), "XOF", false));
        assert!(!within_kyc_lite(&dec("100001"), "XOF", false));
    }

    #[test]
    fn full_kyc_clears_any_amount() {
        assert!(within_kyc_lite(&dec("5000000"), "XOF", true));
        assert!(within_kyc_lite(&dec("5000"), "EUR", true));
    }

    #[test]
    fn a_currency_light_kyc_was_not_written_for_is_refused() {
        // The ceiling is stated in XOF. Applying the same number to euros
        // would be a ceiling six hundred times higher by accident.
        assert!(!within_kyc_lite(&dec("50"), "EUR", false));
    }

    #[test]
    fn every_provider_names_what_it_needs() {
        for provider in [ProviderName::Orange, ProviderName::Mtn, ProviderName::Wave] {
            assert!(
                !provider.required_env().is_empty(),
                "{} would look configured with no credentials at all",
                provider.as_str()
            );
        }
    }

    #[test]
    fn a_provider_name_round_trips() {
        for (input, expected) in [
            ("orange", ProviderName::Orange),
            ("orange_money", ProviderName::Orange),
            ("mtn_momo", ProviderName::Mtn),
            ("wave", ProviderName::Wave),
        ] {
            assert_eq!(ProviderName::from_str(input).unwrap(), expected);
        }
        assert!(ProviderName::from_str("paypal").is_err());
    }
}
