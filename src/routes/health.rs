//! Health check endpoints (Phase 1.4 + 1.16).
//!
//! - `GET /api/health` : process liveness (no dependency I/O). Used by Docker / k8s.
//! - `GET /api/health/live` : alias of `/api/health`. Kept for Uptime Kuma backward compat.
//! - `GET /api/health/deep` : exhaustive — Postgres + Redis + MinIO + Judge0 + Brevo + WS stats.
//!
//! Sub-millisecond on the basic path. Up to ~3-5s on /deep when external deps are slow.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Instant;

use crate::AppState;

pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(liveness))
        .route("/health/live", get(liveness))
        .route("/health/deep", get(deep_health))
}

#[derive(Serialize)]
struct LivenessResponse {
    status: &'static str,
    version: &'static str,
}

async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse {
        status: "live",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn deep_health(State(state): State<AppState>) -> impl IntoResponse {
    let (pg_status, pg_ms) = check_postgres(&state).await;
    let (redis_status, redis_ms) = check_redis(&state).await;
    let (minio_status, minio_ms) = check_minio(&state).await;
    let (judge0_status, judge0_ms) = check_judge0(&state).await;
    let brevo_status = check_brevo();
    let (ws_connections, ws_rooms, ws_users) = state.ws.stats().await;

    let critical_ok = pg_status == "ok" && redis_status == "ok";
    let all_ok = critical_ok && minio_status == "ok" && judge0_status == "ok";
    let (overall, http_code) = if all_ok {
        ("healthy", StatusCode::OK)
    } else if critical_ok {
        ("degraded", StatusCode::OK)
    } else {
        ("unhealthy", StatusCode::SERVICE_UNAVAILABLE)
    };

    let body = json!({
        "data": {
            "status": overall,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": uptime_seconds(),
            "services": {
                "postgres": { "status": pg_status, "latency_ms": pg_ms },
                "redis": { "status": redis_status, "latency_ms": redis_ms },
                "minio": { "status": minio_status, "latency_ms": minio_ms },
                "judge0": { "status": judge0_status, "latency_ms": judge0_ms },
                "brevo": { "status": brevo_status, "latency_ms": Value::Null },
            },
            "websocket": {
                "connections": ws_connections,
                "rooms": ws_rooms,
                "users": ws_users,
            }
        },
        "meta": {
            "request_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    });
    (http_code, Json(body))
}

async fn check_postgres(state: &AppState) -> (&'static str, Option<u128>) {
    let start = Instant::now();
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => ("ok", Some(start.elapsed().as_millis())),
        Err(_) => ("unreachable", Some(start.elapsed().as_millis())),
    }
}

async fn check_redis(state: &AppState) -> (&'static str, Option<u128>) {
    let start = Instant::now();
    match redis::cmd("PING")
        .query_async::<String>(&mut state.redis.clone())
        .await
    {
        Ok(_) => ("ok", Some(start.elapsed().as_millis())),
        Err(_) => ("unreachable", Some(start.elapsed().as_millis())),
    }
}

async fn check_minio(state: &AppState) -> (&'static str, Option<u128>) {
    let start = Instant::now();
    // Light check : a generic HEAD on the storage endpoint root would be ideal.
    // For now we presign a fake key and just assert the URL builder doesn't error.
    match state.storage.presigned_get_url("__healthcheck__", 1).await {
        Ok(_) => ("ok", Some(start.elapsed().as_millis())),
        Err(_) => ("unreachable", Some(start.elapsed().as_millis())),
    }
}

async fn check_judge0(state: &AppState) -> (&'static str, Option<u128>) {
    let start = Instant::now();
    if state.sandbox.health_check().await {
        ("ok", Some(start.elapsed().as_millis()))
    } else {
        ("unreachable", Some(start.elapsed().as_millis()))
    }
}

fn check_brevo() -> &'static str {
    if std::env::var("BREVO_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some()
    {
        "configured"
    } else {
        "disabled"
    }
}

fn uptime_seconds() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs()
}
