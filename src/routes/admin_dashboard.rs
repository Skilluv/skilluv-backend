//! Admin dashboard — Phase 4.15.
//!
//! Consolidated KPIs for platform ops : MRR, financial breakdown, moderation
//! queue counts, current period funnels, ops health.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_dashboard_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/dashboard/overview", get(overview))
        .route("/admin/dashboard/financial", get(financial))
        .route("/admin/dashboard/moderation-queue", get(moderation_queue))
        .route("/admin/dashboard/health", get(ops_health))
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

fn ensure_admin(auth: &AuthUser) -> Result<(), AppError> {
    if auth.role == "admin" {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn overview(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Value>, AppError> {
    ensure_admin(&auth)?;
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
    Ok(Json(build_response(json!({
        "signups_today": signups_today,
        "enterprises_total": enterprises_total,
        "paying_enterprises": paying_enterprises,
        "hires_this_month": hires_this_month,
        "mrr_eur_cents": mrr_cents,
        "refund_rate_pct_30d": (refund_rate * 100.0).round() / 100.0,
    }))))
}

async fn financial(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Value>, AppError> {
    ensure_admin(&auth)?;
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
    use sqlx::Row;
    let packs: Vec<Value> = by_pack
        .iter()
        .map(|r| {
            json!({
                "session_group": r.get::<Option<String>, _>("session_id"),
                "purchases": r.get::<i64, _>("purchases"),
                "credits_total": r.get::<Option<String>, _>("credits_total"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({
        "month_revenue_ttc_cents": month_revenue.0,
        "month_invoices_count": month_revenue.1,
        "primary_currency": month_revenue.2,
        "purchases_breakdown": packs,
    }))))
}

async fn moderation_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    ensure_admin(&auth)?;
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
    Ok(Json(build_response(json!({
        "reports_pending": reports_pending,
        "kyc_pending": kyc_pending,
        "sponsored_requests_pending": sponsored_pending,
        "banned_last_30d": banned_last_30d,
    }))))
}

async fn ops_health(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    ensure_admin(&auth)?;
    let pool_size = state.db.size();
    let pool_idle = state.db.num_idle();
    let ws_stats = state.ws.stats().await;
    let recent_errors_30m: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action LIKE '%.failed' AND created_at > NOW() - INTERVAL '30 minutes'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    Ok(Json(build_response(json!({
        "database": { "pool_size": pool_size, "pool_idle": pool_idle },
        "websocket": {
            "connections": ws_stats.0,
            "rooms": ws_stats.1,
            "users": ws_stats.2,
        },
        "recent_error_events_30m": recent_errors_30m,
    }))))
}
