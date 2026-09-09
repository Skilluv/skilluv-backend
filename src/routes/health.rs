//! Health check endpoints (Phase 1.4 + 1.16).
//!
//! - `GET /api/health` : process liveness (no dependency I/O). Used by Docker / k8s.
//! - `GET /api/health/live` : alias of `/api/health`. Kept for Uptime Kuma backward compat.
//! - `GET /api/health/deep` : exhaustive - Postgres + Redis + MinIO + Brevo + WS stats.
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
    /// Always `"live"` - the process is up and event-looping.
    #[schema(example = "live")]
    pub status: &'static str,
    /// `Cargo.toml` package version, baked in at compile time.
    pub version: &'static str,
}

/// Cheap liveness check - no dependency I/O. Used by Docker / k8s /
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
    /// `"ok"`, `"unreachable"`, `"configured"`, or `"disabled"` -
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

/// Exhaustive dependency check - Postgres + Redis + MinIO +
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
    let brevo_status = check_brevo();
    let (ws_connections, ws_rooms, ws_users) = state.ws.stats().await;

    // A deployment with no mail transport reported `healthy`, because this
    // field was rendered and never read. Everything that needs mail is down
    // in that state: email verification, password reset, invitations, the
    // newsletter confirmation link. Degraded and not unhealthy, deliberately:
    // the platform still serves everything else, and returning 503 would take
    // a box out of rotation over something a restart cannot fix.
    let mail_down = brevo_status == "none" && state.config.is_a_real_deployment();

    let critical_ok = pg_status == "ok" && redis_status == "ok";
    let all_ok = critical_ok && minio_status == "ok" && !mail_down;
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

/// Which transport mail actually leaves by, if any.
///
/// It read `BREVO_API_KEY` alone, so a deployment sending happily over SMTP
/// was reported as `disabled` while a deployment sending nothing at all was
/// reported the same way. Two opposite states behind one word, on the page an
/// operator reads to find out whether mail works.
///
/// `EmailService::new` picks SMTP first and falls back to the Brevo API, and
/// this answers in that same order for the same reason: the report has to
/// name the transport that would actually be used.
fn check_brevo() -> &'static str {
    let read = |name: &str| std::env::var(name).ok().filter(|s| !s.is_empty());
    mail_transport(
        read("SMTP_HOST").as_deref(),
        read("BREVO_API_KEY").as_deref(),
    )
}

/// The decision, separated from the environment so it can be tested.
///
/// Reading the variables inline meant the only way to exercise this was to
/// mutate the process environment, which is racy across parallel tests, so
/// the bug below shipped with nothing able to catch it. The wrapper reads,
/// this decides.
fn mail_transport(smtp_host: Option<&str>, brevo_key: Option<&str>) -> &'static str {
    match (smtp_host, brevo_key) {
        (Some(_), _) => "smtp",
        (None, Some(_)) => "brevo",
        // Named for what it costs rather than for what is missing. On a real
        // deployment every send now fails, so this is not a disabled optional
        // feature: email verification, password reset, invitations and the
        // newsletter confirmation link are all down.
        (None, None) => "none",
    }
}

fn uptime_seconds() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs()
}

#[cfg(test)]
mod tests {
    use super::mail_transport;

    /// Two opposite states used to share one word.
    ///
    /// This read `BREVO_API_KEY` alone and answered `disabled` otherwise, so
    /// a box sending happily over SMTP and a box sending nothing at all were
    /// reported identically, on the page an operator opens to find out
    /// whether mail works.
    #[test]
    fn the_report_names_the_transport_that_would_actually_be_used() {
        assert_eq!(mail_transport(Some("smtp.example.com"), None), "smtp");
        assert_eq!(mail_transport(None, Some("xkeysib-...")), "brevo");
        assert_eq!(mail_transport(None, None), "none");

        // SMTP first, because `EmailService::new` picks it first. A report
        // naming the transport that would not be used is worse than none.
        assert_eq!(
            mail_transport(Some("smtp.example.com"), Some("xkeysib-...")),
            "smtp",
            "both configured means SMTP is the one that sends"
        );
    }
}
