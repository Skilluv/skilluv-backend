//! P13.1 — Endpoints wallet talent.
//!
//! - GET /api/users/me/wallet : solde EUR + XOF + statut providers.
//! - GET /api/users/me/wallet/transactions?limit=20 : ledger récent.
//! - POST /api/users/me/wallet/residency { country: "CI" } : déclare la
//!   résidence (utilisée pour choisir le canal payout par défaut).
//!
//! Les withdraw endpoints (Stripe / Momo) sont dans P13.2 et P13.3.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::talent_wallet;

pub fn talent_wallet_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/wallet", get(my_wallet))
        .route("/users/me/wallet/transactions", get(my_wallet_transactions))
        .route("/users/me/wallet/residency", post(set_my_residency))
        // P13.2 — Stripe Connect
        .route("/users/me/wallet/stripe/onboard", post(stripe_onboard))
        .route("/users/me/wallet/withdraw/stripe", post(stripe_withdraw))
        .route("/webhooks/stripe-connect", post(stripe_connect_webhook))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct TxQuery {
    limit: Option<i64>,
}

async fn my_wallet(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let wallet = talent_wallet::get_or_init_wallet(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "wallet": wallet }))))
}

async fn my_wallet_transactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<TxQuery>,
) -> Result<Json<Value>, AppError> {
    let txs = talent_wallet::list_transactions(
        &state.db,
        auth.user_id,
        q.limit.unwrap_or(20),
    )
    .await?;
    Ok(Json(build_response(json!({ "transactions": txs }))))
}

#[derive(Debug, Deserialize)]
struct ResidencyBody {
    country: String,
}

async fn set_my_residency(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ResidencyBody>,
) -> Result<Json<Value>, AppError> {
    let wallet =
        talent_wallet::set_residency_country(&state.db, auth.user_id, &body.country).await?;
    Ok(Json(build_response(json!({ "wallet": wallet }))))
}

// ═══════════════════════════════════════════════════════════════════
// P13.2 — Stripe Connect Express (talent payout channel EU/international)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct StripeOnboardBody {
    /// ISO 3166-1 alpha-2, ex "FR", "US". Stripe supporte une liste précise.
    country: String,
}

/// POST /api/users/me/wallet/stripe/onboard
///
/// Crée un compte Stripe Connect Express + retourne l'URL d'onboarding
/// hébergée. Le user complète KYC côté Stripe, on capture l'account_id.
/// Le webhook `account.updated` (endpoint plus bas) met à jour le statut KYC
/// dès que Stripe confirme la vérification.
async fn stripe_onboard(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<StripeOnboardBody>,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env().ok_or_else(|| {
        AppError::Internal("Stripe is not configured on this deployment".into())
    })?;

    // Récupère l'email du user (Stripe requires it).
    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    let account =
        crate::services::stripe::create_connect_account(&cfg, &email, &body.country.to_uppercase())
            .await?;

    // Persist l'account_id dès la création (avant onboarding complet), pour
    // qu'on puisse retrouver le user via webhook.
    sqlx::query(
        "UPDATE talent_wallets
         SET stripe_account_id = $1, stripe_kyc_status = 'pending',
             residency_country = COALESCE(residency_country, $2),
             updated_at = NOW()
         WHERE user_id = $3",
    )
    .bind(&account.id)
    .bind(body.country.to_uppercase())
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    // Assure que le wallet existe (INSERT ... ON CONFLICT dans la fn init).
    let _ = talent_wallet::get_or_init_wallet(&state.db, auth.user_id).await?;
    // Puis on re-tente l'update au cas où l'INSERT initial n'aurait pas eu
    // les colonnes stripe positionnées.
    sqlx::query(
        "UPDATE talent_wallets
         SET stripe_account_id = $1, stripe_kyc_status = 'pending',
             residency_country = COALESCE(residency_country, $2),
             updated_at = NOW()
         WHERE user_id = $3 AND stripe_account_id IS NULL",
    )
    .bind(&account.id)
    .bind(body.country.to_uppercase())
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    let refresh_url = format!(
        "{base}/wallet/onboarding?refresh=1",
        base = state.config.base_url
    );
    let return_url = format!(
        "{base}/wallet/onboarding/done",
        base = state.config.base_url
    );
    let link =
        crate::services::stripe::create_account_link(&cfg, &account.id, &refresh_url, &return_url)
            .await?;

    metrics::counter!("skilluv_stripe_connect_onboarding_started_total").increment(1);

    Ok(Json(build_response(json!({
        "account_id": account.id,
        "onboarding_url": link.url,
        "expires_at": link.expires_at,
    }))))
}

#[derive(Debug, Deserialize)]
struct StripeWithdrawBody {
    /// Montant en devise (pas cents). Ex: "12.50" EUR.
    amount: String,
    /// EUR uniquement pour Stripe. XOF est traité par Momo (P13.3).
    #[serde(default = "default_eur")]
    currency: String,
}

fn default_eur() -> String {
    "EUR".to_string()
}

