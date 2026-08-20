//! Enterprise credits + Stripe checkout + webhook (Phase 3 — items 3.6-3.10).

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{credits, invoices, stripe};

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type EnterpriseCreditsRow586 = (
    Uuid,
    String,
    bigdecimal::BigDecimal,
    Option<i32>,
    i32,
    Option<chrono::DateTime<chrono::Utc>>,
    chrono::DateTime<chrono::Utc>,
);

pub fn enterprise_credits_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/credits", get(get_credits))
        .route("/enterprise/credits/transactions", get(list_txns))
        .route("/enterprise/credits/checkout", post(create_checkout))
        .route("/enterprise/credits/redeem", post(redeem_promo))
        .route("/enterprise/billing/portal", post(billing_portal))
        .route("/stripe/webhook", post(stripe_webhook))
        .route("/enterprise/invoices", get(list_invoices))
        .route("/enterprise/invoices/{id}", get(get_invoice))
        .route("/enterprise/invoices/{id}/html", get(get_invoice_html))
        // Alias — front called it `/preview` (per BE-P0-13 audit) and there's
        // no reason to make the front migrate the name for a doc mismatch.
        .route("/enterprise/invoices/{id}/preview", get(get_invoice_html))
        // BE-P0-36 — PDF endpoint. Delegates to the external
        // `skilluv-pdf-renderer` service (Python + weasyprint) via HTTP.
        // Returns 503 when `PDF_RENDERER_URL` isn't configured.
        .route("/enterprise/invoices/{id}/pdf", get(get_invoice_pdf))
        // Pricing public endpoint (Phase 3.14)
        .route("/pricing", get(public_pricing))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ─── Credits read ─────────────────────────────────────────────────

async fn current_enterprise_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

/// Get the current enterprise credits balance.
#[utoipa::path(
    get, path = "/api/enterprise/credits", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn get_credits(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let row = credits::get_or_init_credits(&state.db, enterprise_id).await?;
    Ok(Json(build_response(json!({
        "credits": {
            "enterprise_id": row.enterprise_id,
            "balance": credits::balance_as_f64(&row.balance),
            "total_purchased": row.total_purchased,
            "total_used": credits::balance_as_f64(&row.total_used),
            "total_refunded": credits::balance_as_f64(&row.total_refunded),
            "updated_at": row.updated_at,
        }
    }))))
}

#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

/// List enterprise credits transactions.
#[utoipa::path(
    get, path = "/api/enterprise/credits/transactions", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_txns(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let rows = credits::list_transactions(&state.db, enterprise_id, per_page, offset).await?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id,
                "delta": credits::balance_as_f64(&t.delta),
                "balance_after": credits::balance_as_f64(&t.balance_after),
                "reason": t.reason,
                "related_interest_request_id": t.related_interest_request_id,
                "related_payment_id": t.related_payment_id,
                "notes": t.notes,
                "expires_at": t.expires_at,
                "created_at": t.created_at,
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "transactions": items }))))
}

// ─── Stripe Checkout ─────────────────────────────────────────────

#[derive(Deserialize)]
struct CheckoutBody {
    pack_slug: String,
}

