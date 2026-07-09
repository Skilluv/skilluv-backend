#![recursion_limit = "512"]

rust_i18n::i18n!("locales", fallback = "en");

pub mod config;
pub mod errors;
pub mod grpc;
pub mod middleware;
pub mod models;
pub mod observability;
pub mod routes;
pub mod services;
pub mod validators;
pub mod websocket;

use std::sync::Arc;

use axum::Router;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use grpc::AiClient;
use services::{
    AnalyticsService, EmailService, GeoService, QueueService, SandboxService, StorageService,
    WebauthnService,
};
use websocket::WsManager;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub config: AppStateConfig,
    pub sandbox: Arc<SandboxService>,
    pub storage: Arc<StorageService>,
    pub email: Arc<EmailService>,
    pub ai: Option<Arc<AiClient>>,
    pub queue: Arc<QueueService>,
    pub geo: Arc<GeoService>,
    pub analytics: AnalyticsService,
    pub ws: WsManager,
    pub webauthn: Arc<WebauthnService>,
}

#[derive(Clone)]
pub struct AppStateConfig {
    pub jwt_secret: String,
    pub base_url: String,
    /// 32-byte AES-256-GCM key for enterprise SSO client_secret at-rest encryption.
    /// `None` in dev when the env var is unset ; SSO endpoints error out cleanly.
    pub sso_encryption_key: Option<[u8; 32]>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .nest("/api", routes::health_routes())
        .nest("/api", routes::auth_routes())
        .nest("/api", routes::email_prefs_routes())
        .nest("/api", routes::challenge_routes())
        .nest("/api", routes::sandbox_routes())
        .nest("/api", routes::admin_routes())
        .nest("/api", routes::gamification_routes())
        .nest("/api", routes::geo_routes())
        .nest("/api", routes::legal_routes())
        .nest("/api", routes::i18n_routes())
        .nest("/api", routes::social_routes())
        .nest("/api", routes::dm_routes())
        .nest("/api", routes::feed_routes())
        .nest("/api", routes::forum_routes())
        .nest("/api", routes::guild_routes())
        .nest("/api", routes::github_routes())
        .nest("/api", routes::project_routes())
        .nest("/api", routes::tournament_routes())
        .nest("/api", routes::leaderboard_routes())
        .nest("/api", routes::profile_routes())
        .nest("/api", routes::enterprise_routes())
        .nest("/api", routes::enterprise_sso_routes())
        .nest("/api", routes::scim_routes())
        .nest("/api", routes::talent_search_routes())
        .nest("/api", routes::talent_list_routes())
        .nest("/api", routes::contact_routes())
        .nest("/api", routes::notification_routes())
        .nest("/api", routes::enterprise_dashboard_routes())
        .nest("/api", routes::user_profile_routes())
        .nest("/api", routes::profile_extras_routes())
        .nest("/api", routes::oauth_routes())
        .nest("/api", routes::report_routes())
        .nest("/api", routes::admin_moderation_routes())
        .nest("/api", routes::challenge_tag_routes())
        .nest("/api", routes::community_routes())
        .nest("/api", routes::admin_community_routes())
        .nest("/api", routes::challenge_team_routes())
        .nest("/api", routes::developer_routes())
        .nest("/api", routes::public_api_routes())
        .nest("/api", routes::openapi_routes())
        .nest("/api", routes::sponsored_routes())
        .nest("/api", routes::enterprise_credits_routes())
        .nest("/api", routes::enterprise_pipeline_routes())
        .nest("/api", routes::enterprise_kyc_routes())
        .nest("/api", routes::talent_search_v2_routes())
        .nest("/api", routes::magic_link_routes())
        .nest("/api", routes::webauthn_routes())
        .nest("/api", routes::push_routes())
        .nest("/api", routes::admin_dashboard_routes())
        // Phase 5
        .nest("/api", routes::bounty_routes())
        .nest("/api", routes::certification_routes())
        .nest("/api", routes::mentorship_routes())
        .nest("/api", routes::tenant_routes())
        .nest("/api", routes::ai_job_routes())
        .nest("/api", routes::enterprise_subscription_routes())
        .merge(routes::well_known_routes().with_state(state.clone()))
        .merge(routes::metrics_routes().with_state(state.clone()))
        .merge(websocket::ws_routes().with_state(state.clone()))
        .layer(middleware::SecurityHeadersLayer)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        // Sentry layers (outermost so they wrap everything). NewSentryLayer creates a
        // per-request Hub so user/tag context set in handlers doesn't leak across requests.
        .layer(sentry_tower::SentryHttpLayer::with_transaction())
        .layer(sentry_tower::NewSentryLayer::new_from_top())
        .with_state(state)
}
