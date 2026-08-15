//! The money an operator has to be able to see and act on.
//!
//! Everything under `/admin/*` here answers a question somebody asks at
//! nine in the morning, and every one of them was previously unanswerable
//! without a psql prompt:
//!
//! * Who paid and received nothing?
//! * Which payouts are stuck, and how long have they been stuck?
//! * Do our books still agree with the provider's?
//! * Which corridors are open, and can I close one right now?
//!
//! ## Read-mostly on purpose
//!
//! Only two things here write, and both are reversals of a decision rather
//! than a movement of money: enabling or disabling a route. Nothing in this
//! module moves a balance, refunds a charge or releases a hold. An operator
//! panel that can move money is an operator panel that will, at three in
//! the morning, on the wrong row.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_money_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/money/overview", get(overview))
        .route("/admin/money/payments", get(payments))
        .route("/admin/money/payouts", get(payouts))
        .route("/admin/money/routes", get(routes))
        .route("/admin/money/routes/{id}/toggle", post(toggle_route))
        .route("/admin/money/methods", get(methods))
        .route("/admin/money/methods/{id}/toggle", post(toggle_method))
}

/// One number per thing that can be wrong.
///
/// Deliberately small. A dashboard with forty figures is one nobody reads,
/// and each of these is a count that should be zero or near it.
#[derive(Debug, Serialize, ToSchema)]
pub struct Overview {
    /// Paid, and the thing paid for does not exist. The worst state in the
    /// system: the customer is the only one who knows.
    pub paid_but_undelivered: i64,
    /// Checkouts still open past the point where they usually settle.
    pub payments_pending: i64,
    /// Payouts the provider has not confirmed.
    pub payouts_pending: i64,
    /// Payouts that were given up on and handed to a person.
    pub payouts_failed_today: i64,
    /// Disputes waiting on a human, because the two sides disagree.
    pub disputes_awaiting_decision: i64,
    /// Notifications that could not be delivered on any channel after
    /// every retry.
    pub notifications_abandoned: i64,
    /// Ledger accounts whose running total disagrees with their own
    /// entries. Must be zero — anything else means balances are wrong.
    pub ledger_snapshot_drift: i64,
    /// What each provider is holding, per our books.
    pub provider_positions: Vec<ProviderPosition>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderPosition {
    pub account_code: String,
    pub currency: String,
    pub balance: String,
}

/// The figures worth waking up to.
#[utoipa::path(
    get, path = "/api/admin/money/overview", tag = "admin",
    responses(
        (status = 200, description = "Counts and positions", body = ApiResponse<Overview>),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn overview(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Overview>>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    // One round trip. Seven scalar subqueries on indexed predicates is
    // cheaper than seven round trips, and this is polled by a dashboard.
    let counts: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM payments
              WHERE status = 'succeeded' AND fulfilled_at IS NULL),
            (SELECT COUNT(*) FROM payments WHERE status = 'pending'),
            (SELECT COUNT(*) FROM payouts WHERE status = 'pending'),
            (SELECT COUNT(*) FROM payouts
              WHERE status = 'failed' AND settled_at > NOW() - INTERVAL '24 hours'),
            (SELECT COUNT(*) FROM disputes WHERE status = 'contested'),
            (SELECT COUNT(*) FROM notification_outbox WHERE status = 'abandoned'),
            (SELECT COUNT(*) FROM ledger_verify_balances())",
    )
    .fetch_one(&state.db)
    .await?;

