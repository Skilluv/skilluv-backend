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

    // The catalogue this platform cannot work without: the administrator, the
    // repositories work is drawn from, the onboarding challenges, the seasons.
    //
    // Here rather than in a deployment script because a script is something
    // somebody has to remember. A database restored from scratch used to come
    // up with its migrations and an empty catalogue, and the only way to find
    // out was to open the app and see empty pages.
    //
    // Every step is idempotent and the ledger skips the ones already applied,
    // so on an up-to-date database this is a single SELECT. Set
    // `SKILLUV_SEED_ON_BOOT=0` to turn it off — for a replica that should not
    // race the primary, or for a restore being inspected before it is trusted.
    if std::env::var("SKILLUV_SEED_ON_BOOT").as_deref() != Ok("0") {
        tracing::info!("Applying seed catalogue...");
        match skilluv_backend::services::seed::run(&db).await {
            Ok(report) => {
                tracing::info!(
                    applied = report.applied,
                    skipped = report.skipped,
                    "seed catalogue up to date"
                );
                if report.blocked_on_owner {
                    // Not fatal: a first deployment that has not been told who
                    // the administrator is should still come up. It is loud
                    // because a half-seeded database found later is worse.
                    tracing::warn!(
                        "part of the seed catalogue was skipped: this database has no                          administrator. Set SEED_ADMIN_PASSWORD (12+ characters) and restart,                          or run `skilluv-seed-all`."
                    );
                }
            }
            // Never fatal. A seed that cannot apply is a catalogue problem;
            // refusing to serve the requests that do work would turn it into
            // an outage.
            Err(e) => tracing::error!(error = %e, "seed catalogue failed to apply"),
        }
    }

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

    // Compares our books against what each provider says it holds. Daily,
    // and it never corrects anything — a discrepancy in real money is for a
    // person to resolve, not a background job.
    skilluv_backend::services::balance_check::start_balance_check(db.clone());

    // Asks providers about payments still open, and delivers anything paid
    // for that was never delivered. The piece that makes a closed browser
    // tab cost nothing.
    skilluv_backend::services::payment_poller::start_payment_poller(db.clone());

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
    spawn_artifact_stats_worker(state.clone());
    spawn_craft_score_worker(state.clone());
    spawn_contest_reminder_worker(state.clone());
    spawn_audio_analysis_worker(state.clone());
    spawn_design_upload_sweeper(state.clone());
    spawn_code_portfolio_worker(state.clone());
    spawn_portfolio_sync_worker(state.clone());
    spawn_release_sweep_worker(state.clone());
    spawn_credential_expiry_worker(state.clone());
    spawn_ats_erasure_worker(state.clone());
    spawn_payout_reconciliation_worker(state.clone());
    spawn_profile_readme_sync_worker(state.clone());
    spawn_security_embargo_worker(state.clone());
    spawn_security_dedup_worker(state.clone());
    spawn_security_proof_sweeper(state.clone());

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
/// Erases applicant records past their retention date.
///
/// Not behind a feature flag, and not optional. The retention date is a
/// promise made to people who never signed up to this platform, and a date
/// nobody acts on is a comment. Daily is enough: the promise is "not kept
/// beyond N days", not "deleted at midnight exactly".
fn spawn_ats_erasure_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            match skilluv_backend::services::ats::erase_expired(&state.db).await {
                Ok(erased) if erased > 0 => {
                    tracing::info!(erased, "applicant records past their retention erased");
                }
                Ok(_) => {}
                Err(e) => tracing::error!(
                    error = %e,
                    "applicant erasure failed - records are being kept past what was promised"
                ),
            }
        }
    });
}

/// Tells people their certifications are about to lapse, a month ahead.
///
/// Daily, because the query selects exactly one day of the notice window: a
/// shorter interval would send the same notice several times, and a longer
/// one would skip days entirely.
fn spawn_credential_expiry_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::credentials::notify_expiring(&state).await {
                Ok(sent) if sent > 0 => {
                    tracing::info!(sent, "credential expiry notices sent");
                }
                Ok(_) => {}
                Err(e) => tracing::error!(
                    error = %e,
                    "credential expiry sweep failed - somebody's certification \
                     will lapse without warning"
                ),
            }
        }
    });
}