/// Create a Stripe checkout session for a credits pack.
#[utoipa::path(
    post, path = "/api/enterprise/credits/checkout", tag = "enterprise",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn create_checkout(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CheckoutBody>,
) -> Result<Json<Value>, AppError> {
    // Owner-only: buying credits engages the enterprise's card, so we don't
    // want a recruiter to top-up (or worse, drain the CB) without the owner's
    // knowledge. `require_enterprise_owner_pub` also enforces the standard
    // enterprise gates (verified email, active membership, 2FA).
    let enterprise = crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let pack =
        stripe::pack_by_slug(&body.pack_slug).ok_or(AppError::Validation("unknown pack".into()))?;
    let enterprise_id = enterprise.id;

    // The buyer, and the country their organisation is in — which decides
    // whether a card checkout reaches them at all. A Beninese enterprise
    // could not buy credits before this: the only door was Stripe, and
    // Stripe does not take Mobile Money there.
    // The buyer's own country: `enterprises` records no location, so the
    // person clicking is the best signal we have for which corridor
    // reaches them.
    let buyer: (String, String, Option<String>) =
        sqlx::query_as("SELECT email, display_name, country_iso2 FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;
    let (email, display_name, country) = buyer;

    let currency = crate::services::ledger::Currency::Eur;
    let registry = crate::services::collect_adapters::registry_from_env();
    let provider = registry
        .resolve(
            &state.db,
            country.as_deref(),
            currency,
            crate::services::collect::Method::Card,
        )
        .await?;

    let amount =
        bigdecimal::BigDecimal::from(pack.price_eur_cents) / bigdecimal::BigDecimal::from(100);
    let base = state.config.frontend_url.trim_end_matches('/').to_string();
    let success_url = format!("{base}/enterprise/credits?paid=1");
    let cancel_url = format!("{base}/enterprise/credits?canceled=1");
    // Keyed on the order rather than the enterprise: a company buying two
    // packs in a day is buying two packs.
    let order = Uuid::new_v4();
    let idempotency_key = format!("credit_pack:{order}");
    let description = format!("Skilluv — {} crédit(s)", pack.credits);

    let session = crate::services::collect::start(
        &state.db,
        provider.as_ref(),
        crate::services::collect::Method::Card,
        crate::services::collect::CollectionRequest {
            payer_id: Some(auth.user_id),
            payer_enterprise_id: Some(enterprise_id),
            payer_email: &email,
            payer_name: &display_name,
            payer_country: country.as_deref(),
            payer_phone: None,
            subject_type: "credit_pack",
            subject_id: order,
            amount: &amount,
            currency,
            description: &description,
            success_url: &success_url,
            cancel_url: &cancel_url,
            idempotency_key: &idempotency_key,
            operator: None,
            credits: Some(pack.credits),
            merchant_reference: None,
        },
    )
    .await?;

    Ok(Json(build_response(json!({
        "checkout_url": session.redirect_url,
        "provider": session.provider,
        "session_id": session.session_id,
        "payment_id": session.payment_id,
    }))))
}

/// Open the Stripe billing portal (owner only).
#[utoipa::path(
    post, path = "/api/enterprise/billing/portal", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn billing_portal(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    // Owner-only: the portal lets the caller update the card, download every
    // invoice, and — crucially — cancel the subscription. Recruiters must not
    // be able to nuke the enterprise's billing state.
    let enterprise = crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let cfg = stripe::StripeConfig::from_env().ok_or(AppError::ServiceUnavailable(
        "payments are not configured on this deployment".into(),
    ))?;
    // Look up the most recent Stripe customer for the enterprise (cached in metadata)
    let enterprise_id = enterprise.id;
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT notes FROM credit_transactions WHERE enterprise_id = $1 AND reason = 'purchase' AND notes LIKE 'cus_%' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(enterprise_id)
    .fetch_optional(&state.db)
    .await?;
    let customer_id = row.and_then(|(s,)| s).ok_or(AppError::Validation(
        "No Stripe customer recorded for this enterprise yet. Make at least one purchase first."
            .into(),
    ))?;
    let return_url = std::env::var("STRIPE_PORTAL_RETURN_URL")
        .unwrap_or_else(|_| format!("{}/enterprise/credits", crate::config::PUBLIC_SITE_URL));
    let url = stripe::create_billing_portal_session(&cfg, &customer_id, &return_url).await?;
    Ok(Json(build_response(json!({ "url": url }))))
}

// ─── Stripe Webhook ──────────────────────────────────────────────

/// Stripe webhook receiver.
#[utoipa::path(
    post, path = "/api/stripe/webhook", tag = "enterprise",
    request_body(content = serde_json::Value),
    responses((status = 200)),
)]
pub async fn stripe_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // Stripe not configured (dev/CI/deployments qui n'utilisent pas Stripe) :
    // silently ack le webhook avec 200. C'est la best-practice Stripe — un
    // non-200 declenche des retries indefinis. Le webhook n'est pas traite,
    // on log pour observabilite.
    let Some(cfg) = stripe::StripeConfig::from_env() else {
        tracing::warn!("Stripe webhook received but STRIPE_* env not configured — acking silently");
        return Ok(axum::http::StatusCode::OK);
    };
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    stripe::verify_webhook_signature(&cfg.webhook_secret, &body, sig, 300)?;

    let event: stripe::WebhookEvent = serde_json::from_slice(&body)
        .map_err(|e| AppError::Internal(format!("webhook decode failed: {e}")))?;

    // Idempotency
    let first_time = credits::mark_webhook_event(&state.db, &event.id, &event.event_type).await?;
    if !first_time {
        return Ok(axum::http::StatusCode::OK);
    }

    // Dispatch selon type d'event
    let purpose = event
        .data
        .object
        .get("metadata")
        .and_then(|m| m.get("purpose"))
        .and_then(Value::as_str)
        .unwrap_or("");

    // Certifications, mentorship, subscriptions — utilisent checkout.session.completed
    // avec `metadata.purpose` distinct de la vente de crédits.
    if event.event_type == "checkout.session.completed" && purpose == "certification" {
        handle_certification_paid(&state, &event.data.object).await?;
        credits::mark_webhook_processed(&state.db, &event.id).await?;
        return Ok(axum::http::StatusCode::OK);
    }
    if event.event_type == "checkout.session.completed" && purpose == "mentorship" {
        handle_mentorship_paid(&state, &event.data.object).await?;
        credits::mark_webhook_processed(&state.db, &event.id).await?;
        return Ok(axum::http::StatusCode::OK);
    }
    if event.event_type == "checkout.session.completed" && purpose == "subscription" {
        handle_subscription_started(&state, &event.data.object).await?;
        credits::mark_webhook_processed(&state.db, &event.id).await?;
        return Ok(axum::http::StatusCode::OK);
    }
    if matches!(
        event.event_type.as_str(),
        "customer.subscription.updated" | "customer.subscription.deleted"
    ) {
        handle_subscription_lifecycle(&state, &event.event_type, &event.data.object).await?;
        credits::mark_webhook_processed(&state.db, &event.id).await?;
        return Ok(axum::http::StatusCode::OK);
    }
    if event.event_type == "invoice.paid" {
        handle_invoice_paid_subscription(&state, &event.data.object).await?;
        credits::mark_webhook_processed(&state.db, &event.id).await?;
        return Ok(axum::http::StatusCode::OK);
    }

    if event.event_type == "checkout.session.completed" {
        let obj = &event.data.object;
        let pack_slug = obj
            .get("metadata")
            .and_then(|m| m.get("pack_slug"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let enterprise_id_str = obj
            .get("metadata")
            .and_then(|m| m.get("enterprise_id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let customer_id = obj
            .get("customer")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let payment_intent_id = obj
            .get("payment_intent")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let session_id = obj
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // Stripe's automatic_tax populates these on the session
        let amount_subtotal = obj
            .get("amount_subtotal")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let amount_total = obj.get("amount_total").and_then(Value::as_i64).unwrap_or(0);
        let total_tax = obj
            .get("total_details")
            .and_then(|d| d.get("amount_tax"))
            .and_then(Value::as_i64)
            .unwrap_or(amount_total - amount_subtotal);
        let currency_code = obj
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or("eur")
            .to_uppercase();
        // Customer's billing details (collected by Stripe)
        let billing_details = obj.get("customer_details");
        let billing_email = billing_details
            .and_then(|d| d.get("email"))
            .and_then(Value::as_str)
            .map(String::from);
        let billing_name = billing_details
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str)
            .map(String::from);
        let billing_country = billing_details
            .and_then(|d| d.get("address"))
            .and_then(|a| a.get("country"))
            .and_then(Value::as_str)
            .map(String::from);
        let billing_address_formatted = billing_details
            .and_then(|d| d.get("address"))
            .and_then(|a| serde_json::to_string_pretty(a).ok());

        let enterprise_id = Uuid::parse_str(enterprise_id_str)
            .map_err(|_| AppError::Internal("missing enterprise_id metadata".into()))?;
        let pack = stripe::pack_by_slug(pack_slug).ok_or(AppError::Internal(format!(
            "unknown pack_slug in webhook: {pack_slug}"
        )))?;

        let amount = credits::dec(&pack.credits.to_string());
        let notes = if !customer_id.is_empty() {
            customer_id.clone()
        } else {
            format!("session={session_id}")
        };
        let txn = credits::grant(
            &state.db,
            credits::GrantInput {
                enterprise_id,
                amount: &amount,
                reason: "purchase",
                related_payment_id: None,
                related_promo_code_id: None,
                notes: Some(&notes),
                actor_user_id: None,
                expires_at: None,
            },
        )
        .await?;

        // Generate invoice
        let tva_rate_pct = if amount_subtotal > 0 {
            (total_tax as f64 / amount_subtotal as f64) * 100.0
        } else {
            0.0
        };
        if let Err(err) = invoices::create(
            &state.db,
            invoices::CreateInvoiceInput {
                enterprise_id,
                amount_ht_cents: amount_subtotal,
                amount_tva_cents: total_tax,
                amount_ttc_cents: amount_total,
                tva_rate_pct,
                currency: &currency_code,
                billing_country: billing_country.as_deref(),
                billing_company_name: billing_name.as_deref(),
                billing_address: billing_address_formatted.as_deref(),
                billing_vat_number: None,
                description: Some(&format!("Pack de {} crédit(s) Skilluv", pack.credits)),
                stripe_payment_intent_id: if payment_intent_id.is_empty() {
                    None
                } else {
                    Some(&payment_intent_id)
                },
                stripe_session_id: Some(&session_id),
                related_transaction_id: Some(txn.id),
            },
        )
        .await
        {
            tracing::warn!(error = %err, %enterprise_id, "invoice generation failed");
        }
        let _ = billing_email; // reserved for future email-the-invoice flow

        tracing::info!(
            %enterprise_id,
            pack_slug,
            %payment_intent_id,
            "credits granted via Stripe checkout"
        );
        metrics::counter!(
            "skilluv_credits_purchased_total",
            "pack" => pack.slug.to_string()
        )
        .increment(pack.credits as u64);
    }

    credits::mark_webhook_processed(&state.db, &event.id).await?;
    Ok(axum::http::StatusCode::OK)
}

// ─── Webhook dispatch helpers (Phase 5 finalization) ─────────────

async fn handle_certification_paid(state: &AppState, obj: &Value) -> Result<(), AppError> {
    let attempt_id = obj
        .get("metadata")
        .and_then(|m| m.get("attempt_id"))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(AppError::Internal("missing attempt_id metadata".into()))?;
    let pi = obj
        .get("payment_intent")
        .and_then(Value::as_str)
        .map(String::from);
    let sid = obj.get("id").and_then(Value::as_str).map(String::from);
    sqlx::query(
        r#"
        UPDATE certification_attempts
        SET status = 'paid', stripe_payment_intent_id = $1, stripe_session_id = $2
        WHERE id = $3 AND status = 'pending'
        "#,
    )
    .bind(&pi)
    .bind(&sid)
    .bind(attempt_id)
    .execute(&state.db)
    .await?;
    metrics::counter!("skilluv_certifications_paid_total").increment(1);
    Ok(())
}

async fn handle_mentorship_paid(state: &AppState, obj: &Value) -> Result<(), AppError> {
    let session_id = obj
        .get("metadata")
        .and_then(|m| m.get("session_id"))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(AppError::Internal("missing session_id metadata".into()))?;
    let pi = obj
        .get("payment_intent")
        .and_then(Value::as_str)
        .map(String::from);
    let sid = obj.get("id").and_then(Value::as_str).map(String::from);
    sqlx::query(
        r#"
        UPDATE mentorship_sessions
        SET status = 'paid', stripe_payment_intent_id = $1, stripe_session_id = $2
        WHERE id = $3 AND status = 'pending'
        "#,
    )
    .bind(&pi)
    .bind(&sid)
    .bind(session_id)
    .execute(&state.db)
    .await?;
    metrics::counter!("skilluv_mentorship_paid_total").increment(1);
    Ok(())
}

async fn handle_subscription_started(state: &AppState, obj: &Value) -> Result<(), AppError> {
    let enterprise_id = obj
        .get("metadata")
        .and_then(|m| m.get("enterprise_id"))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(AppError::Internal("missing enterprise_id metadata".into()))?;
    let plan_slug = obj
        .get("metadata")
        .and_then(|m| m.get("plan_slug"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let monthly_credit_grant = obj
        .get("metadata")
        .and_then(|m| m.get("monthly_credit_grant"))
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    let stripe_sub_id = obj
        .get("subscription")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let customer_id = obj
        .get("customer")
        .and_then(Value::as_str)
        .map(String::from);
    if stripe_sub_id.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now();
    let sub = crate::services::subscriptions::upsert_from_stripe(
        &state.db,
        crate::services::subscriptions::StripeSubscriptionUpsert {
            enterprise_id,
            plan_slug: &plan_slug,
            stripe_customer_id: customer_id.as_deref(),
            stripe_subscription_id: &stripe_sub_id,
            status: "active",
            current_period_start: Some(now),
            current_period_end: Some(now + chrono::Duration::days(30)),
            cancel_at_period_end: false,
            monthly_credit_grant,
        },
    )
    .await?;
    let _ = crate::services::subscriptions::grant_monthly_credits_if_due(&state.db, &sub).await;
    metrics::counter!(
        "skilluv_subscriptions_started_total",
        "plan" => plan_slug
    )
    .increment(1);
    Ok(())
}

async fn handle_subscription_lifecycle(
    state: &AppState,
    event_type: &str,
    obj: &Value,
) -> Result<(), AppError> {
    let stripe_sub_id = obj.get("id").and_then(Value::as_str).unwrap_or("");
    if stripe_sub_id.is_empty() {
        return Ok(());
    }
    let status = obj
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active");
    let cancel_at_period_end = obj
        .get("cancel_at_period_end")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cps = obj.get("current_period_start").and_then(Value::as_i64);
    let cpe = obj.get("current_period_end").and_then(Value::as_i64);
    let cps_dt = cps.and_then(|t| chrono::DateTime::from_timestamp(t, 0));
    let cpe_dt = cpe.and_then(|t| chrono::DateTime::from_timestamp(t, 0));

    let final_status = if event_type == "customer.subscription.deleted" {
        "canceled".to_string()
    } else {
        status.to_string()
    };
    sqlx::query(
        r#"
        UPDATE enterprise_subscriptions
        SET status = $1,
            current_period_start = COALESCE($2, current_period_start),
            current_period_end = COALESCE($3, current_period_end),
            cancel_at_period_end = $4,
            updated_at = NOW()
        WHERE stripe_subscription_id = $5
        "#,
    )
    .bind(&final_status)
    .bind(cps_dt)
    .bind(cpe_dt)
    .bind(cancel_at_period_end)
    .bind(stripe_sub_id)
    .execute(&state.db)
    .await?;
    metrics::counter!(
        "skilluv_subscriptions_lifecycle_total",
        "status" => final_status
    )
    .increment(1);
    Ok(())
}

async fn handle_invoice_paid_subscription(state: &AppState, obj: &Value) -> Result<(), AppError> {
    let stripe_sub_id = obj
        .get("subscription")
        .and_then(Value::as_str)
        .unwrap_or("");
    if stripe_sub_id.is_empty() {
        return Ok(());
    }
    // Renewal → nouvelle période, on doit re-grant les crédits mensuels inclus.
    let sub: Option<crate::services::subscriptions::EnterpriseSubscription> =
        sqlx::query_as("SELECT * FROM enterprise_subscriptions WHERE stripe_subscription_id = $1")
            .bind(stripe_sub_id)
            .fetch_optional(&state.db)
            .await?;
    if let Some(sub) = sub {
        let _ = crate::services::subscriptions::grant_monthly_credits_if_due(&state.db, &sub).await;
    }
    Ok(())
}

// ─── Promo codes (3.11) ──────────────────────────────────────────

#[derive(Deserialize)]
struct RedeemBody {
    code: String,
}

/// Redeem a promo code for credits.
#[utoipa::path(
    post, path = "/api/enterprise/credits/redeem", tag = "enterprise",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn redeem_promo(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RedeemBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let code_normalised = body.code.trim().to_uppercase();
    let row: Option<EnterpriseCreditsRow586> = sqlx::query_as(
        "SELECT id, kind, value, max_uses, uses_count, valid_until, valid_from FROM promo_codes WHERE UPPER(code) = $1",
    )
    .bind(&code_normalised)
    .fetch_optional(&state.db)
    .await?;
    let (promo_id, kind, value, max_uses, uses_count, valid_until, valid_from) =
        row.ok_or(AppError::NotFound("promo code not found".into()))?;
    let now = chrono::Utc::now();
    if valid_from > now {
        return Err(AppError::Validation("Promo code not yet active".into()));
    }
    if let Some(vu) = valid_until
        && vu < now
    {
        return Err(AppError::Validation("Promo code expired".into()));
    }
    if let Some(max) = max_uses
        && uses_count >= max
    {
        return Err(AppError::Validation(
            "Promo code has reached max uses".into(),
        ));
    }
    // One redemption per enterprise
    let already: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM promo_code_redemptions WHERE promo_code_id = $1 AND enterprise_id = $2",
    )
    .bind(promo_id)
    .bind(enterprise_id)
    .fetch_optional(&state.db)
    .await?;
    if already.is_some() {
        return Err(AppError::Validation(
            "Already redeemed by this enterprise".into(),
        ));
    }

    match kind.as_str() {
        "bonus_credits" => {
            credits::grant(
                &state.db,
                credits::GrantInput {
                    enterprise_id,
                    amount: &value,
                    reason: "promo_code",
                    related_payment_id: None,
                    related_promo_code_id: Some(promo_id),
                    notes: Some(&format!("Promo {code_normalised}")),
                    actor_user_id: Some(auth.user_id),
                    expires_at: None,
                },
            )
            .await?;
        }
        "percent_off" => {
            // Discount applies on the next Stripe Checkout — kept as a record only ;
            // the actual coupon application logic ties into Stripe Coupons (deferred).
        }
        _ => return Err(AppError::Internal("unknown promo kind".into())),
    }

    sqlx::query(
        "INSERT INTO promo_code_redemptions (promo_code_id, enterprise_id) VALUES ($1, $2)",
    )
    .bind(promo_id)
    .bind(enterprise_id)
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE promo_codes SET uses_count = uses_count + 1 WHERE id = $1")
        .bind(promo_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "redeemed": true,
        "kind": kind,
    }))))
}

