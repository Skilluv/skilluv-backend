//! P13.1 — Endpoints wallet talent.
//!
//! - GET /api/users/me/wallet : solde EUR + XOF + statut providers.
//! - GET /api/users/me/wallet/transactions?limit=20 : ledger récent.
//! - POST /api/users/me/wallet/residency { country: "CI" } : déclare la
//!   résidence (utilisée pour choisir le canal payout par défaut).
//!
//! Les withdraw endpoints (Stripe / Momo) sont dans P13.2 et P13.3.

use std::str::FromStr;

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

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
        .route("/users/me/wallet/withdraw", post(withdraw))
        .route("/webhooks/stripe-connect", post(stripe_connect_webhook))
        // P13.3 — Mobile Money (Orange, MTN, Wave)
        .route("/users/me/wallet/momo/phone", post(register_momo_phone))
        // P13.5 — Compliance : limites journalières/mensuelles + statement CSV
        .route("/users/me/wallet/statement.csv", get(wallet_statement_csv))
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

/// Get the caller's talent wallet.
#[utoipa::path(
    get, path = "/api/users/me/wallet", tag = "wallet",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_wallet(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let wallet = talent_wallet::get_or_init_wallet(&state.db, auth.user_id).await?;
    // Balances are derived from the ledger on every read rather than stored
    // on the row: a cached balance that can drift from the books is exactly
    // what migration 0158 removed.
    let balances = talent_wallet::balances(&state.db, auth.user_id).await?;
    Ok(Json(build_response(
        json!({ "wallet": wallet, "balances": balances }),
    )))
}

/// List my wallet transactions.
#[utoipa::path(
    get, path = "/api/users/me/wallet/transactions", tag = "wallet",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_wallet_transactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<TxQuery>,
) -> Result<Json<Value>, AppError> {
    let movements =
        talent_wallet::list_movements(&state.db, auth.user_id, q.limit.unwrap_or(20)).await?;
    Ok(Json(build_response(json!({ "transactions": movements }))))
}

#[derive(Debug, Deserialize)]
struct ResidencyBody {
    country: String,
}

/// Set residency country for wallet payouts.
#[utoipa::path(
    post, path = "/api/users/me/wallet/residency", tag = "wallet",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn set_my_residency(
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
/// Start Stripe Connect Express onboarding for talent payouts.
#[utoipa::path(
    post, path = "/api/users/me/wallet/stripe/onboard", tag = "wallet",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn stripe_onboard(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<StripeOnboardBody>,
) -> Result<Json<Value>, AppError> {
    let cfg = crate::services::stripe::StripeConfig::from_env()
        .ok_or_else(|| AppError::Internal("Stripe is not configured on this deployment".into()))?;

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

/// POST /api/webhooks/stripe-connect
///
/// Reçoit `account.updated` de Stripe. Vérifie la signature HMAC, extrait
/// `charges_enabled` et `payouts_enabled` pour marquer le KYC verified.
/// Stripe Connect webhook receiver (account.updated).
#[utoipa::path(
    post, path = "/api/webhooks/stripe-connect", tag = "wallet",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn stripe_connect_webhook(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    // Stripe pas configuré (dev/CI/deployments sans Stripe) : ack
    // silencieusement 200. Best-practice pour webhooks externes.
    let Some(cfg) = crate::services::stripe::StripeConfig::from_env() else {
        tracing::warn!(
            "Stripe Connect webhook received but STRIPE_* env not configured — acking silently"
        );
        return Ok(Json(json!({ "status": "acked_not_configured" })));
    };
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    crate::services::stripe::verify_webhook_signature(&cfg.webhook_secret, &body, signature, 300)?;

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
    let account_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
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

// ═══════════════════════════════════════════════════════════════════
// P13.5 — Compliance : limites journalière / mensuelle + statement CSV
// ═══════════════════════════════════════════════════════════════════

/// Vérifie qu'un débit projeté ne dépasse pas la limite configurée pour la
/// fenêtre. `label` sert au message d'erreur ("daily", "monthly").
async fn enforce_limit(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    currency: crate::services::ledger::Currency,
    proposed: &bigdecimal::BigDecimal,
    env_key: &str,
    hours: i32,
    label: &str,
) -> Result<(), AppError> {
    let limit = std::env::var(env_key)
        .ok()
        .and_then(|s| bigdecimal::BigDecimal::from_str(&s).ok());
    let Some(limit) = limit else {
        return Ok(()); // Limite non configurée = pas de gate.
    };
    let already = talent_wallet::withdrawn_within(db, user_id, currency, hours).await?;
    let projected = &already + proposed;
    if projected > limit {
        return Err(AppError::Validation(format!(
            "{label} withdraw limit exceeded ({} {} already withdrawn + {} proposed > {} limit)",
            already,
            currency.as_str(),
            proposed,
            limit
        )));
    }
    Ok(())
}

/// GET /api/users/me/wallet/statement.csv
///
/// Export CSV du ledger complet du user — obligation fiscale + audit personnel.
/// Export the wallet ledger as CSV.
#[utoipa::path(
    get, path = "/api/users/me/wallet/statement.csv", tag = "wallet",
    responses((status = 200, description = "CSV export")),
    security(("cookie_auth" = [])),
)]
pub async fn wallet_statement_csv(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    let csv = talent_wallet::statement_csv(&state.db, auth.user_id).await?;
    let body = axum::body::Body::from(csv);
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"skilluv-wallet-statement.csv\"",
            ),
        ],
        body,
    )
        .into_response())
}