/// Walks the disclosure clocks, once a day.
///
/// Daily rather than hourly because what it produces is an item on somebody's
/// list, and an item that appears at 03:14 instead of 04:00 is the same item.
///
/// It never publishes anything. An expired embargo becomes
/// `partially_disclosed` and waits for an administrator, because publishing a
/// vulnerability is irreversible and a cron job is the wrong thing to be
/// holding that decision — the argument `sweep_embargoes` makes in full.
/// Deletes proof uploads no report references, once a day.
///
/// Uploads happen before a report is submitted — that is the shape of the form
/// — so an abandoned draft leaves files behind. A bucket that only grows is one
/// that eventually holds somebody's proof of a vulnerability they never
/// reported, which is the worst thing in it to be keeping.
///
/// Thirty days, so that a report started on a Friday and finished a fortnight
/// later still finds its screenshots.
fn spawn_security_proof_sweeper(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;

            let listing = match state.storage.list_private("security-proofs/").await {
                Ok(listing) => listing,
                Err(e) => {
                    // Not an error worth waking anybody for: no object store in
                    // development means no uploads to sweep.
                    tracing::debug!(error = %e, "proof sweep found no object store");
                    continue;
                }
            };

            match skilluv_backend::services::security_proofs::sweep_orphans(
                &state.db,
                &state.storage,
                &listing,
            )
            .await
            {
                Ok(deleted) if deleted > 0 => {
                    tracing::info!(deleted, "orphaned vulnerability proofs deleted");
                }
                Ok(_) => {}
                Err(e) => tracing::error!(
                    error = %e,
                    "proof sweep failed - unreferenced evidence of unfixed                      vulnerabilities is still in the bucket"
                ),
            }
        }
    });
}

fn spawn_security_embargo_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::security_findings::sweep_embargoes(&state.db).await {
                Ok(sweep) => {
                    if !sweep.expired.is_empty() {
                        tracing::warn!(
                            count = sweep.expired.len(),
                            "embargoes ran out - these findings are waiting on a                              publication decision nobody has taken"
                        );
                    }
                    for (finding_id, days) in &sweep.reminded {
                        // The reporter is told, because the clock is a promise
                        // this platform made them and they are the one who
                        // finds out whether it was kept.
                        match skilluv_backend::services::security_findings::notifiable(
                            &state.db,
                            *finding_id,
                        )
                        .await
                        {
                            Ok(f) => {
                                let _ = skilluv_backend::services::notify::send(
                                    &state,
                                    skilluv_backend::services::notify::Recipient::User(
                                        f.reporter_user_id,
                                    ),
                                    "security.embargo_ending",
                                )
                                .arg("title", f.title)
                                .arg("days", days.to_string())
                                .payload(serde_json::json!({ "finding_id": finding_id }))
                                .execute()
                                .await;
                            }
                            Err(e) => tracing::warn!(
                                finding = %finding_id, error = %e,
                                "an embargo reminder had nobody to send to"
                            ),
                        }
                    }
                    if !sweep.reminded.is_empty() {
                        tracing::info!(count = sweep.reminded.len(), "embargo reminders sent");
                    }
                }
                Err(e) => tracing::error!(
                    error = %e,
                    "embargo sweep failed - a disclosure deadline may pass unnoticed"
                ),
            }
        }
    });
}

/// Looks for findings that resemble each other, every fifteen minutes.
///
/// Frequent, because the result is what a triager reads *before* deciding, and
/// a duplicate detected after somebody has spent an afternoon reproducing it
/// has cost exactly what the scan exists to save. Cheap: it only touches rows
/// nothing has scanned yet.
fn spawn_security_dedup_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::security_findings::sweep_similarity(&state.db, 50)
                .await
            {
                Ok(scanned) if scanned > 0 => {
                    tracing::info!(scanned, "findings scanned for look-alikes");
                }
                Ok(_) => {}
                Err(e) => tracing::error!(
                    error = %e,
                    "similarity sweep failed - triagers will be reading without                      duplicate candidates"
                ),
            }
        }
    });
}

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