// ─── Invoices (3.10) ─────────────────────────────────────────────

/// List enterprise invoices.
#[utoipa::path(
    get, path = "/api/enterprise/invoices", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "enterpriseCreditsListInvoices",
)]
pub async fn list_invoices(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let rows =
        crate::services::invoices::list_for_enterprise(&state.db, enterprise_id, per_page, offset)
            .await?;
    // BE-P0-35 : emit tva_rate_pct (numeric) alongside the default serialization.
    let enriched: Vec<Value> = rows
        .iter()
        .map(|inv| {
            let mut v = serde_json::to_value(inv).unwrap_or(Value::Null);
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "tva_rate_pct".to_string(),
                    json!(crate::services::invoices::tva_rate_as_f64(&inv.tva_rate)),
                );
            }
            v
        })
        .collect();
    Ok(Json(build_response(json!({ "invoices": enriched }))))
}

/// Get a single invoice.
#[utoipa::path(
    get, path = "/api/enterprise/invoices/{id}", tag = "enterprise",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn get_invoice(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let inv = crate::services::invoices::by_id_for_enterprise(&state.db, id, enterprise_id).await?;
    let mut v = serde_json::to_value(&inv).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "tva_rate_pct".to_string(),
            json!(crate::services::invoices::tva_rate_as_f64(&inv.tva_rate)),
        );
    }
    Ok(Json(build_response(json!({ "invoice": v }))))
}

