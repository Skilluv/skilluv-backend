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

    let fmt_layer = tracing_subscriber::fmt::layer().json().with_target(true);
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
    // Installed before any background task can emit: the proof engine, the
    // mention recorder and the reconciliation sweep hold only a `PgPool`,
    // and without this their notifications reach every channel except the
    // one that matters when nobody is looking at the app.
    skilluv_backend::services::notify::install_ambient(
        email.clone(),
        config.frontend_url.clone(),
        config.jwt_secret.clone(),
    );

    // Drains what failed on its channel, every minute — the shortest
    // backoff, so a first retry waits for the backoff rather than the tick.
    skilluv_backend::services::outbox::start_outbox_worker(db.clone(), email.clone());

    // Drip sequences (Phase 3.15) — hourly background task, idempotent via email_log.
    skilluv_backend::services::drip::start_drip_task(
        db.clone(),
        email.clone(),
        config.frontend_url.clone(),
        config.jwt_secret.clone(),
    );

    // The streak reminder the settings screen has promised since phase 1.7.
    skilluv_backend::services::streak_reminder::start_streak_reminder_task(db.clone());

    // P19.3 — Proof engine sweep (weekly by default). Filet de sécurité qui
    // rattrape les évolutions de seuils/rules et les hooks inline en échec.
    // Activation via SKILLUV_PROOF_SWEEP_ENABLED=1.
    skilluv_backend::services::proof_hooks::start_proof_sweep_task(db.clone());

    // SKI-38 — weekly sweep stamping achieved goals and archiving settled
    // ones. Env-gated (SKILLUV_GOAL_ARCHIVAL_ENABLED=1), off by default.
    skilluv_backend::services::goals::start_goal_archival_task(db.clone());

    // P26 v2 SKI-88 — fallback poller that catches missed CI webhooks.
    // Silently no-ops when SKILLUV_BOT_GITHUB_TOKEN is unset.
    skilluv_backend::services::ci_sync::start_ci_poll_task(db.clone());

    // P26 v2 SKI-111 — external repo refresh poller.
    // Detects upstream issue edits, closures, and PR merge/close on
    // repos where we can't install a webhook. No-op if bot token unset.
    skilluv_backend::services::external_refresh::start_external_refresh_task(db.clone());

    // P26 v2 SKI-120 — maintainer digest weekly task. Every hour scans
    // for confirmed subscriptions due (last_digest_at > 7d) and emails.
    skilluv_backend::services::maintainer_digest::start_maintainer_digest_task(
        db.clone(),
        email.clone(),
        config.base_url.clone(),
    );

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

    let geo_dir = GeoService::data_dir_from_env();
    tracing::info!(path = %geo_dir.display(), "Loading GeoNames data (countries + cities)...");
    let geo = Arc::new(GeoService::load_or_empty(&geo_dir));
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
            frontend_url: config.frontend_url,
            sso_encryption_key: config.sso_encryption_key,
            pdf_renderer_url: config.pdf_renderer_url,
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

    // Startup tasks — services background lances a interval fixe. Chacun est
    // gate par un env-var pour eviter les surprises en dev. En staging/prod,
    // activer avec :
    //   SKILLUV_HELLO_WALL_MIRROR_ENABLED=1
    //   SKILLUV_PROFILE_README_SYNC_ENABLED=1
    // Sans le flag, la tache spawn quand meme mais log un no-op.
    spawn_hello_wall_mirror_worker(state.clone());
    spawn_release_sweep_worker(state.clone());
    spawn_payout_reconciliation_worker(state.clone());
    spawn_profile_readme_sync_worker(state.clone());

    let app = build_router(state);
    tracing::info!("Skilluv backend listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app).await.expect("Server error");
}