/// Refresh download figures for published libraries, once a day.
///
/// The sweep itself only touches rows older than a week, so a daily tick
/// spreads the work rather than doing it all on one day — and a deployment
/// that was down on sync day is not a week behind.
///
/// Off unless asked for: it calls three third-party services, and a
/// development machine has no business doing that on every boot.
/// Refresh the accounts people have on other platforms.
///
/// Weekly: none of these figures move fast enough to be worth asking more
/// often, and every one of these APIs rate-limits anonymous callers.
fn spawn_code_portfolio_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_CODE_PORTFOLIO_SYNC_ENABLED").as_deref() != Ok("1") {
            tracing::info!("code_portfolio worker : disabled (env flag absent)");
            return;
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "code_portfolio worker : no HTTP client, giving up");
                return;
            }
        };

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            let secret = state.config.jwt_secret.clone();
            match skilluv_backend::services::code_portfolio::sync_stale(
                &state.db,
                &client,
                Some(&secret),
            )
            .await
            {
                Ok(0) => tracing::debug!("code_portfolio worker : nothing stale"),
                Ok(n) => {
                    tracing::info!(refreshed = n, "code_portfolio worker : profiles refreshed")
                }
                Err(e) => tracing::error!(
                    error = %e,
                    "code_portfolio worker : sweep failed, figures stay as they were"
                ),
            }
        }
    });
}

/// Measure the audio files that arrived, and give every master a preview.
///
/// Every two minutes rather than hourly: an uploader is waiting for the
/// waveform and the loudness figures their own review grid asks about, and an
/// hour of "pending" reads as broken.
///
/// Not gated behind an env flag, unlike the craft-score sweep. That flag
/// exists because recomputing every score is expensive whether or not anything
/// changed; this pass does nothing at all when nothing is pending, and where
/// ffmpeg is absent it marks the queue `skipped` once and then finds it empty.
fn spawn_audio_analysis_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::audio_files::analyse_pending(
                &state.db,
                &state.storage,
                25,
            )
            .await
            {
                Ok(0) => {}
                Ok(n) => tracing::info!(analysed = n, "audio worker : files measured"),
                Err(e) => tracing::error!(
                    error = %e,
                    "audio worker : pass failed, the files stay pending"
                ),
            }
        }
    });
}

/// Keep the stored craft scores fresh.
///
/// The score is computed live on the profile page, so this exists only for
/// the column the sorted lists read. Hourly, bounded per pass: a sweep that
/// tries to do the whole table at once is one that times out and never
/// reaches the end of the alphabet.
/// Give up on the large uploads nobody finished.
///
/// An abandoned multipart upload keeps the parts already sent, and the object
/// store bills for them whether or not anybody ever completes it. Nightly,
/// because the sessions live a week and nothing is urgent — but it is the only
/// thing standing between a slow month and a storage invoice nobody can
/// explain.
fn spawn_design_upload_sweeper(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_DESIGN_UPLOAD_SWEEP_ENABLED").as_deref() != Ok("1") {
            tracing::info!("design_upload_sweeper : disabled (env flag absent)");
            return;
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::design_uploads::sweep_expired(
                &state.db,
                &state.storage,
            )
            .await
            {
                Ok(0) => tracing::debug!("design_upload_sweeper : nothing abandoned"),
                Ok(n) => tracing::info!(swept = n, "design_upload_sweeper : uploads abandoned"),
                Err(e) => tracing::error!(
                    error = %e,
                    "design_upload_sweeper : sweep failed, the next pass retries"
                ),
            }
        }
    });
}