/// Render an invoice as HTML.
#[utoipa::path(
    get, path = "/api/enterprise/invoices/{id}/html", tag = "enterprise",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, description = "HTML content")),
    security(("cookie_auth" = [])),
)]
pub async fn get_invoice_html(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<axum::response::Html<String>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let inv = crate::services::invoices::by_id_for_enterprise(&state.db, id, enterprise_id).await?;
    let row: (String,) = sqlx::query_as("SELECT company_name FROM enterprises WHERE id = $1")
        .bind(enterprise_id)
        .fetch_one(&state.db)
        .await?;
    Ok(axum::response::Html(
        crate::services::invoices::render_html(&inv, &row.0),
    ))
}

// BE-P0-36 / BE-P2-INVOICE-PDF — invoice PDF via external renderer.
//
// The backend stays in Rust land : it renders the same HTML as `/html` then
// POSTs it to a sidecar `skilluv-pdf-renderer` service (Python + weasyprint)
// deployed alongside via Coolify. Splitting the concern avoids pulling
// weasyprint / chromiumoxide / wkhtmltopdf into this image.
//
// Contract (renderer side) : POST {PDF_RENDERER_URL}/render with
//   Content-Type: text/html, body = the HTML
// returns 200 application/pdf on success.
//
// When `PDF_RENDERER_URL` isn't configured we surface a clear 503 rather than
// a broken blob or 500 — the front knows to fall back to browser print.
async fn get_invoice_pdf(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<axum::response::Response, AppError> {
    let Some(renderer_url) = state.config.pdf_renderer_url.as_deref() else {
        return Err(AppError::ServiceUnavailable(
            "pdf renderer not configured (set PDF_RENDERER_URL)".into(),
        ));
    };

    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let inv = crate::services::invoices::by_id_for_enterprise(&state.db, id, enterprise_id).await?;
    let row: (String,) = sqlx::query_as("SELECT company_name FROM enterprises WHERE id = $1")
        .bind(enterprise_id)
        .fetch_one(&state.db)
        .await?;
    let html = crate::services::invoices::render_html(&inv, &row.0);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Internal(format!("pdf client build: {e}")))?;

    let endpoint = format!("{}/render", renderer_url.trim_end_matches('/'));
    let resp = client
        .post(&endpoint)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(html)
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("pdf renderer unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let snippet = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(AppError::Internal(format!(
            "pdf renderer returned {status}: {snippet}"
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("pdf body read: {e}")))?;

    let filename = format!(
        "attachment; filename=\"invoice-{}.pdf\"",
        inv.invoice_number
    );
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/pdf")
        .header("Content-Disposition", filename)
        .body(axum::body::Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("pdf response build: {e}")))
}