/// Worker cron du mirror Hello Wall. Toutes les 5 min, pousse les entrees
/// pending sur `skilluv-community/hello-wall`.
///
/// Requiert `SKILLUV_HELLO_WALL_MIRROR_ENABLED=1` + `SKILLUV_BOT_GITHUB_TOKEN`.
/// Sans le token, la tache log un warning au demarrage puis dort — permet
/// d'ajouter le token plus tard sans redemarrer.
/// Releases money whose hold has expired.
///
/// Not behind a feature flag, unlike the other workers. Every other job here
/// enriches something; this one is the difference between a mentor being paid
/// and a mentor waiting forever. A deployment that forgets to enable it would
/// look healthy and quietly stop paying people.
///
/// Runs every ten minutes. The precision that matters is hours — nobody
/// notices their money arriving at 14:07 instead of 14:00 — and a short
/// interval keeps the backlog small enough that one failing hold cannot bury
/// the rest.
fn spawn_release_sweep_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::release::sweep(&state.db).await {
                Ok(report) => {
                    if !report.failed.is_empty() {
                        tracing::error!(
                            failed = report.failed.len(),
                            examined = report.examined,
                            details = ?report.failed,
                            "release sweep could not release every due hold -                              people are owed money they cannot reach"
                        );
                    }
                }
                Err(e) => tracing::error!(
                    error = %e,
                    "release sweep failed entirely - no funds were released this cycle"
                ),
            }

            // Anything still late an hour after its window closed means the
            // sweep is not doing its job. Surfaced separately from a failed
            // cycle: a sweep can succeed every time and still leave a hold
            // stuck behind a dispute nobody resolved.
            if let Ok(late) = skilluv_backend::services::release::overdue(&state.db).await
                && !late.is_empty()
            {
                metrics::gauge!("skilluv_release_overdue_holds").set(late.len() as f64);
                tracing::warn!(
                    count = late.len(),
                    "holds are past their release window and still unreleased"
                );
            }
        }
    });
}

/// Chases payouts whose provider never called back.
///
/// Not behind a feature flag, for the same reason as the release sweep: a
/// deployment that forgets to enable it looks healthy while holding payouts
/// that are `pending` forever — the recipient's balance debited, the money
/// somewhere nobody can name.
///
/// Every fifteen minutes. The sweep only looks at payouts older than its own
/// quiet period, so running often costs a cheap indexed query and keeps the
/// backlog from arriving all at once.
fn spawn_payout_reconciliation_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            let registry = skilluv_backend::services::payout_adapters::registry_from_env();
            match skilluv_backend::services::reconciliation::sweep(&state.db, &registry).await {
                Ok(report) => {
                    if report.checked > 0 || report.replayed_events > 0 {
                        tracing::info!(
                            checked = report.checked,
                            settled = report.settled,
                            failed = report.failed,
                            escalated = report.escalated,
                            replayed = report.replayed_events,
                            "payout reconciliation cycle"
                        );
                    }
                }
                Err(e) => tracing::error!(
                    error = %e,
                    "payout reconciliation failed entirely - unconfirmed payouts stayed unconfirmed"
                ),
            }
        }
    });
}

fn spawn_hello_wall_mirror_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_HELLO_WALL_MIRROR_ENABLED").as_deref() != Ok("1") {
            tracing::info!("hello_wall_mirror worker : disabled (env flag absent)");
            return;
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        // Skip la premiere tick immediate (comportement par defaut = fire now).
        interval.tick().await;
        loop {
            interval.tick().await;
            let token = match std::env::var("SKILLUV_BOT_GITHUB_TOKEN") {
                Ok(t) => t,
                Err(_) => {
                    tracing::warn!(
                        "hello_wall_mirror worker : SKILLUV_BOT_GITHUB_TOKEN manquant, skip cycle"
                    );
                    continue;
                }
            };
            match skilluv_backend::services::hello_wall_mirror::mirror_pending_entries(
                &state.db, &token,
            )
            .await
            {
                Ok(report) => {
                    if !report.mirrored.is_empty() || !report.failed.is_empty() {
                        tracing::info!(
                            mirrored = report.mirrored.len(),
                            failed = report.failed.len(),
                            "hello_wall_mirror worker cycle done"
                        );
                    }
                }
                Err(e) => tracing::error!(error = %e, "hello_wall_mirror worker cycle failed"),
            }
        }
    });
}

/// Worker cron du sync Profile README. 1x/heure, iterate les users
/// `profile_readme_source='github_sync'` et refetch leur README GitHub.
///
/// Requiert `SKILLUV_PROFILE_README_SYNC_ENABLED=1`. Token bot optionnel.
fn spawn_profile_readme_sync_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_PROFILE_README_SYNC_ENABLED").as_deref() != Ok("1") {
            tracing::info!("profile_readme_sync worker : disabled (env flag absent)");
            return;
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            let token = std::env::var("SKILLUV_BOT_GITHUB_TOKEN").ok();
            match skilluv_backend::services::profile_readme_sync::sync_pending_readmes(
                &state.db,
                token.as_deref(),
            )
            .await
            {
                Ok(report) => {
                    if !report.synced.is_empty() || !report.failed.is_empty() {
                        tracing::info!(
                            synced = report.synced.len(),
                            failed = report.failed.len(),
                            skipped_no_readme = report.skipped_no_readme.len(),
                            "profile_readme_sync worker cycle done"
                        );
                    }
                }
                Err(e) => tracing::error!(error = %e, "profile_readme_sync worker cycle failed"),
            }
        }
    });
}