/// POST /api/users/me/wallet/withdraw/stripe
///
/// Débite le wallet + crée un Stripe Transfer vers le compte Connect du user.
/// Nécessite que `stripe_kyc_status = 'verified'` (le webhook a confirmé).
async fn stripe_withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<StripeWithdrawBody>,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env().ok_or_else(|| {
        AppError::Internal("Stripe is not configured on this deployment".into())
    })?;
    if body.currency.to_uppercase() != "EUR" {
        return Err(AppError::Validation(
            "Stripe withdraw only supports EUR currently".into(),
        ));
    }
    let amount = bigdecimal::BigDecimal::from_str(&body.amount)
        .map_err(|_| AppError::Validation("invalid amount".into()))?;

    // Verifie KYC + charge account_id
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT stripe_account_id, stripe_kyc_status
         FROM talent_wallets WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;
    let (account_id, kyc_status) = row.ok_or_else(|| {
        AppError::Validation("Wallet not initialized — call onboard first".into())
    })?;
    let account_id = account_id.ok_or_else(|| {
        AppError::Validation("Stripe Connect not linked — call onboard first".into())
    })?;
    if kyc_status != "verified" {
        return Err(AppError::Validation(format!(
            "KYC status is '{kyc_status}', payout not allowed"
        )));
    }

    // Debit d'abord (guardé par balance) puis transfer.
    let debit_entry = talent_wallet::LedgerEntry {
        user_id: auth.user_id,
        delta: &amount,
        currency: talent_wallet::Currency::Eur,
        reason: "withdraw_stripe",
        related_slice_id: None,
        related_provider_txn_id: None,
        notes: None,
    };
    let debit_row = talent_wallet::debit(&state.db, debit_entry).await?;

    // Convertit en cents. BigDecimal * 100 → i64.
    let amount_cents: i64 = {
        use num_traits::ToPrimitive;
        let cents = &amount * bigdecimal::BigDecimal::from(100);
        cents
            .to_i64()
            .ok_or_else(|| AppError::Validation("amount too large".into()))?
    };

    let transfer_res = crate::services::stripe::create_transfer(
        &cfg,
        &account_id,
        amount_cents,
        "eur",
        &format!("skilluv-payout:{}", debit_row.id),
    )
    .await;

    match transfer_res {
        Ok(transfer_json) => {
            let stripe_txn_id = transfer_json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            // Enregistre l'id Stripe sur la ligne de tx (traçabilité).
            sqlx::query(
                "UPDATE talent_transactions SET related_provider_txn_id = $1 WHERE id = $2",
            )
            .bind(&stripe_txn_id)
            .bind(debit_row.id)
            .execute(&state.db)
            .await?;

            metrics::counter!("skilluv_stripe_payouts_total").increment(1);
            Ok(Json(build_response(json!({
                "transaction_id": debit_row.id,
                "stripe_transfer_id": stripe_txn_id,
                "amount_cents": amount_cents,
            }))))
        }
        Err(e) => {
            // Rollback logique : ré-crédite le wallet si Stripe refuse.
            let refund_entry = talent_wallet::LedgerEntry {
                user_id: auth.user_id,
                delta: &amount,
                currency: talent_wallet::Currency::Eur,
                reason: "withdraw_stripe_refund",
                related_slice_id: None,
                related_provider_txn_id: None,
                notes: Some("stripe transfer failed"),
            };
            let _ = talent_wallet::credit(&state.db, refund_entry).await;
            Err(e)
        }
    }
}

/// POST /api/webhooks/stripe-connect
///
/// Reçoit `account.updated` de Stripe. Vérifie la signature HMAC, extrait
/// `charges_enabled` et `payouts_enabled` pour marquer le KYC verified.
async fn stripe_connect_webhook(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env().ok_or_else(|| {
        AppError::Internal("Stripe is not configured".into())
    })?;
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    crate::services::stripe::verify_webhook_signature(
        &cfg.webhook_secret,
        &body,
        signature,
        300,
    )?;

    let event: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::Validation(format!("stripe payload decode: {e}")))?;
    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type != "account.updated" {
        return Ok(Json(build_response(json!({ "ignored": event_type }))));
    }

    let obj = event
        .get("data")
        .and_then(|d| d.get("object"))
        .cloned()
        .unwrap_or(Value::Null);
    let account_id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let details_submitted = obj
        .get("details_submitted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let charges_enabled = obj
        .get("charges_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let payouts_enabled = obj
        .get("payouts_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let new_status = if payouts_enabled && charges_enabled {
        "verified"
    } else if details_submitted {
        "pending"
    } else {
        "not_started"
    };

    sqlx::query(
        "UPDATE talent_wallets
         SET stripe_kyc_status = $1, updated_at = NOW()
         WHERE stripe_account_id = $2",
    )
    .bind(new_status)
    .bind(&account_id)
    .execute(&state.db)
    .await?;

    metrics::counter!(
        "skilluv_stripe_webhook_events_total",
        "type" => event_type.to_string(),
        "status" => new_status.to_string()
    )
    .increment(1);

    Ok(Json(build_response(json!({
        "account_id": account_id,
        "new_status": new_status,
    }))))
}

use std::str::FromStr;
