//! Admin dashboard — Phase 4.15.
//!
//! Consolidated KPIs for platform ops : MRR, financial breakdown, moderation
//! queue counts, current period funnels, ops health.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::Row;
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_dashboard_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/dashboard/overview", get(overview))
        .route("/admin/dashboard/financial", get(financial))
        .route("/admin/dashboard/moderation-queue", get(moderation_queue))
        .route("/admin/dashboard/health", get(ops_health))
}

/// The one admin check, delegated.
///
/// This file used to read `auth.role` out of the JWT, which stopped being the
/// answer at P21 when the gate moved to `user_capabilities`. The two disagreed
/// in both directions: somebody granted `admin` was refused here, and somebody
/// whose capability had been revoked still got in as long as the column and
/// their token said `admin`. Revoking a capability has to close every door.
async fn ensure_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::routes::admin::require_admin(state, auth).await
}

// ─── Types de réponse ────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOverviewResponse {
    pub signups_today: i64,
    pub enterprises_total: i64,
    pub paying_enterprises: i64,
    pub hires_this_month: i64,
    pub mrr_eur_cents: i64,
    /// Refund rate (%) over the last 30 days.
    pub refund_rate_pct_30d: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PurchaseBreakdownRow {
    pub session_group: Option<String>,
    pub purchases: i64,
    pub credits_total: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminFinancialResponse {
    pub month_revenue_ttc_cents: i64,
    pub month_invoices_count: i64,
    pub primary_currency: String,
    pub purchases_breakdown: Vec<PurchaseBreakdownRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModerationQueueResponse {
    pub reports_pending: i64,
    pub kyc_pending: i64,
    pub sponsored_requests_pending: i64,
    pub banned_last_30d: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DbPoolInfo {
    pub pool_size: u32,
    pub pool_idle: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WsInfo {
    pub connections: usize,
    pub rooms: usize,
    pub users: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OpsHealthResponse {
    pub database: DbPoolInfo,
    pub websocket: WsInfo,
    pub recent_error_events_30m: i64,
}

/// Admin overview KPIs: signups, enterprises, hires, MRR, refund rate.
#[utoipa::path(
    get,
    path = "/api/admin/dashboard/overview",
    tag = "admin",
    responses(
        (status = 200, description = "Overview KPIs", body = ApiResponse<AdminOverviewResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "adminDashboardOverview",
)]
pub async fn overview(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<AdminOverviewResponse>>, AppError> {
    ensure_admin(&state, &auth).await?;
    let signups_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE created_at >= date_trunc('day', NOW())",
    )
    .fetch_one(&state.db)
    .await?;
    let enterprises_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM enterprises")
        .fetch_one(&state.db)
        .await?;
    let paying_enterprises: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT enterprise_id) FROM credit_transactions WHERE reason = 'purchase'",
    )
    .fetch_one(&state.db)
    .await?;
    let hires_this_month: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enterprise_pipeline_entries WHERE stage = 'hired' AND updated_at >= date_trunc('month', NOW())",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    // MRR = sum of active subscriptions' plan price
    let mrr_cents: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(pp.price_eur_cents), 0)::BIGINT
        FROM enterprise_subscriptions es
        JOIN pricing_packs pp ON pp.slug = es.plan_slug
        WHERE es.status IN ('trialing', 'active', 'past_due')
        "#,
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    let refund_rate_30d: (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE reason LIKE 'refund_%' AND created_at > NOW() - INTERVAL '30 days')::BIGINT,
            COUNT(*) FILTER (WHERE reason = 'spend_interest_request' AND created_at > NOW() - INTERVAL '30 days')::BIGINT
        FROM credit_transactions
        "#,
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0));
    let refund_rate = if refund_rate_30d.1 > 0 {
        (refund_rate_30d.0 as f64) / (refund_rate_30d.1 as f64) * 100.0
    } else {
        0.0
    };
    Ok(Json(ApiResponse::new(AdminOverviewResponse {
        signups_today,
        enterprises_total,
        paying_enterprises,
        hires_this_month,
        mrr_eur_cents: mrr_cents,
        refund_rate_pct_30d: (refund_rate * 100.0).round() / 100.0,
    })))
}