// ═══════════════════════════════════════════════════════════════════
// P13.3 — Mobile Money (channel Africa-first)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct RegisterMomoBody {
    /// Format E.164, ex "+22507xxxxxxxx".
    phone: String,
    /// P13.3 placeholder — trusts the client to have run OTP verification.
    /// Optional with a default of `true` (matching the current permissive
    /// stance) so the front doesn't 422 when it just sends `{ phone,
    /// provider }`. **TODO P15**: replace with a real OTP round-trip via
    /// `services::otp::verify` and drop the default.
    #[serde(default = "default_true")]
    verified: bool,
    /// "orange" | "mtn" | "wave" — optional today (single-provider
    /// dispatch via env), tracked so we can route per-provider in P15.
    #[serde(default)]
    #[allow(dead_code)]
    provider: Option<String>,
}

fn default_true() -> bool {
    true
}

/// POST /api/users/me/wallet/momo/phone
///
/// Enregistre / met à jour le téléphone Mobile Money du user. `verified=true`
/// débloque les payouts < seuil KYC lite. Idempotent.
/// Register a Mobile Money phone number.
#[utoipa::path(
    post, path = "/api/users/me/wallet/momo/phone", tag = "wallet",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn register_momo_phone(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterMomoBody>,
) -> Result<Json<Value>, AppError> {
    // Validation E.164 minimale
    if !body.phone.starts_with('+')
        || body.phone[1..]
            .chars()
            .filter(|c| c.is_ascii_digit())
            .count()
            < 8
    {
        return Err(AppError::Validation(
            "phone must be E.164 format (starts with '+' and 8-15 digits)".into(),
        ));
    }

    // Validate the operator before writing: a typo stored here would only
    // surface much later, on the first withdrawal.
    let provider = match body.provider.as_deref() {
        Some(p) => Some(
            crate::services::mobile_money::ProviderName::from_str(p)?
                .as_str()
                .to_string(),
        ),
        None => None,
    };

    let _ = talent_wallet::get_or_init_wallet(&state.db, auth.user_id).await?;
    sqlx::query(
        "UPDATE talent_wallets
         SET momo_phone = $1,
             momo_phone_verified = $2,
             -- Keep a previously known operator when this call omits one,
             -- rather than blanking it and breaking the next withdrawal.
             momo_provider = COALESCE($3, momo_provider),
             updated_at = NOW()
         WHERE user_id = $4",
    )
    .bind(&body.phone)
    .bind(body.verified)
    .bind(provider.as_deref())
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "phone": body.phone,
        "verified": body.verified,
        "provider": provider,
    }))))
}

/// KYC-lite ceiling. Above this, a full identity check is required before
/// money can leave.
const KYC_LITE_LIMIT_XOF: i64 = 100_000;

#[derive(Debug, Deserialize)]
struct WithdrawBody {
    /// Amount in currency units, not minor units: "12.50" EUR, "5000" XOF.
    amount: String,
    /// EUR or XOF. Inferred from the recipient's country when absent.
    currency: Option<String>,
    /// Force a rail. Normally inferred from the currency and what the
    /// recipient has on file.
    rail: Option<String>,
}

