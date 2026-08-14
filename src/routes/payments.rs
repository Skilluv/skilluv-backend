//! What a payer can choose, and starting a payment they never leave for.
//!
//! ## Why the front asks rather than knows
//!
//! The operators available in a country change — Celtiis appeared in Benin,
//! Togocel is called Mixx By Yas now — and a list compiled into the front
//! end means a release to add one. The list lives in `payment_methods` and
//! is served from here, so opening an operator is a row.
//!
//! ## Why the status endpoint is not the source of truth
//!
//! The return page calls [`status`] to stop showing a spinner. It does not
//! deliver anything: it asks the provider and lets the shared fulfilment
//! path decide, which is the same path a webhook and the poller take. A
//! forged `?status=approved` therefore buys nothing, and a payer who never
//! comes back loses nothing.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::{AuthUser, OptionalAuth};

pub fn payment_routes() -> Router<AppState> {
    Router::new()
        .route("/payments/methods", get(methods))
        .route("/payments/{id}/status", get(status))
        .route("/payments/{id}/charge", post(charge))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MethodsQuery {
    /// ISO 3166-1 alpha-2 of the payer. Decides which operators exist.
    ///
    /// Optional for a signed-in caller, whose own country is used instead.
    /// The account stores the code; the profile only exposes the country's
    /// name, so a client asked to supply this would have to map names back
    /// to codes and would get it wrong for every country spelled two ways.
    pub country: Option<String>,
    /// Defaults to the currency the country is quoted in.
    pub currency: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct PaymentMethod {
    /// Stable identifier to send back when starting a payment.
    pub operator: String,
    /// What to show the payer. Operators rename themselves.
    pub label: String,
    /// TRUE when the payer confirms on their phone without leaving the
    /// page; FALSE when they are sent to the provider's own form.
    pub supports_inline: bool,
    pub provider: String,
}

/// The ways a payer in this country can pay.
#[utoipa::path(
    get, path = "/api/payments/methods", tag = "payments",
    params(MethodsQuery),
    responses(
        (status = 200, description = "Available methods, in display order", body = ApiResponse<Vec<PaymentMethod>>),
    ),
)]
pub async fn methods(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Query(q): Query<MethodsQuery>,
) -> Result<Json<ApiResponse<Vec<PaymentMethod>>>, AppError> {
    let currency = q
        .currency
        .unwrap_or_else(|| "XOF".to_string())
        .to_uppercase();

    let country = match q.country {
        Some(given) => given,
        None => {
            let user = auth.ok_or_else(|| {
                AppError::Validation("country is required when not signed in".into())
            })?;
            sqlx::query_scalar::<_, Option<String>>("SELECT country_iso2 FROM users WHERE id = $1")
                .bind(user.user_id)
                .fetch_optional(&state.db)
                .await?
                .flatten()
                .ok_or_else(|| {
                    AppError::Validation("this account has no country on it yet".into())
                })?
        }
    };

    let methods: Vec<PaymentMethod> = sqlx::query_as(
        "SELECT operator, label, supports_inline, provider
           FROM payment_methods
          WHERE enabled = TRUE
            AND country = $1
            AND currency = $2
          ORDER BY sort_order, label",
    )
    .bind(country.to_uppercase())
    .bind(&currency)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(methods)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChargeRequest {
    /// One of the `operator` values from `/payments/methods`.
    pub operator: String,
    /// E.164. Defaults to the number on the payment's customer record.
    pub phone: Option<String>,
}

/// Push the operator's prompt to the payer's phone.
///
/// For the methods that support it, this is the whole payment: no
/// redirect, no new tab. The payer approves on their handset and the
/// confirmation reaches us by webhook or by polling — never through this
/// request, which returns as soon as the prompt is sent.
#[utoipa::path(
    post, path = "/api/payments/{id}/charge", tag = "payments",
    params(("id" = Uuid, Path, description = "Payment id")),
    request_body = ChargeRequest,
    responses(
        (status = 200, description = "Prompt sent; wait for confirmation", body = serde_json::Value),
        (status = 400, description = "This operator needs a redirect, or the number is missing", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not the payer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn charge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ChargeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        payer_id: Option<Uuid>,
        provider: String,
        provider_session_id: Option<String>,
        status: String,
        merchant_reference: Option<String>,
        country: Option<String>,
    }

    let payment: Row = sqlx::query_as(
        "SELECT p.payer_id, p.provider, p.provider_session_id, p.status,
                p.merchant_reference, u.country_iso2 AS country
           FROM payments p
           LEFT JOIN users u ON u.id = p.payer_id
          WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("payment".into()))?;

    if payment.payer_id != Some(auth.user_id) {
        return Err(AppError::Forbidden);
    }
    if payment.status != "pending" {
        return Err(AppError::Conflict(format!(
            "this payment is {}, and cannot be charged again",
            payment.status
        )));
    }

    let method: Option<(String, bool)> = sqlx::query_as(
        "SELECT provider_mode, supports_inline
           FROM payment_methods
          WHERE provider = $1 AND operator = $2 AND enabled = TRUE
          ORDER BY (country = $3) DESC
          LIMIT 1",
    )
    .bind(&payment.provider)
    .bind(&body.operator)
    .bind(payment.country.as_deref().unwrap_or(""))
    .fetch_optional(&state.db)
    .await?;

    let Some((mode, supports_inline)) = method else {
        return Err(AppError::Validation(format!(
            "'{}' is not a payment method this deployment offers",
            body.operator
        )));
    };
    if !supports_inline {
        return Err(AppError::Validation(format!(
            "'{}' cannot be paid without leaving the page — use the checkout URL",
            body.operator
        )));
    }

    let phone = match body.phone {
        Some(ref given) => given.clone(),
        None => sqlx::query_scalar::<_, Option<String>>(
            "SELECT momo_phone FROM talent_wallets WHERE user_id = $1",
        )
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?
        .flatten()
        .ok_or_else(|| {
            AppError::Validation("a phone number is required for this payment method".into())
        })?,
    };

    let Some(transaction_id) = payment.provider_session_id.as_deref() else {
        return Err(AppError::Conflict(
            "this payment has no transaction at the provider yet".into(),
        ));
    };
    let cfg = crate::services::fedapay::FedaPayConfig::from_env()
        .ok_or_else(|| AppError::Internal("this deployment holds no FedaPay credentials".into()))?;

    crate::services::fedapay::charge_inline(
        &cfg,
        transaction_id,
        &mode,
        &phone,
        payment.country.as_deref().unwrap_or("bj"),
        payment.merchant_reference.as_deref().unwrap_or(""),
    )
    .await?;

    sqlx::query("UPDATE payments SET operator = $2 WHERE id = $1")
        .bind(id)
        .bind(&body.operator)
        .execute(&state.db)
        .await?;

    // Nothing is confirmed here on purpose. The payer has a prompt on their
    // phone; whether they approve it reaches us by webhook or by polling,
    // and this request returning has no bearing on either.
    Ok(Json(json!({
        "data": {
            "status": "prompt_sent",
            "message": "Approve the payment on your phone.",
        }
    })))
}

/// The shortest gap between two questions to the provider about one payment.
///
/// The front polls this endpoint every second or two while someone watches
/// a spinner, and every one of those used to become a request to FedaPay:
/// three minutes of waiting is ninety requests for a single payment.
/// Multiply by concurrent checkouts and the account gets rate-limited —
/// and the day that happens, the background poller is throttled with it,
/// which turns a cosmetic problem into a delivery one.
///
/// Three seconds is under what a person perceives as lag and two orders of
/// magnitude below a rate limit.
const PROVIDER_ASK_EVERY_SECONDS: i64 = 3;

/// Where a payment has got to.
///
/// Called by the page the payer is waiting on. It asks the provider — a
/// real question, not a cached guess — but at most once every few seconds
/// per payment, and it routes any answer through the shared fulfilment
/// path. It is a way to stop showing a spinner, not a way to be paid: a
/// forged call delivers nothing that the poller would not have delivered a
/// minute later anyway.
#[utoipa::path(
    get, path = "/api/payments/{id}/status", tag = "payments",
    params(("id" = Uuid, Path, description = "Payment id")),
    responses(
        (status = 200, description = "Current state", body = serde_json::Value),
        (status = 403, description = "Not the payer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        payer_id: Option<Uuid>,
        provider: String,
        status: String,
        merchant_reference: Option<String>,
        provider_reference: Option<String>,
        provider_session_id: Option<String>,
        fulfilled_at: Option<chrono::DateTime<chrono::Utc>>,
        /// Seconds since the provider was last asked about this payment, by
        /// this endpoint or by the background poller. They share the budget
        /// on purpose: the provider counts the requests, not our reasons
        /// for making them.
        since_last_ask: Option<f64>,
    }

    let payment: Row = sqlx::query_as(
        "SELECT payer_id, provider, status, merchant_reference, provider_reference,
                provider_session_id, fulfilled_at,
                EXTRACT(EPOCH FROM (NOW() - last_checked_at)) AS since_last_ask
           FROM payments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("payment".into()))?;

    if payment.payer_id != Some(auth.user_id) {
        return Err(AppError::Forbidden);
    }

    let may_ask = payment
        .since_last_ask
        .is_none_or(|seconds| seconds >= PROVIDER_ASK_EVERY_SECONDS as f64);

    if payment.status == "pending" && may_ask {
        // Stamped before the call, not after: two requests arriving
        // together must not both decide they are allowed to ask.
        //
        // `last_checked_at` only. `check_count` drives the poller's
        // backoff — a front polling for three minutes would push it past
        // sixty and convince the poller to wait half an hour, turning a
        // spinner into the reason the safety net stops working.
        sqlx::query("UPDATE payments SET last_checked_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await?;

        // Both providers, so the page behaves the same whichever one the
        // payer was routed to. Neither is asked through a search endpoint.
        let lookup = match payment.provider.as_str() {
            "fedapay" => match crate::services::fedapay::FedaPayConfig::from_env() {
                Some(cfg) => match (
                    payment.merchant_reference.as_deref(),
                    payment.provider_reference.as_deref(),
                ) {
                    (Some(reference), _) => {
                        crate::services::fedapay::transaction_by_merchant_reference(&cfg, reference)
                            .await
                    }
                    (None, Some(their_id)) => {
                        crate::services::fedapay::transaction_status(&cfg, their_id).await
                    }
                    (None, None) => Ok("pending".to_string()),
                },
                None => Ok("pending".to_string()),
            },

            "stripe" => match (
                crate::services::stripe::StripeConfig::from_env(),
                payment.provider_session_id.as_deref(),
            ) {
                (Some(cfg), Some(session_id)) => {
                    crate::services::stripe::retrieve_checkout_session(&cfg, session_id)
                        .await
                        .map(|session| {
                            crate::services::stripe::session_outcome(&session).to_string()
                        })
                }
                // No session id means the create response was lost.
                // Recovering it replays a create call, which is the
                // poller's job rather than a request handler's — the front
                // waits a minute rather than this endpoint doing something
                // expensive on every impatient refresh.
                _ => Ok("pending".to_string()),
            },

            _ => Ok("pending".to_string()),
        };

        if let Ok(remote) = lookup
            && matches!(remote.as_str(), "approved" | "transferred" | "paid")
        {
            // The delivery happens in the shared path, not here — so a
            // forged call to this endpoint cannot deliver anything, and the
            // result is identical to the poller finding it a minute later.
            let _ = crate::services::fulfilment::settle_and_deliver(&state.db, id, None).await;
        }
    }

    let (status, fulfilled): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, fulfilled_at FROM payments WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(json!({
        "data": {
            "status": status,
            // How long to wait before asking again. Sent rather than left
            // for the front to guess: the right interval is a property of
            // what the backend does with the question, and a front that
            // guesses too low is the thing that gets us rate-limited.
            "poll_after_ms": PROVIDER_ASK_EVERY_SECONDS * 1000,
            "delivered": fulfilled.is_some() || payment.fulfilled_at.is_some(),
        }
    })))
}
