use std::sync::Arc;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use skilluv_backend::config::{AppConfig, DatabaseConfig, RedisConfig};
use skilluv_backend::grpc::AiClient;
use skilluv_backend::observability;
use skilluv_backend::services::{
    AnalyticsService, EmailService, GeoService, QueueService, SandboxService, StorageService,
};
use skilluv_backend::websocket::WsManager;
use skilluv_backend::{AppState, AppStateConfig, build_router};

fn main() {
    dotenvy::dotenv().ok();

    let config = AppConfig::from_env();
    // Refuse to boot in prod with insecure defaults (Phase 1.12).
    config.assert_production_secrets();

    // Init Sentry *before* the Tokio runtime so panic capture is wired immediately.
    // The returned guard must outlive the program — held by `_sentry_guard`.
    let _sentry_guard = observability::init_sentry(&config);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(observability::sentry_tracing_layer())
        .init();

    if config.sentry_dsn.is_some() {
        tracing::info!(
            environment = %config.environment,
            traces_sample_rate = config.sentry_traces_sample_rate,
            "Sentry/GlitchTip error tracking enabled"
        );
    } else {
        tracing::info!("Sentry/GlitchTip disabled (SENTRY_DSN not set)");
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime")
        .block_on(async_main(config));
}

async fn async_main(config: AppConfig) {
    skilluv_backend::routes::init_metrics();

    tracing::info!("Connecting to PostgreSQL...");
    let db = DatabaseConfig::connect(&config.database_url).await;
    skilluv_backend::routes::start_business_gauges(db.clone());
    skilluv_backend::services::credits::start_interest_timeout_refunder(db.clone());
    // Phase 4.4 — FX rate refresher (ECB reference every 6h)
    skilluv_backend::services::fx::start_fx_refresher(db.clone());

    tracing::info!("Connecting to Redis...");
    let redis = RedisConfig::connect(&config.redis_url).await;

    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run database migrations");

    tracing::info!("Seeding leaderboards from database...");
    skilluv_backend::services::LeaderboardService::seed_from_db(&mut redis.clone(), &db)
        .await
        .expect("Failed to seed leaderboards");

    let sandbox = Arc::new(SandboxService::new(&config.judge0_url));

    tracing::info!("Initializing storage service...");
    let storage = Arc::new(StorageService::new(&config).await);

    let email = Arc::new(EmailService::new(
        config.brevo_api_key.clone(),
        &config.email_from,
        &config.email_from_name,
    ));
    // Drip sequences (Phase 3.15) — hourly background task, idempotent via email_log.
    skilluv_backend::services::drip::start_drip_task(db.clone(), email.clone());

    // Connect to AI service (optional — backend works without it)
    let ai = if let Some(ref grpc_url) = config.grpc_ai_url {
        tracing::info!("Connecting to AI service at {grpc_url}...");
        match AiClient::connect(grpc_url).await {
            Some(client) => {
                tracing::info!("AI service connected");
                Some(Arc::new(client))
            }
            None => {
                tracing::warn!("AI service unavailable — running without AI features");
                None
            }
        }
    } else {
        tracing::info!("No GRPC_AI_URL configured — AI features disabled");
        None
    };

    // Initialize Redis queue service for async AI jobs
    let queue = Arc::new(QueueService::new(redis.clone()));
    queue.start_listener(&config.redis_url);
    tracing::info!("Redis queue service initialized");

    tracing::info!("Loading GeoNames data (countries + cities)...");
    let geo = Arc::new(
        GeoService::load(std::path::Path::new("data"))
            .expect("Failed to load GeoNames data from ./data"),
    );
    tracing::info!(
        countries = geo.countries().len(),
        cities = geo.total_cities(),
        "GeoNames data loaded"
    );

    let analytics = AnalyticsService::from_env();
    if analytics.is_enabled() {
        tracing::info!("PostHog analytics enabled");
    } else {
        tracing::info!("PostHog analytics disabled (POSTHOG_API_KEY not set)");
    }

    let ws = WsManager::new();

    let addr = config.addr();

    let webauthn = Arc::new(
        skilluv_backend::services::WebauthnService::new(&config.base_url)
            .expect("Failed to build WebAuthn service — check BASE_URL"),
    );

    let state = AppState {
        db,
        redis,
        config: AppStateConfig {
            jwt_secret: config.jwt_secret,
            base_url: config.base_url,
            sso_encryption_key: config.sso_encryption_key,
        },
        sandbox,
        storage,
        email,
        ai,
        queue,
        geo,
        analytics,
        ws,
        webauthn,
    };

    let app = build_router(state);
    tracing::info!("Skilluv backend listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app).await.expect("Server error");
}