/// Withdraw available funds.
///
/// One endpoint for every rail. There used to be two — `/withdraw/stripe`
/// and `/withdraw/momo` — each with its own idea of what happens when a
/// provider refuses, and each carrying a copy of the limit checks. Which
/// rail reaches a recipient is a routing question answered by
/// `payout_routes`, not something a client should know or a URL encode.
///
/// The amount leaves the `available` balance, which only holds money whose
/// release window has closed. Held funds are not withdrawable, and that is
/// the point of holding them.
#[utoipa::path(
    post, path = "/api/users/me/wallet/withdraw", tag = "wallet",
    request_body(content = serde_json::Value),
    responses(
        (status = 200, description = "Accepted by the provider", body = serde_json::Value),
        (status = 400, description = "Insufficient available balance, over the KYC-lite limit, or no destination on file", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<WithdrawBody>,
) -> Result<Json<Value>, AppError> {
    use crate::services::ledger::{self, Currency, State as FundState};
    use crate::services::payout::{PayoutRequest, Rail};
    use sqlx::Row;

    let wallet = sqlx::query(
        "SELECT residency_country, momo_phone, momo_phone_verified,
                stripe_account_id, stripe_kyc_status
           FROM talent_wallets WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Validation("Wallet not initialized".into()))?;

    let residency: Option<String> = wallet.get("residency_country");
    let momo_phone: Option<String> = wallet.get("momo_phone");
    let momo_verified: bool = wallet.get("momo_phone_verified");
    let connect_id: Option<String> = wallet.get("stripe_account_id");
    let kyc: String = wallet.get("stripe_kyc_status");

    let currency: Currency = match body.currency.as_deref() {
        Some(c) => c.parse()?,
        None => {
            if matches!(
                residency.as_deref(),
                Some("CI" | "SN" | "BJ" | "TG" | "ML" | "BF" | "NE" | "GW")
            ) {
                Currency::Xof
            } else {
                Currency::Eur
            }
        }
    };

    let amount = bigdecimal::BigDecimal::from_str(&body.amount)
        .map_err(|_| AppError::Validation("invalid amount".into()))?;
    if amount.sign() != bigdecimal::num_bigint::Sign::Plus {
        return Err(AppError::Validation("amount must be positive".into()));
    }

    // Only released money can leave. Including held funds here would let
    // someone withdraw money the payer can still reclaim.
    let available =
        ledger::user_balance(&state.db, auth.user_id, FundState::Available, currency).await?;
    if amount > available {
        return Err(AppError::Validation(format!(
            "insufficient available balance: {available} {}. Funds inside their \
             release window cannot be withdrawn yet.",
            currency.as_str()
        )));
    }

    let (rail, destination) = match body.rail.as_deref() {
        Some("mobile_money") => (Rail::MobileMoney, momo_phone.clone()),
        Some("bank_account") => (Rail::BankAccount, connect_id.clone()),
        Some(other) => {
            return Err(AppError::Validation(format!(
                "unknown rail '{other}' (mobile_money or bank_account)"
            )));
        }
        None => match currency {
            Currency::Xof => (Rail::MobileMoney, momo_phone.clone()),
            Currency::Eur => (Rail::BankAccount, connect_id.clone()),
        },
    };

    let destination = destination.ok_or_else(|| {
        AppError::Validation(
            "no destination on file for this rail — register a Mobile Money \
             number or complete Stripe onboarding first"
                .into(),
        )
    })?;

    match rail {
        Rail::MobileMoney => {
            if !momo_verified {
                return Err(AppError::Validation(
                    "Phone not verified — complete the SMS OTP first".into(),
                ));
            }
            use num_traits::ToPrimitive;
            if amount.to_i64().unwrap_or(0) > KYC_LITE_LIMIT_XOF {
                return Err(AppError::Validation(format!(
                    "Amount exceeds KYC-lite limit ({KYC_LITE_LIMIT_XOF} XOF). \
                     Complete full KYC first."
                )));
            }
        }
        Rail::BankAccount => {
            if kyc != "verified" {
                return Err(AppError::Validation(
                    "Complete Stripe onboarding before withdrawing".into(),
                ));
            }
        }
    }

    let (daily_var, monthly_var) = match currency {
        Currency::Eur => ("WALLET_DAILY_LIMIT_EUR", "WALLET_MONTHLY_LIMIT_EUR"),
        Currency::Xof => ("WALLET_DAILY_LIMIT_XOF", "WALLET_MONTHLY_LIMIT_XOF"),
    };
    enforce_limit(
        &state.db,
        auth.user_id,
        currency,
        &amount,
        daily_var,
        24,
        "daily",
    )
    .await?;
    enforce_limit(
        &state.db,
        auth.user_id,
        currency,
        &amount,
        monthly_var,
        30 * 24,
        "monthly",
    )
    .await?;

    let registry = crate::services::payout_adapters::registry_from_env();
    let provider = registry
        .resolve(&state.db, residency.as_deref(), currency, rail)
        .await?;

    let idempotency_key = format!(
        "withdraw:{}:{}:{}",
        auth.user_id,
        amount,
        chrono::Utc::now().timestamp()
    );

    let receipt = crate::services::payout::send(
        &state.db,
        provider.as_ref(),
        PayoutRequest {
            user_id: auth.user_id,
            amount: &amount,
            currency,
            destination: &destination,
            note: "Skilluv withdrawal",
            idempotency_key: &idempotency_key,
        },
    )
    .await?;

    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(auth.user_id),
        "payout.sent",
    )
    .arg("amount", format!("{} {}", amount, currency.as_str()))
    .arg("destination", provider.name())
    .payload(json!({ "reference": receipt.reference }))
    .execute()
    .await;

    Ok(Json(build_response(json!({
        "amount": amount.to_string(),
        "currency": currency.as_str(),
        "provider": receipt.provider,
        "reference": receipt.reference,
        "status": receipt.status,
    }))))
}
