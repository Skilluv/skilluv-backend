//! P13.3 — Mobile Money provider trait (Orange, MTN, Wave).
//!
//! Design :
//! - Trait `MobileMoneyProvider` normalise l'API : `initiate_payout(...)`.
//! - `OrangeMoneyProvider`, `MtnMobileMoneyProvider`, `WaveProvider` sont
//!   des impls concrètes. Les 3 sont en mode "stub sandbox" en P13.3 —
//!   elles retournent des transaction_ids générés localement + logs, mais
//!   ne frappent pas les APIs réelles tant que les env `<PROVIDER>_API_KEY`
//!   ne sont pas configurées.
//! - Un `Provider::from_str` permet de dispatcher depuis un param HTTP.
//!
//! Compliance / KYC :
//! - Pour < 100 000 XOF (~150 EUR), un téléphone vérifié SMS suffit (mode
//!   "KYC lite"). Au-delà, on requiert un KYC full à ajouter en P14+.
//! - Chaque payout est logué dans `talent_transactions` avec
//!   `related_provider_txn_id` pour rapprochement.

use std::str::FromStr;

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use serde::Serialize;
use uuid::Uuid;

use crate::errors::AppError;

/// Providers supportés côté API publique.
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

/// Résultat d'un payout : l'id retourné par le provider (à stocker dans
/// `talent_transactions.related_provider_txn_id`) + statut.
#[derive(Debug, Clone, Serialize)]
pub struct PayoutResult {
    pub provider: ProviderName,
    pub provider_txn_id: String,
    pub status: PayoutStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutStatus {
    /// Le provider a accepté la requête, en attente de confirmation async.
    Pending,
    /// Le provider a confirmé + le talent a reçu.
    Completed,
    /// Refusé (KYC insuffisant, numéro invalide, solde provider insuffisant…).
    Rejected,
}

/// Paramètres d'un payout.
#[derive(Debug, Clone)]
pub struct PayoutParams<'a> {
    pub user_id: Uuid,
    /// Numéro E.164 (ex "+22507xxxxxxxx" pour Côte d'Ivoire).
    pub phone: &'a str,
    /// Devise (XOF, XAF, KES, GHS…). En P13.3 on ne teste que XOF.
    pub currency: &'a str,
    pub amount: &'a BigDecimal,
    /// Description brève pour le user + audit interne.
    pub note: &'a str,
}

#[async_trait]
pub trait MobileMoneyProvider: Send + Sync {
    fn name(&self) -> ProviderName;

    async fn initiate_payout(&self, params: &PayoutParams<'_>) -> Result<PayoutResult, AppError>;
}

/// Sanity-check du numéro de téléphone E.164 : commence par '+' + 8 à 15 digits.
fn validate_e164(phone: &str) -> Result<(), AppError> {
    if !phone.starts_with('+') {
        return Err(AppError::Validation(
            "phone must be in E.164 format starting with '+'".into(),
        ));
    }
    let digits = &phone[1..];
    if digits.len() < 8 || digits.len() > 15 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Validation(
            "phone must contain 8-15 digits after '+'".into(),
        ));
    }
    Ok(())
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
                "Orange Money currently supports XOF only".into(),
            ));
        }

        // En prod : POST https://api.orange.com/orange-money-webpay/dev/v1/webpayment
        // avec `Authorization: Bearer <ORANGE_API_KEY>`. Ici on gate sur env.
        let has_credentials = std::env::var("ORANGE_MONEY_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some();

        // On log toujours l'attempt + retourne un ID synthétique en dev.
        let txn_id = format!("orange:dev:{}", Uuid::new_v4());
        tracing::info!(
            provider = "orange",
            user_id = %params.user_id,
            phone = params.phone,
            amount = %params.amount,
            currency = params.currency,
            has_credentials,
            txn_id,
            note = params.note,
            "orange money payout initiated (stub)"
        );

        if !has_credentials {
            // Mode dev : on retourne pending (côté service, à charge d'appeler
            // le webhook Orange plus tard pour marquer completed).
            return Ok(PayoutResult {
                provider: ProviderName::Orange,
                provider_txn_id: txn_id,
                status: PayoutStatus::Pending,
                message: Some("dev mode — no real Orange API call made".into()),
            });
        }

        // TODO : appeler l'API Orange réelle. Pour l'instant même retour que dev.
        Ok(PayoutResult {
            provider: ProviderName::Orange,
            provider_txn_id: txn_id,
            status: PayoutStatus::Pending,
            message: None,
        })
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
        let txn_id = format!("mtn:dev:{}", Uuid::new_v4());
        tracing::info!(
            provider = "mtn",
            user_id = %params.user_id,
            phone = params.phone,
            amount = %params.amount,
            currency = params.currency,
            txn_id,
            "mtn momo payout initiated (stub)"
        );
        Ok(PayoutResult {
            provider: ProviderName::Mtn,
            provider_txn_id: txn_id,
            status: PayoutStatus::Pending,
            message: Some("MTN MoMo integration stub — not live".into()),
        })
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
        let txn_id = format!("wave:dev:{}", Uuid::new_v4());
        tracing::info!(
            provider = "wave",
            user_id = %params.user_id,
            phone = params.phone,
            amount = %params.amount,
            currency = params.currency,
            txn_id,
            "wave payout initiated (stub)"
        );
        Ok(PayoutResult {
            provider: ProviderName::Wave,
            provider_txn_id: txn_id,
            status: PayoutStatus::Pending,
            message: Some("Wave integration stub — not live".into()),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// Factory
// ═══════════════════════════════════════════════════════════════════

pub fn get_provider(name: ProviderName) -> Box<dyn MobileMoneyProvider> {
    match name {
        ProviderName::Orange => Box::new(OrangeMoneyProvider),
        ProviderName::Mtn => Box::new(MtnMobileMoneyProvider),
        ProviderName::Wave => Box::new(WaveProvider),
    }
}