/// Admin financial KPIs: month revenue, invoices count, breakdown per
/// purchase session group.
#[utoipa::path(
    get,
    path = "/api/admin/dashboard/financial",
    tag = "admin",
    responses(
        (status = 200, description = "Financial KPIs", body = ApiResponse<AdminFinancialResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn financial(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<AdminFinancialResponse>>, AppError> {
    ensure_admin(&state, &auth).await?;
    // Revenue this month (from invoices)
    let month_revenue: (i64, i64, String) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(amount_ttc_cents), 0)::BIGINT,
            COUNT(*)::BIGINT,
            COALESCE(MAX(currency), 'EUR')
        FROM invoices
        WHERE issued_at >= date_trunc('month', NOW())
        "#,
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, "EUR".into()));
    // Credits sold per pack this month
    let by_pack: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT SUBSTRING(notes FROM 'session=(.*)') AS session_id,
               COUNT(*) AS purchases, SUM(delta)::TEXT AS credits_total
        FROM credit_transactions
        WHERE reason = 'purchase' AND created_at >= date_trunc('month', NOW())
        GROUP BY 1
        "#,
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let packs: Vec<PurchaseBreakdownRow> = by_pack
        .iter()
        .map(|r| PurchaseBreakdownRow {
            session_group: r.get("session_id"),
            purchases: r.get("purchases"),
            credits_total: r.get("credits_total"),
        })
        .collect();
    Ok(Json(ApiResponse::new(AdminFinancialResponse {
        month_revenue_ttc_cents: month_revenue.0,
        month_invoices_count: month_revenue.1,
        primary_currency: month_revenue.2,
        purchases_breakdown: packs,
    })))
}

/// Moderation queue depth: pending reports, KYC pending, sponsored
/// requests in review, bans in the last 30 days.
#[utoipa::path(
    get,
    path = "/api/admin/dashboard/moderation-queue",
    tag = "admin",
    responses(
        (status = 200, description = "Moderation queue snapshot", body = ApiResponse<ModerationQueueResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn moderation_queue(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<ModerationQueueResponse>>, AppError> {
    ensure_admin(&state, &auth).await?;
    let reports_pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reports WHERE status = 'pending'")
            .fetch_one(&state.db)
            .await?;
    let kyc_pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM enterprise_kyc WHERE status = 'pending'")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
    let sponsored_pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sponsored_challenge_requests WHERE status IN ('pending', 'negotiating')",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    let banned_last_30d: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM users
        WHERE is_banned = TRUE AND updated_at > NOW() - INTERVAL '30 days'
        "#,
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    Ok(Json(ApiResponse::new(ModerationQueueResponse {
        reports_pending,
        kyc_pending,
        sponsored_requests_pending: sponsored_pending,
        banned_last_30d,
    })))
}

/// Ops health: DB pool, WS stats, recent .failed audit events.
#[utoipa::path(
    get,
    path = "/api/admin/dashboard/health",
    tag = "admin",
    responses(
        (status = 200, description = "Ops health snapshot", body = ApiResponse<OpsHealthResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn ops_health(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<OpsHealthResponse>>, AppError> {
    ensure_admin(&state, &auth).await?;
    let pool_size = state.db.size();
    let pool_idle = state.db.num_idle();
    let ws_stats = state.ws.stats().await;
    let recent_errors_30m: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action LIKE '%.failed' AND created_at > NOW() - INTERVAL '30 minutes'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    Ok(Json(ApiResponse::new(OpsHealthResponse {
        database: DbPoolInfo {
            pool_size,
            pool_idle,
        },
        websocket: WsInfo {
            connections: ws_stats.0,
            rooms: ws_stats.1,
            users: ws_stats.2,
        },
        recent_error_events_30m: recent_errors_30m,
    })))
}