    let positions: Vec<(String, String, bigdecimal::BigDecimal)> = sqlx::query_as(
        "SELECT account_code, currency, balance FROM ledger_provider_positions
          ORDER BY account_code",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(Overview {
        paid_but_undelivered: counts.0,
        payments_pending: counts.1,
        payouts_pending: counts.2,
        payouts_failed_today: counts.3,
        disputes_awaiting_decision: counts.4,
        notifications_abandoned: counts.5,
        ledger_snapshot_drift: counts.6,
        provider_positions: positions
            .into_iter()
            .map(|(account_code, currency, balance)| ProviderPosition {
                account_code,
                currency,
                // Text, not a float: money read off a screen must be the
                // number in the database, not a rounding of it.
                balance: balance.to_string(),
            })
            .collect(),
    })))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MoneyQuery {
    /// `pending`, `succeeded`, `failed`, `refunded`. Omit for all.
    pub status: Option<String>,
    /// Only rows where money arrived and nothing was delivered.
    pub undelivered: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub provider: String,
    pub method: String,
    pub operator: Option<String>,
    pub amount: String,
    pub currency: String,
    pub status: String,
    /// Their identifier, for looking the charge up in the provider's own
    /// dashboard — which is where an operator goes next.
    pub provider_reference: Option<String>,
    /// Ours, which is what a poller recovers a lost payment by.
    pub merchant_reference: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub succeeded_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Null on a succeeded payment means money taken and nothing given.
    pub fulfilled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub check_count: i32,
}

/// Money coming in.
#[utoipa::path(
    get, path = "/api/admin/money/payments", tag = "admin",
    params(MoneyQuery),
    responses(
        (status = 200, description = "Payments", body = serde_json::Value),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn payments(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MoneyQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let (per_page, offset) = paging(&q);

    let rows: Vec<PaymentRow> = sqlx::query_as(
        "SELECT id, subject_type, subject_id, provider, method, operator,
                amount::TEXT AS amount, currency, status,
                provider_reference, merchant_reference, failure_reason,
                created_at, succeeded_at, fulfilled_at, check_count
           FROM payments
          WHERE ($1::text IS NULL OR status = $1)
            AND ($2::bool IS NOT TRUE OR (status = 'succeeded' AND fulfilled_at IS NULL))
          ORDER BY created_at DESC
          LIMIT $3 OFFSET $4",
    )
    .bind(q.status.as_deref())
    .bind(q.undelivered)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "data": { "payments": rows } })))
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct PayoutRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub rail: String,
    pub amount: String,
    pub currency: String,
    pub status: String,
    /// Masked at write time. Enough to recognise, not enough to use.
    pub destination_masked: Option<String>,
    pub provider_reference: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub settled_at: Option<chrono::DateTime<chrono::Utc>>,
    /// How many times the sweep has asked the provider. A high number on a
    /// still-pending payout is the signal that it will not resolve itself.
    pub check_count: i32,
}

/// Money going out.
#[utoipa::path(
    get, path = "/api/admin/money/payouts", tag = "admin",
    params(MoneyQuery),
    responses(
        (status = 200, description = "Payouts", body = serde_json::Value),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn payouts(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MoneyQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let (per_page, offset) = paging(&q);

    let rows: Vec<PayoutRow> = sqlx::query_as(
        "SELECT id, user_id, provider, rail, amount::TEXT AS amount, currency, status,
                destination_masked, provider_reference, failure_reason,
                created_at, settled_at, check_count
           FROM payouts
          WHERE ($1::text IS NULL OR status = $1)
          ORDER BY created_at DESC
          LIMIT $2 OFFSET $3",
    )
    .bind(q.status.as_deref())
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "data": { "payouts": rows } })))
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct RouteRow {
    pub id: Uuid,
    /// `in` for collection, `out` for payout. One list rather than two
    /// screens: an operator closing a corridor during an outage wants both
    /// directions in front of them.
    pub direction: String,
    pub country: Option<String>,
    pub currency: String,
    /// `card` / `mobile_money` / `bank_transfer` on the way in, the rail on
    /// the way out.
    pub method: String,
    pub provider: String,
    pub priority: i16,
    pub enabled: bool,
    pub notes: Option<String>,
}