// ─── Public pricing (3.14 + 4.4 dynamic multi-currency) ──────────

#[derive(Deserialize)]
struct PricingQuery {
    /// Country ISO2 (used to choose the display currency automatically).
    country: Option<String>,
    /// Explicit currency override (e.g. "USD", "NGN").
    currency: Option<String>,
}

/// Public pricing endpoint (packs + subscriptions).
#[utoipa::path(
    get, path = "/api/pricing", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn public_pricing(
    State(state): State<AppState>,
    Query(q): Query<PricingQuery>,
) -> Result<Json<Value>, AppError> {
    let packs = crate::services::fx::active_packs(&state.db, Some("credits")).await?;
    let subs = crate::services::fx::active_packs(&state.db, Some("subscription")).await?;
    let currency = resolve_currency(q.country.as_deref(), q.currency.as_deref());
    let provider = provider_for_display(&state.db, q.country.as_deref(), &currency)
        .await
        .unwrap_or_else(|| "unavailable".to_string());
    let mut redis = state.redis.clone();

    let mut packs_out: Vec<Value> = Vec::with_capacity(packs.len());
    for p in &packs {
        let conv = crate::services::fx::convert_from_eur(
            &state.db,
            &mut redis,
            &currency,
            p.price_eur_cents,
        )
        .await
        .ok();
        let quote_amount_cents = conv
            .as_ref()
            .map(|c| c.amount_cents)
            .unwrap_or(p.price_eur_cents);
        let price = (quote_amount_cents as f64) / 100.0;
        let per_credit = if p.credit_count > 0 {
            price / (p.credit_count as f64)
        } else {
            0.0
        };
        packs_out.push(json!({
            "slug": p.slug,
            "credits": p.credit_count,
            "kind": p.kind,
            "price": price,
            "price_cents": quote_amount_cents,
            "price_eur": (p.price_eur_cents as f64) / 100.0,
            "per_credit": (per_credit * 100.0).round() / 100.0,
            "fx_rate_applied": conv.as_ref().map(|c| c.rate),
            "fx_margin_pct": conv.as_ref().map(|c| c.margin_applied_pct),
        }));
    }

    let mut subs_out: Vec<Value> = Vec::with_capacity(subs.len());
    for p in &subs {
        let conv = crate::services::fx::convert_from_eur(
            &state.db,
            &mut redis,
            &currency,
            p.price_eur_cents,
        )
        .await
        .ok();
        let quote_amount_cents = conv
            .as_ref()
            .map(|c| c.amount_cents)
            .unwrap_or(p.price_eur_cents);
        subs_out.push(json!({
            "slug": p.slug,
            "credits_included": p.credit_count,
            "price": (quote_amount_cents as f64) / 100.0,
            "price_cents": quote_amount_cents,
            "kind": p.kind,
        }));
    }

    Ok(Json(build_response(json!({
        "currency": currency,
        "psp": provider,
        "packs": packs_out,
        "subscriptions": subs_out,
        "refund_policy": {
            "refused": 0.5,
            "timeout_days": 30,
            "timeout_refund": 0.5,
        }
    }))))
}

