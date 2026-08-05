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
use std::time::Instant;
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::{ApiResponse, MetaInfo};

pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(liveness))
        .route("/health/live", get(liveness))
        .route("/health/deep", get(deep_health))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LivenessResponse {
    /// Always `"live"` — the process is up and event-looping.
    #[schema(example = "live")]
    pub status: &'static str,
    /// `Cargo.toml` package version, baked in at compile time.
    pub version: &'static str,
}

/// Cheap liveness check — no dependency I/O. Used by Docker / k8s /
/// Uptime Kuma. Sub-millisecond. Also served on `/api/health/live`
/// (undocumented alias kept for Uptime Kuma backward compat).
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "health",
    responses(
        (status = 200, description = "Process is live", body = LivenessResponse),
    ),
)]
pub async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse {
        status: "live",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceHealth {
    /// `"ok"`, `"unreachable"`, `"configured"`, or `"disabled"` —
    /// exact value depends on the service. Frontends should treat any
    /// value other than `"ok"` / `"configured"` as unhealthy.
    #[schema(example = "ok")]
    pub status: String,
    /// Round-trip time to the dependency, in milliseconds. `None` for
    /// synchronous checks that don't measure latency (e.g. Brevo config).
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthServices {
    pub postgres: ServiceHealth,
    pub redis: ServiceHealth,
    pub minio: ServiceHealth,
    pub judge0: ServiceHealth,
    pub brevo: ServiceHealth,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebsocketStats {
    pub connections: usize,
    pub rooms: usize,
    pub users: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeepHealthResponse {
    /// One of `"healthy"`, `"degraded"`, `"unhealthy"`. HTTP status
    /// is 200 when critical (postgres + redis) are OK, otherwise 503.
    #[schema(example = "healthy")]
    pub status: String,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub services: HealthServices,
    pub websocket: WebsocketStats,
}

/// Exhaustive dependency check — Postgres + Redis + MinIO + Judge0 +
/// Brevo + WebSocket stats. Returns 200 when critical deps (postgres +
/// redis) are OK, 503 otherwise. Takes up to ~3-5s when a dep is slow.
#[utoipa::path(
    get,
    path = "/api/health/deep",
    tag = "health",
    responses(
        (status = 200, description = "All critical deps healthy (may still be degraded)", body = ApiResponse<DeepHealthResponse>),
        (status = 503, description = "Critical dependency unreachable", body = ApiResponse<DeepHealthResponse>),
    ),
)]
pub async fn deep_health(State(state): State<AppState>) -> impl IntoResponse {
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

    let body = ApiResponse {
        data: DeepHealthResponse {
            status: overall.to_string(),
            version: env!("CARGO_PKG_VERSION"),
            uptime_seconds: uptime_seconds(),
            services: HealthServices {
                postgres: ServiceHealth {
                    status: pg_status.to_string(),
                    latency_ms: pg_ms,
                },
                redis: ServiceHealth {
                    status: redis_status.to_string(),
                    latency_ms: redis_ms,
                },
                minio: ServiceHealth {
                    status: minio_status.to_string(),
                    latency_ms: minio_ms,
                },
                judge0: ServiceHealth {
                    status: judge0_status.to_string(),
                    latency_ms: judge0_ms,
                },
                brevo: ServiceHealth {
                    status: brevo_status.to_string(),
                    latency_ms: None,
                },
            },
            websocket: WebsocketStats {
                connections: ws_connections,
                rooms: ws_rooms,
                users: ws_users,
            },
        },
        meta: MetaInfo::now(),
    };
    (http_code, Json(body))
}

async fn check_postgres(state: &AppState) -> (&'static str, Option<u64>) {
    let start = Instant::now();
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => ("ok", Some(start.elapsed().as_millis() as u64)),
        Err(_) => ("unreachable", Some(start.elapsed().as_millis() as u64)),
    }
}

async fn check_redis(state: &AppState) -> (&'static str, Option<u64>) {
    let start = Instant::now();
    match redis::cmd("PING")
        .query_async::<String>(&mut state.redis.clone())
        .await
    {
        Ok(_) => ("ok", Some(start.elapsed().as_millis() as u64)),
        Err(_) => ("unreachable", Some(start.elapsed().as_millis() as u64)),
    }
}

async fn check_minio(state: &AppState) -> (&'static str, Option<u64>) {
    let start = Instant::now();
    // Light check : a generic HEAD on the storage endpoint root would be ideal.
    // For now we presign a fake key and just assert the URL builder doesn't error.
    match state.storage.presigned_get_url("__healthcheck__", 1).await {
        Ok(_) => ("ok", Some(start.elapsed().as_millis() as u64)),
        Err(_) => ("unreachable", Some(start.elapsed().as_millis() as u64)),
    }
}

async fn check_judge0(state: &AppState) -> (&'static str, Option<u64>) {
    let start = Instant::now();
    if state.sandbox.health_check().await {
        ("ok", Some(start.elapsed().as_millis() as u64))
    } else {
        ("unreachable", Some(start.elapsed().as_millis() as u64))
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