/// Which corridors are open, both directions.
#[utoipa::path(
    get, path = "/api/admin/money/routes", tag = "admin",
    responses(
        (status = 200, description = "Routes", body = serde_json::Value),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn routes(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let rows: Vec<RouteRow> = sqlx::query_as(
        "SELECT id, 'in' AS direction, country, currency, method, provider,
                priority, enabled, notes
           FROM collection_routes
         UNION ALL
         SELECT id, 'out' AS direction, country, currency, rail AS method, provider,
                priority, enabled, notes
           FROM payout_routes
          ORDER BY direction, currency, country NULLS LAST, priority",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "data": { "routes": rows } })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ToggleRequest {
    pub enabled: bool,
    /// `in` or `out`. Required because the two tables have separate id
    /// spaces and guessing would eventually disable the wrong corridor.
    pub direction: String,
}

/// Open or close one corridor.
///
/// The one thing an operator needs during a provider outage, and the
/// reason `enabled` is a column rather than a deployment.
#[utoipa::path(
    post, path = "/api/admin/money/routes/{id}/toggle", tag = "admin",
    params(("id" = Uuid, Path, description = "Route id")),
    request_body = ToggleRequest,
    responses(
        (status = 200, description = "Toggled", body = serde_json::Value),
        (status = 400, description = "Unknown direction", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn toggle_route(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ToggleRequest>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let table = match body.direction.as_str() {
        "in" => "collection_routes",
        "out" => "payout_routes",
        other => {
            return Err(AppError::Validation(format!(
                "direction must be 'in' or 'out', not '{other}'"
            )));
        }
    };

    // `table` comes from the match above and can only be one of two
    // literals, so interpolating it cannot inject.
    let sql = format!("UPDATE {table} SET enabled = $2 WHERE id = $1");
    let changed = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(body.enabled)
        .execute(&state.db)
        .await?;

    if changed.rows_affected() == 0 {
        return Err(AppError::NotFound("route".into()));
    }

    // Closing a corridor is the kind of thing someone should be able to
    // explain a week later.
    tracing::warn!(
        route = %id,
        direction = %body.direction,
        enabled = body.enabled,
        operator = %auth.user_id,
        "payment corridor toggled"
    );

    Ok(Json(json!({ "data": { "enabled": body.enabled } })))
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct MethodRow {
    pub id: Uuid,
    pub provider: String,
    pub country: String,
    pub currency: String,
    pub operator: String,
    pub label: String,
    pub provider_mode: String,
    /// Whether the payer can confirm without leaving the page.
    pub supports_inline: bool,
    pub enabled: bool,
    pub sort_order: i16,
}

/// The operators a payer can be offered.
#[utoipa::path(
    get, path = "/api/admin/money/methods", tag = "admin",
    responses(
        (status = 200, description = "Payment methods", body = serde_json::Value),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn methods(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let rows: Vec<MethodRow> = sqlx::query_as(
        "SELECT id, provider, country, currency, operator, label, provider_mode,
                supports_inline, enabled, sort_order
           FROM payment_methods
          ORDER BY country, sort_order, label",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "data": { "methods": rows } })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MethodToggleRequest {
    pub enabled: bool,
}

/// Turn one operator on or off.
///
/// Same reasoning as a route: an operator having a bad day should be one
/// column, reversible, with no deployment. It is also how the sandbox rail
/// gets enabled on a staging deployment without touching production.
#[utoipa::path(
    post, path = "/api/admin/money/methods/{id}/toggle", tag = "admin",
    params(("id" = Uuid, Path, description = "Method id")),
    request_body = MethodToggleRequest,
    responses(
        (status = 200, description = "Toggled", body = serde_json::Value),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn toggle_method(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MethodToggleRequest>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let changed = sqlx::query("UPDATE payment_methods SET enabled = $2 WHERE id = $1")
        .bind(id)
        .bind(body.enabled)
        .execute(&state.db)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(AppError::NotFound("payment method".into()));
    }

    tracing::warn!(
        method = %id,
        enabled = body.enabled,
        operator = %auth.user_id,
        "payment method toggled"
    );
    Ok(Json(json!({ "data": { "enabled": body.enabled } })))
}

/// Page size, clamped. An operator list is read, not exported.
fn paging(q: &MoneyQuery) -> (i64, i64) {
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    (per_page, (page - 1) * per_page)
}