/// The currency this country is quoted in.
///
/// The provider used to come back from here too, out of a const array in
/// `psp.rs` that nothing else read. It comes from `collection_routes` now,
/// which is the table that actually decides — a display that disagrees with
/// the routing is worse than no display.
fn resolve_currency(country: Option<&str>, currency: Option<&str>) -> String {
    if let Some(c) = currency {
        return c.to_uppercase();
    }
    let Some(cc) = country else {
        return "EUR".into();
    };
    let cc = cc.to_uppercase();
    // Very small country → currency table for display purposes.
    let currency = match cc.as_str() {
        "NG" => "NGN",
        "GH" => "GHS",
        "EG" => "EGP",
        "ZA" => "ZAR",
        "KE" => "KES",
        "UG" => "UGX",
        "TZ" => "TZS",
        "RW" => "RWF",
        "MA" => "MAD",
        "TN" => "TND",
        "DZ" => "DZD",
        "SN" | "CI" | "BJ" | "BF" | "TG" | "ML" | "NE" | "GW" => "XOF",
        "CM" | "GA" | "CG" | "TD" | "CF" | "GQ" => "XAF",
        "GB" => "GBP",
        "US" => "USD",
        "CA" => "CAD",
        "AU" => "AUD",
        "CH" => "CHF",
        _ => "EUR",
    };
    currency.into()
}

/// Which provider would take this payment, for display.
///
/// Read from the routing table rather than guessed, and `None` when no
/// route matches — a currency we cannot collect should say so on the
/// pricing page rather than at checkout.
async fn provider_for_display(
    db: &sqlx::PgPool,
    country: Option<&str>,
    currency: &str,
) -> Option<String> {
    use crate::services::collect::{Method, routes};
    use crate::services::ledger::Currency;

    let parsed: Currency = currency.parse().ok()?;
    // Card first, because that is what a pricing page implies; falling back
    // to Mobile Money covers the franc zone, where it is the normal way to
    // pay.
    for method in [Method::Card, Method::MobileMoney] {
        if let Ok(found) = routes(db, country, parsed, method).await
            && let Some(route) = found.first()
        {
            return Some(route.provider.clone());
        }
    }
    None
}
