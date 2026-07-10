use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use serde_json::json;
use sqlx::PgPool;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

use crate::AppState;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Initialize the Prometheus metrics recorder. Call once at startup.
pub fn init_metrics() {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder");
    PROMETHEUS_HANDLE
        .set(handle)
        .expect("Metrics already initialized");
}

/// Spawn a periodic task that refreshes business gauges every 60s.
/// Read-only queries against the live DB. Idempotent, restart-safe.
pub fn start_business_gauges(db: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            if let Err(err) = refresh_business_gauges(&db).await {
                tracing::warn!(error = %err, "business gauges refresh failed");
            }
        }
    });
}

async fn refresh_business_gauges(db: &PgPool) -> Result<(), sqlx::Error> {
    let users_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(db).await?;
    let users_active_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT user_id) FROM user_activity WHERE activity_date >= CURRENT_DATE - INTERVAL '1 day'",
    )
    .fetch_one(db)
    .await?;
    let challenges_in_progress: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions WHERE status = 'in_progress'",
    )
    .fetch_one(db)
    .await?;
    let enterprises_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM enterprises")
        .fetch_one(db)
        .await?;
    let pending_reports: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reports WHERE status = 'pending'",
    )
    .fetch_one(db)
    .await?;
    let active_conversations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE closed = FALSE")
            .fetch_one(db)
            .await?;

    metrics::gauge!("skilluv_users_total").set(users_total as f64);
    metrics::gauge!("skilluv_users_active_24h").set(users_active_24h as f64);
    metrics::gauge!("skilluv_challenges_in_progress").set(challenges_in_progress as f64);
    metrics::gauge!("skilluv_enterprises_total").set(enterprises_total as f64);
    metrics::gauge!("skilluv_reports_pending").set(pending_reports as f64);
    metrics::gauge!("skilluv_conversations_active").set(active_conversations as f64);
    Ok(())
}

pub fn metrics_routes() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(prometheus_metrics))
        .route("/api/metrics/summary", get(metrics_summary))
}

/// GET /metrics — Prometheus scrape endpoint (text format)
///
/// If `METRICS_TOKEN` is set in env, requires `Authorization: Bearer <token>`.
/// Otherwise public (dev convenience).
async fn prometheus_metrics(headers: HeaderMap) -> impl IntoResponse {
    if let Ok(expected) = std::env::var("METRICS_TOKEN") {
        if !expected.is_empty() {
            let provided = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .unwrap_or("");
            if provided != expected {
                return (StatusCode::UNAUTHORIZED, String::new()).into_response();
            }
        }
    }
    let body = PROMETHEUS_HANDLE
        .get()
        .map(|h| h.render())
        .unwrap_or_default();
    (StatusCode::OK, body).into_response()
}

/// GET /api/metrics/summary — JSON summary for internal dashboards
async fn metrics_summary(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, crate::errors::AppError> {
    // DB stats
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;

    let active_users: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE profile_active = TRUE AND is_banned = FALSE",
    )
    .fetch_one(&state.db)
    .await?;

    let total_submissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenge_submissions")
        .fetch_one(&state.db)
        .await?;

    let today_submissions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions WHERE started_at > NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&state.db)
    .await?;

    let total_challenges: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenge_templates WHERE status = 'published'")
            .fetch_one(&state.db)
            .await?;

    let total_enterprises: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM enterprises")
        .fetch_one(&state.db)
        .await?;

    let pending_reports: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reports WHERE status = 'pending'")
            .fetch_one(&state.db)
            .await?;

    let active_conversations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE closed = FALSE")
            .fetch_one(&state.db)
            .await?;

    let today_active_users: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT user_id) FROM user_activity WHERE activity_date = CURRENT_DATE",
    )
    .fetch_one(&state.db)
    .await?;

    // WebSocket stats
    let (ws_connections, ws_rooms, ws_users) = state.ws.stats().await;

    // DB pool stats
    let pool_size = state.db.size();
    let pool_idle = state.db.num_idle();

    Ok(Json(json!({
        "data": {
            "users": {
                "total": total_users,
                "active": active_users,
                "today_active": today_active_users,
            },
            "challenges": {
                "published": total_challenges,
                "total_submissions": total_submissions,
                "today_submissions": today_submissions,
            },
            "enterprises": total_enterprises,
            "moderation": {
                "pending_reports": pending_reports,
            },
            "messaging": {
                "active_conversations": active_conversations,
            },
            "websocket": {
                "connections": ws_connections,
                "rooms": ws_rooms,
                "users": ws_users,
            },
            "database": {
                "pool_size": pool_size,
                "pool_idle": pool_idle,
            },
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}