/// Warn people before a contest deadline passes, and tell them when it has.
///
/// Hourly, which is the coarsest cadence that still lands a forty-eight hour
/// warning inside its window. Enabled by env like the other sweeps: a second
/// process running this would double every reminder, and the flag is what
/// makes "which box sends the mail" an explicit answer.
fn spawn_contest_reminder_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_CONTEST_REMINDERS_ENABLED").as_deref() != Ok("1") {
            tracing::info!("contest_reminders worker : disabled (env flag absent)");
            return;
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::contest_reminders::sweep(&state.db).await {
                Ok(report) if report.total() == 0 => {
                    tracing::debug!("contest_reminders worker : no deadline in the window")
                }
                Ok(report) => tracing::info!(
                    deadlines = report.deadline_warnings,
                    juries = report.jury_warnings,
                    closures = report.closures_announced,
                    "contest_reminders worker : reminders sent"
                ),
                Err(e) => tracing::error!(
                    error = %e,
                    "contest_reminders worker : sweep failed, the next pass retries"
                ),
            }
        }
    });
}

fn spawn_craft_score_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_CRAFT_SCORE_ENABLED").as_deref() != Ok("1") {
            tracing::info!("craft_score worker : disabled (env flag absent)");
            return;
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            // One worker, one sweep per domain. A second worker per domain
            // would mean a second env flag somebody forgets to set, and the
            // symptom is a domain whose listings quietly never sort.
            for (domain, outcome) in [
                (
                    "code",
                    skilluv_backend::services::craft_score::sweep(&state.db, 500).await,
                ),
                (
                    "ai",
                    skilluv_backend::services::ai_profile::sweep(&state.db, 500).await,
                ),
                (
                    "audio",
                    skilluv_backend::services::audio_profile::sweep(&state.db, 500).await,
                ),
                (
                    "communication",
                    skilluv_backend::services::communication_profile::sweep(&state.db, 500).await,
                ),
                (
                    "education",
                    skilluv_backend::services::education_profile::sweep(&state.db, 500).await,
                ),
            ] {
                match outcome {
                    Ok(0) => tracing::debug!(domain, "craft_score worker : nothing stale"),
                    Ok(n) => {
                        tracing::info!(
                            domain,
                            recomputed = n,
                            "craft_score worker : scores refreshed"
                        )
                    }
                    Err(e) => tracing::error!(
                        domain, error = %e,
                        "craft_score worker : sweep failed, scores stay as they were"
                    ),
                }
            }
        }
    });
}

fn spawn_artifact_stats_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_ARTIFACT_STATS_ENABLED").as_deref() != Ok("1") {
            tracing::info!("artifact_stats worker : disabled (env flag absent)");
            return;
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "artifact_stats worker : no HTTP client, giving up");
                return;
            }
        };

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::artifact_registry::sync_stale(&state.db, &client).await
            {
                Ok(0) => tracing::debug!("artifact_stats worker : nothing stale"),
                Ok(n) => tracing::info!(refreshed = n, "artifact_stats worker : figures refreshed"),
                Err(e) => tracing::error!(
                    error = %e,
                    "artifact_stats worker : sweep failed, figures stay as they were"
                ),
            }
        }
    });
}

/// Refreshes the external accounts linked outside the code forges.
///
/// Weekly, because these figures move slowly and every one of these services
/// is somebody else's, run for free. `spawn_code_portfolio_worker` covers the
/// forges, where the figures and the verification are a different problem.
fn spawn_portfolio_sync_worker(state: skilluv_backend::AppState) {
    tokio::spawn(async move {
        if std::env::var("SKILLUV_PORTFOLIO_SYNC_ENABLED").as_deref() != Ok("1") {
            tracing::info!("portfolio_sync worker : disabled (env flag absent)");
            return;
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "portfolio_sync worker : no HTTP client, giving up");
                return;
            }
        };

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match skilluv_backend::services::portfolio_sync::sync_stale(&state.db, &client).await {
                Ok(0) => tracing::debug!("portfolio_sync worker : nothing stale"),
                Ok(n) => {
                    tracing::info!(refreshed = n, "portfolio_sync worker : accounts refreshed")
                }
                Err(e) => tracing::error!(
                    error = %e,
                    "portfolio_sync worker : sweep failed, figures stay as they were"
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
