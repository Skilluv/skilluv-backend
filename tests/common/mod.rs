// Ce module est partagé entre plusieurs binaires de test ; Rust émet un
// dead_code par binaire pour chaque helper qui n'est pas utilisé dans CE
// binaire — même si un autre s'en sert. On les tolère globalement.
#![allow(dead_code)]

pub mod mock_oidc;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::{Arc, Once};
use uuid::Uuid;

use skilluv_backend::{AppState, AppStateConfig, build_router};

/// Init a tracing subscriber once per test-binary process, so backend
/// `tracing::error!` calls surface in `cargo test -- --nocapture`.
/// Without this, a 500 in a handler is invisible during test debugging.
///
/// Verbosity controlled by `RUST_LOG` env-var (default: warn).
fn init_test_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        use tracing_subscriber::{EnvFilter, fmt};
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
        let _ = fmt().with_env_filter(filter).with_test_writer().try_init();
    });
}

/// Base connection string for the test database server, without a database
/// name. Override with `TEST_DATABASE_BASE_URL`.
///
/// Configurable rather than hardcoded because the default port is easy to
/// shadow: anything else bound to `127.0.0.1:5433` — most plausibly an SSH
/// tunnel to a remote database — silently wins over the Docker container,
/// and this harness issues `CREATE DATABASE` / `DROP DATABASE` on whatever
/// answers. Pointing the suite somewhere explicit must not require editing
/// source.
const DEFAULT_TEST_DB_BASE: &str = "postgres://skilluv:skilluv_secret@localhost:5433";

/// Connection string for `db_name` on the test server.
pub fn test_db_url(db_name: &str) -> String {
    let base = std::env::var("TEST_DATABASE_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_TEST_DB_BASE.to_string());
    format!("{}/{db_name}", base.trim_end_matches('/'))
}

/// Migrations, applied once and then copied.
///
/// ## Why
///
/// Every test used to create an empty database and replay every migration
/// into it. That is roughly thirteen seconds each, on a schema that is over
/// two hundred and forty migrations long — a suite of eighteen tests spent
/// four minutes doing the same work eighteen times, and the whole integration
/// suite spent most of an hour on it.
///
/// PostgreSQL can copy a database at the file level. So the migrations run
/// once into a template, and each test gets a copy of that template, which
/// takes about as long as creating an empty database did.
///
/// ## Why the template name carries a fingerprint
///
/// It is built from every migration's version and checksum, which is exactly
/// what would have been applied. Change any migration and the name changes,
/// so the next run builds a fresh template instead of copying a stale schema
/// — the failure mode this optimisation would otherwise introduce, and the
/// one that would waste an afternoon.
///
/// ## Why an advisory lock
///
/// Several test binaries start at once, and all of them would find the
/// template missing and try to build it. The lock makes one of them build it
/// while the others wait; they then find it present. It is taken on the
/// maintenance database, not on the template, because nothing may hold a
/// connection to a database being used as a template.
mod template {
    use super::*;

    /// Namespace for the advisory lock. Arbitrary, but fixed: two runs have
    /// to pick the same number to exclude each other.
    const LOCK_NAMESPACE: i32 = 0x5C11;

    /// A fingerprint of the migration set, short enough to read in a database
    /// name and specific enough that two different sets never collide in
    /// practice.
    fn fingerprint() -> String {
        // FNV-1a over each migration's version and checksum. Hand-rolled to
        // keep this file dependency-free, and sufficient: this distinguishes
        // schema versions, it does not defend against an adversary.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |byte: u8| {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for migration in sqlx::migrate!("./migrations").iter() {
            for byte in migration.version.to_le_bytes() {
                eat(byte);
            }
            for byte in migration.checksum.iter() {
                eat(*byte);
            }
        }
        format!("{hash:016x}")
    }

    pub fn name() -> String {
        format!("skilluv_tmpl_{}", fingerprint())
    }

    /// Build the template if it is not there, and drop any left by an older
    /// migration set.
    ///
    /// Returns the template name, ready to be copied from.
    pub async fn ensure(admin: &PgPool) -> String {
        let tmpl = name();

        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&tmpl)
                .fetch_one(admin)
                .await
                .expect("failed to look for the template database");
        if exists {
            return tmpl;
        }

        // One builder, the rest wait. `pg_advisory_lock` is held for the
        // session, and this pool is closed by the caller right after.
        sqlx::query("SELECT pg_advisory_lock($1, $2)")
            .bind(LOCK_NAMESPACE)
            .bind(lock_key(&tmpl))
            .execute(admin)
            .await
            .expect("failed to take the template lock");

        // Somebody may have built it while we waited.
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&tmpl)
                .fetch_one(admin)
                .await
                .expect("failed to look for the template database");

        if !exists {
            build(admin, &tmpl).await;
            sweep_stale_templates(admin, &tmpl).await;
        }

        sqlx::query("SELECT pg_advisory_unlock($1, $2)")
            .bind(LOCK_NAMESPACE)
            .bind(lock_key(&tmpl))
            .execute(admin)
            .await
            .expect("failed to release the template lock");

        tmpl
    }

    async fn build(admin: &PgPool, tmpl: &str) {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{tmpl}\"")))
            .execute(admin)
            .await
            .expect("failed to create the template database");

        // A pool of its own, closed before anybody copies from it: PostgreSQL
        // refuses to use a database as a template while a session is attached.
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&test_db_url(tmpl))
            .await
            .expect("failed to connect to the template database");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to migrate the template database");
        pool.close().await;

        // Nothing should ever connect to it again. Refusing connections is
        // what keeps a stray psql session from blocking every test at once.
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER DATABASE \"{tmpl}\" WITH ALLOW_CONNECTIONS false IS_TEMPLATE true"
        )))
        .execute(admin)
        .await;
    }

    /// Templates from an older migration set are dead weight on somebody's
    /// development machine, and this runs on the machine that made them.
    async fn sweep_stale_templates(admin: &PgPool, keep: &str) {
        let stale: Vec<String> = sqlx::query_scalar(
            "SELECT datname FROM pg_database
              WHERE datname LIKE 'skilluv\\_tmpl\\_%' AND datname <> $1",
        )
        .bind(keep)
        .fetch_all(admin)
        .await
        .unwrap_or_default();

        for old in stale {
            // `IS_TEMPLATE` has to be cleared before a template can be
            // dropped, and both statements are allowed to fail: another run
            // may be copying from it at this instant, and a template left
            // behind is a wasted gigabyte rather than a broken test.
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "ALTER DATABASE \"{old}\" WITH IS_TEMPLATE false"
            )))
            .execute(admin)
            .await;
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS \"{old}\" WITH (FORCE)"
            )))
            .execute(admin)
            .await;
        }
    }

    /// Second half of the advisory lock key, derived from the template name so
    /// two different migration sets do not block each other.
    fn lock_key(tmpl: &str) -> i32 {
        let mut hash: u32 = 2_166_136_261;
        for byte in tmpl.as_bytes() {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(16_777_619);
        }
        (hash & 0x7fff_ffff) as i32
    }
}

/// A test application instance with isolated database.
pub struct TestApp {
    pub addr: String,
    pub db: PgPool,
    pub client: Client,
    db_name: String,
}

impl TestApp {
    /// Connection string of this instance's isolated database.
    ///
    /// Needed by suites that run one of the `src/bin` binaries as a real
    /// subprocess: the binary reads `DATABASE_URL` and has no other way to
    /// find the schema this harness just migrated.
    pub fn database_url(&self) -> String {
        test_db_url(&self.db_name)
    }

    /// Spawn a test server with an isolated database.
    /// Emails are delivered to the local Mailpit container (SMTP :1025, UI :8025).
    pub async fn spawn() -> Self {
        init_test_tracing();
        // Wire the EmailService onto Mailpit for tests. Read by `email::build_smtp_from_env`.
        // Safe to set for every test — env vars are process-global, but the values don't vary.
        // SAFETY: we're only reading and setting env at test-startup, before any concurrent
        // reader kicks in, and the value is the same across every parallel test.
        unsafe {
            std::env::set_var("SMTP_HOST", "localhost");
            std::env::set_var("SMTP_PORT", "1025");
            std::env::set_var("SMTP_TLS", "none");
            // Bypass RateLimiter dans les tests d'intégration : plusieurs
            // binaires parallèles partagent Redis et se rate-limitent mutuellement.
            std::env::set_var("SKILLUV_DISABLE_RATELIMIT", "1");
        }

        // Unique DB name for test isolation
        let db_name = format!(
            "skilluv_test_{}",
            Uuid::new_v4().to_string().replace('-', "")
        );

        // Connect to default DB to create test DB
        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&test_db_url("skilluv"))
            .await
            .expect("Failed to connect to admin DB");

        // The migrations ran once, into a template. This is a file-level copy
        // of it, which is what makes a test cost a second rather than the
        // thirteen it takes to replay two hundred and forty migrations.
        let tmpl = template::ensure(&admin_pool).await;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE \"{db_name}\" TEMPLATE \"{tmpl}\""
        )))
        .execute(&admin_pool)
        .await
        .expect("Failed to create test DB from the migration template");

        admin_pool.close().await;

        // Connect to test DB
        let db_url = test_db_url(&db_name);
        let db = PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test DB");

        // Redis : chaque binaire de test s'attribue une DB distincte via PID % 16
        // (Redis fournit 16 DBs par défaut). Cela évite les races inter-binaires
        // quand `cargo test --jobs 2+` fait tourner plusieurs suites en parallèle
        // qui écrasent mutuellement les clés partagées (rate-limit, leaderboards,
        // notifications:unread:*, etc.). Les tests d'un même binaire partagent
        // néanmoins la DB — c'est OK, ils utilisent des user_ids uniques.
        let redis_db = (std::process::id() as usize) % 16;
        let redis_url = format!("redis://localhost:6379/{redis_db}");
        let redis_client = redis::Client::open(redis_url.clone()).expect("Invalid Redis URL");
        let redis = redis::aio::ConnectionManager::new(redis_client.clone())
            .await
            .expect("Failed to connect to Redis");

        // Seed leaderboards
        skilluv_backend::services::LeaderboardService::seed_from_db(&mut redis.clone(), &db)
            .await
            .ok();

        let sandbox = Arc::new(skilluv_backend::services::SandboxService::new(
            "http://localhost:2358",
        ));

        // Storage — create a minimal config for tests
        let storage_config = skilluv_backend::config::AppConfig {
            host: "0.0.0.0".to_string(),
            port: 0,
            jwt_secret: "test-secret-key-for-testing".to_string(),
            database_url: db_url,
            redis_url: redis_url.clone(),
            base_url: "http://localhost:3001".to_string(),
            frontend_url: "http://localhost:5173".to_string(),
            judge0_url: "http://localhost:2358".to_string(),
            minio_endpoint: "http://localhost:9004".to_string(),
            minio_access_key: "skilluv".to_string(),
            minio_secret_key: "skilluv_secret".to_string(),
            minio_bucket: format!("test-{}", &db_name[..20]),
            minio_bucket_private: format!("testpriv-{}", &db_name[..16]),
            avatar_cdn_base_url: None,
            grpc_ai_url: None,
            brevo_api_key: None,
            email_from: "test@skilluv.com".to_string(),
            email_from_name: "Skilluv Test".to_string(),
            environment: "test".to_string(),
            sso_encryption_key: Some([42u8; 32]),
            sentry_dsn: None,
            sentry_traces_sample_rate: 0.0,
            release: None,
            pdf_renderer_url: None,
        };

        let storage =
            Arc::new(skilluv_backend::services::StorageService::new(&storage_config).await);

        let ws = skilluv_backend::websocket::WsManager::new();

        let queue = Arc::new(skilluv_backend::services::QueueService::new(redis.clone()));

        // SKI-294 — same tolerance as the server: a suite that does not touch
        // country autocompletion should not need the 31 MB of dumps present.
        let geo = Arc::new(skilluv_backend::services::GeoService::load_or_empty(
            &skilluv_backend::services::GeoService::data_dir_from_env(),
        ));

        // Bind to random port FIRST so the app's `base_url` can be aligned with
        // the actual test server address. Otherwise features that mint absolute
        // URLs (SSO redirect_uri, webhook callbacks, etc.) point at a hardcoded
        // port that the tests don't listen on.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind");
        let addr = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());

        // Same as `main`: the background senders hold only a pool, and
        // without this a test asserting that a promotion emails somebody
        // would pass for the wrong reason.
        skilluv_backend::services::notify::install_ambient(
            Arc::new(skilluv_backend::services::EmailService::new(
                None,
                "test@skill-uv.com",
                "Skilluv Test",
            )),
            "http://localhost:5173".to_string(),
            "test-secret-key-for-testing".to_string(),
        );

        let state = AppState {
            db: db.clone(),
            redis,
            config: AppStateConfig {
                jwt_secret: "test-secret-key-for-testing".to_string(),
                base_url: addr.clone(),
                frontend_url: "http://localhost:5173".to_string(),
                sso_encryption_key: Some([42u8; 32]),
                pdf_renderer_url: None,
            },
            sandbox,
            storage,
            email: Arc::new(skilluv_backend::services::EmailService::new(
                None, // No Brevo in tests — dev mode (logging only)
                "test@skilluv.com",
                "Skilluv Test",
            )),
            ai: None,
            queue,
            geo,
            analytics: skilluv_backend::services::AnalyticsService::from_env(),
            ws,
            webauthn: Arc::new(
                skilluv_backend::services::WebauthnService::new("http://localhost:3001")
                    .expect("Failed to build WebauthnService for tests"),
            ),
        };

        let app = build_router(state);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Clear rate limit keys for tests (best-effort)
        if let Ok(mut redis_clear) = redis_client.get_multiplexed_async_connection().await {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg("ratelimit:*")
                .query_async(&mut redis_clear)
                .await
                .unwrap_or_default();
            for key in keys {
                let _: Result<(), redis::RedisError> = redis::cmd("DEL")
                    .arg(&key)
                    .query_async(&mut redis_clear)
                    .await;
            }
        }

        // Chaque TestApp fabrique un X-Forwarded-For unique — le RateLimiter
        // clé par IP, sinon toutes les requêtes tests partageraient le même
        // bucket "unknown" et se rate-limiteraient mutuellement en parallèle.
        let mut headers = reqwest::header::HeaderMap::new();
        let uniq_ip = format!(
            "10.{}.{}.{}",
            (db_name.as_bytes()[10] & 0x7f),
            db_name.as_bytes()[11],
            db_name.as_bytes()[12]
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("x-forwarded-for"),
            reqwest::header::HeaderValue::from_str(&uniq_ip).unwrap(),
        );
        // Origin header : le middleware `ensure_admin_origin` (BE-C) exige
        // Origin (ou Referer) matchant l'admin panel dev/prod. Sans ce header,
        // toutes les routes /admin/* renvoient 403 AdminOriginRequired.
        // On envoie le dev admin origin par defaut ; les endpoints publics
        // ignorent l'Origin, donc pas d'effet de bord.
        headers.insert(
            reqwest::header::ORIGIN,
            reqwest::header::HeaderValue::from_static("http://localhost:5174"),
        );
        let client = Client::builder()
            .cookie_store(true)
            .default_headers(headers)
            .build()
            .expect("Failed to build HTTP client");

        TestApp {
            addr,
            db: db.clone(),
            client,
            db_name,
        }
    }

    /// Password used across tests — satisfies the Vague 1 policy (10+ chars, upper/lower/digit/symbol).
    pub const TEST_PASSWORD: &'static str = "TestPass123!";

    /// Register a user with the standard test payload.
    pub async fn register_user(&self, username: &str) -> Value {
        let resp = self
            .client
            .post(format!("{}/api/auth/register", self.addr))
            .json(&json!({
                "email": format!("{username}@test.com"),
                "username": username,
                "password": Self::TEST_PASSWORD,
                "first_name": "Test",
                "last_name": "User",
                "skill_domain": "code",
                "terms_accepted": true,
            }))
            .send()
            .await
            .expect("Register request failed");

        // The body, not just the status: a failed registration is the first
        // thing a hundred suites hit, and `201 != 500` on its own has cost
        // whole afternoons.
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .expect("Failed to parse register response");
        assert_eq!(
            status,
            StatusCode::CREATED,
            "register {username} said: {body}"
        );

        // Short-circuit the email-verification hop for tests — real users have
        // to click the link in the verification email before AuthUserComplete
        // (write endpoints) or /enterprise/* let them through.
        sqlx::query("UPDATE users SET email_verified = TRUE WHERE username = $1")
            .bind(username)
            .execute(&self.db)
            .await
            .expect("force-verify email for test user");

        body
    }

    /// Login and return the response (cookies are stored in the client jar).
    ///
    /// A second factor is stepped around rather than failed on. An enterprise
    /// fixture turns TOTP on so the `/enterprise/*` gate lets it through, and
    /// every later `login` for that user then answered 403 —
    /// `AUTH_TOTP_REQUIRED`, correctly, because the helper has no authenticator
    /// and cannot produce a code. That is not what any of these tests are
    /// about, and the failure read as an authorisation bug twenty tests wide.
    ///
    /// The flag is toggled off for the length of the request and restored
    /// straight after, so the gate still sees an account with a second factor.
    /// Nothing here can hide a real regression: the endpoint is called for
    /// real, and a login that fails for any other reason still fails.
    pub async fn login(&self, identifier: &str) -> Value {
        let resp = self.try_login(identifier).await;
        if resp.status() != StatusCode::FORBIDDEN {
            assert_eq!(resp.status(), StatusCode::OK);
            return resp.json().await.expect("Failed to parse login response");
        }

        // A 403 is only stepped around when the account actually has TOTP on.
        // Checking first matters: a 403 for any other reason — a banned
        // account, say — would otherwise be retried and then left with
        // `totp_enabled = TRUE` on a user who never had a second factor,
        // which is a lie the next assertion in that test would inherit.
        let has_totp: bool = sqlx::query_scalar(
            "SELECT COALESCE(BOOL_OR(totp_enabled), FALSE) FROM users
              WHERE username = $1 OR email = $1",
        )
        .bind(identifier)
        .fetch_one(&self.db)
        .await
        .expect("read totp state for test login");

        if !has_totp {
            let body = resp.text().await.unwrap_or_default();
            panic!(
                "login as {identifier} was refused with 403, and not for a second factor: {body}"
            );
        }

        sqlx::query("UPDATE users SET totp_enabled = FALSE WHERE username = $1 OR email = $1")
            .bind(identifier)
            .execute(&self.db)
            .await
            .expect("clear totp for test login");
        let resp = self.try_login(identifier).await;
        sqlx::query("UPDATE users SET totp_enabled = TRUE WHERE username = $1 OR email = $1")
            .bind(identifier)
            .execute(&self.db)
            .await
            .expect("restore totp after test login");
        assert_eq!(resp.status(), StatusCode::OK);
        resp.json().await.expect("Failed to parse login response")
    }

    async fn try_login(&self, identifier: &str) -> reqwest::Response {
        self.client
            .post(format!("{}/api/auth/login", self.addr))
            .json(&json!({
                "identifier": identifier,
                "password": Self::TEST_PASSWORD,
            }))
            .send()
            .await
            .expect("Login request failed")
    }

    /// Register a user and set them as admin in the DB.
    pub async fn register_admin(&self, username: &str) -> Value {
        let result = self.register_user(username).await;
        let user_id = result["data"]["user"]["id"].as_str().expect("No user id");

        sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1::UUID")
            .bind(user_id)
            .execute(&self.db)
            .await
            .expect("Failed to set admin role");

        // Middleware ensure_admin_2fa exige TOTP OU passkey. Setter totp_enabled
        // casserait le login (TotpRequired), donc on insere plutot une passkey
        // fictive (webauthn_credentials) — satisfait le middleware sans affecter
        // le flow login. credential_id doit etre unique, on derive des bytes de
        // l'user_id UUID (parse -> as_bytes).
        let user_uuid = Uuid::parse_str(user_id).expect("user_id is valid UUID");
        sqlx::query(
            "INSERT INTO webauthn_credentials (user_id, credential_id, credential, label)
             VALUES ($1, $2, '{}'::jsonb, 'test_setup')
             ON CONFLICT DO NOTHING",
        )
        .bind(user_uuid)
        .bind(user_uuid.as_bytes().as_slice())
        .execute(&self.db)
        .await
        .expect("Failed to insert test passkey");

        // P21.1 — require_admin lit désormais depuis user_capabilities.
        // On grant explicitement la capability admin pour rester compatible.
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1::UUID, 'admin', 'test_setup')
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .expect("Failed to grant admin capability");

        // Re-login to get token with admin role
        self.login(username).await
    }

    /// Register an enterprise account.
    pub async fn register_enterprise(&self, company: &str) -> Value {
        let username = company.to_lowercase().replace(' ', "");
        let resp = self
            .client
            .post(format!("{}/api/enterprise/register", self.addr))
            .json(&json!({
                "email": format!("{username}@enterprise.com"),
                "username": username,
                "password": Self::TEST_PASSWORD,
                "first_name": "Enterprise",
                "last_name": "Owner",
                "company_name": company,
                "company_size": "11-50",
                "terms_accepted": true,
            }))
            .send()
            .await
            .expect("Enterprise register failed");

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body: Value = resp
            .json()
            .await
            .expect("Failed to parse enterprise response");

        // In real usage the owner clicks the link in the verification email
        // before the mandatory-email-verified gate lets them into /enterprise/*.
        // For tests we short-circuit that hop by flipping the DB directly.
        sqlx::query("UPDATE users SET email_verified = TRUE WHERE username = $1")
            .bind(&username)
            .execute(&self.db)
            .await
            .expect("force-verify email for enterprise test user");

        body
    }

    /// Simulate a completed TOTP setup for a user. Enterprise/recruiter routes
    /// are gated behind mandatory-TOTP ; call this AFTER `login` so the login
    /// path (which requires a TOTP code when `totp_enabled=true`) is not
    /// blocked, and BEFORE hitting any `/enterprise/*` endpoint.
    pub async fn enable_totp_for(&self, username: &str) {
        sqlx::query("UPDATE users SET totp_enabled = TRUE WHERE username = $1")
            .bind(username)
            .execute(&self.db)
            .await
            .expect("Failed to force-enable TOTP for test user");
    }

    /// Re-login as a user who has `totp_enabled=true` in the DB. Temporarily
    /// toggles the flag off so the login POST succeeds without needing a real
    /// TOTP code, then flips it back on so subsequent `/enterprise/*` calls
    /// keep passing the gate.
    pub async fn relogin_with_totp(&self, username: &str) -> Value {
        sqlx::query("UPDATE users SET totp_enabled = FALSE WHERE username = $1")
            .bind(username)
            .execute(&self.db)
            .await
            .unwrap();
        let body = self.login(username).await;
        self.enable_totp_for(username).await;
        body
    }

    /// GET helper.
    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{}", self.addr, path))
            .send()
            .await
            .expect("GET request failed")
    }

    /// GET helper carrying one extra header.
    ///
    /// The metered public API authenticates on `x-api-key` rather than on the
    /// session cookie, so its tests need a way to present one.
    pub async fn get_with_header(
        &self,
        path: &str,
        name: &'static str,
        value: &str,
    ) -> reqwest::Response {
        self.client
            .get(format!("{}{}", self.addr, path))
            .header(name, value)
            .send()
            .await
            .expect("GET request failed")
    }

    /// POST helper with JSON body.
    pub async fn post(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .post(format!("{}{}", self.addr, path))
            .json(body)
            .send()
            .await
            .expect("POST request failed")
    }

    /// PUT helper with JSON body.
    pub async fn put(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .put(format!("{}{}", self.addr, path))
            .json(body)
            .send()
            .await
            .expect("PUT request failed")
    }

    /// PATCH helper with JSON body.
    pub async fn patch(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .patch(format!("{}{}", self.addr, path))
            .json(body)
            .send()
            .await
            .expect("PATCH request failed")
    }

    /// DELETE helper.
    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}{}", self.addr, path))
            .send()
            .await
            .expect("DELETE request failed")
    }

    /// DELETE helper with a JSON body.
    ///
    /// A body on a DELETE is unusual and deliberate where it is used: taking
    /// an opportunity off the board has to say why, and a reason in a query
    /// string is a reason that ends up in an access log.
    #[allow(dead_code)]
    pub async fn delete_with_body(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .delete(format!("{}{}", self.addr, path))
            .json(body)
            .send()
            .await
            .expect("DELETE request failed")
    }
}

// ─── Mailpit HTTP helpers ─────────────────────────────────────────

pub struct Mailpit {
    client: Client,
    base: String,
}

impl Mailpit {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base: "http://localhost:8025".to_string(),
        }
    }

    /// Wipe every message. Call at the start of a test that reads mails so ordering is safe.
    pub async fn wipe(&self) {
        let _ = self
            .client
            .delete(format!("{}/api/v1/messages", self.base))
            .send()
            .await;
    }

    /// Poll until at least one message addressed to `to` appears, then return the newest.
    /// Returns the raw JSON of the message.
    pub async fn wait_for(&self, to: &str, timeout_ms: u64) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            let resp = self
                .client
                .get(format!("{}/api/v1/search?query=to:{to}", self.base))
                .send()
                .await
                .expect("mailpit search failed");
            if resp.status().is_success() {
                let body: Value = resp.json().await.expect("mailpit search decode");
                let messages = body["messages"].as_array().cloned().unwrap_or_default();
                if let Some(msg) = messages.first() {
                    let id = msg["ID"].as_str().expect("no message ID");
                    let full = self
                        .client
                        .get(format!("{}/api/v1/message/{id}", self.base))
                        .send()
                        .await
                        .expect("mailpit fetch failed")
                        .json::<Value>()
                        .await
                        .expect("mailpit fetch decode");
                    return full;
                }
            }
            if std::time::Instant::now() >= deadline {
                panic!("no email for {to} within {timeout_ms}ms");
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    /// Extract the first URL-like token from the HTML body that ends with `?<param>=<value>`.
    /// Returns the raw value of the named query parameter.
    pub fn extract_token(msg: &Value, url_param: &str) -> Option<String> {
        let html = msg["HTML"].as_str().unwrap_or_default();
        let text = msg["Text"].as_str().unwrap_or_default();
        let hay = if html.is_empty() { text } else { html };
        let needle = format!("{url_param}=");
        let start = hay.find(&needle)? + needle.len();
        let end = hay[start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '<' || c == '&')
            .map(|e| start + e)
            .unwrap_or(hay.len());
        Some(hay[start..end].to_string())
    }

    /// Extract a 6-digit numeric code from either body of the message.
    pub fn extract_6digit_code(msg: &Value) -> Option<String> {
        let html = msg["HTML"].as_str().unwrap_or_default();
        let text = msg["Text"].as_str().unwrap_or_default();
        let hay = if html.is_empty() { text } else { html };
        // Look for the first standalone 6-digit sequence.
        let bytes = hay.as_bytes();
        let mut i = 0;
        while i + 6 <= bytes.len() {
            if bytes[i..i + 6].iter().all(|b| b.is_ascii_digit()) {
                let boundary_before = i == 0 || !bytes[i - 1].is_ascii_digit();
                let boundary_after = i + 6 == bytes.len() || !bytes[i + 6].is_ascii_digit();
                if boundary_before && boundary_after {
                    return Some(std::str::from_utf8(&bytes[i..i + 6]).unwrap().to_string());
                }
            }
            i += 1;
        }
        None
    }
}

// ─── TOTP helper ──────────────────────────────────────────────────

/// Compute the current TOTP code given the base32-encoded secret returned by `/auth/totp/setup`.
pub fn totp_now(secret_base32: &str) -> String {
    use totp_rs::{Algorithm, Secret, TOTP};
    let bytes = Secret::Encoded(secret_base32.to_string())
        .to_bytes()
        .expect("decode base32 TOTP secret");
    let totp =
        TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, None, "test".to_string()).expect("build TOTP");
    totp.generate_current().expect("compute TOTP")
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        // Spawn a blocking task to drop the test DB
        // This is best-effort cleanup
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let pool = PgPoolOptions::new()
                    .max_connections(2)
                    .connect(&test_db_url("skilluv"))
                    .await
                    .ok();

                if let Some(pool) = pool {
                    // Terminate connections to test DB
                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
                    )))
                    .execute(&pool)
                    .await;

                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db_name}\"")))
                        .execute(&pool)
                        .await;
                }
            });
        });
    }
}

/// Assert a money field by value rather than by the scale its column happened
/// to print.
///
/// `199`, `199.00` and `199.0000` are the same amount. A test that fails on
/// the difference is asserting a NUMERIC's scale, which is a storage decision
/// nobody promised a caller — and it breaks on the day somebody widens the
/// column for a currency with three decimal places.
#[track_caller]
#[allow(dead_code)]
pub fn assert_amount(actual: &Value, expected: &str) {
    let raw = match actual {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => panic!("not a decimal: {other}"),
    };
    let got: sqlx::types::BigDecimal = raw
        .parse()
        .unwrap_or_else(|e| panic!("{raw} is not a decimal: {e}"));
    let want: sqlx::types::BigDecimal = expected
        .parse()
        .unwrap_or_else(|e| panic!("{expected} is not a decimal: {e}"));
    assert_eq!(got, want, "expected {expected}, got {raw}");
}

/// The same comparison for a decimal read straight out of the database.
#[track_caller]
#[allow(dead_code)]
pub fn assert_decimal(actual: &sqlx::types::BigDecimal, expected: &str) {
    let want: sqlx::types::BigDecimal = expected
        .parse()
        .unwrap_or_else(|e| panic!("{expected} is not a decimal: {e}"));
    assert_eq!(actual, &want, "expected {expected}, got {actual}");
}

/// Give somebody a verified deliverable in a reviewer family.
///
/// The mentor matcher reads a mentor's families from what they have actually
/// delivered, not from what they told the wizard interests them — a mentor who
/// declared motion and never delivered any is not a motion mentor. So a test
/// that wants a mentor to be suggested has to give them work, and one that
/// only sets a craft score and a profile is describing the person the rule
/// exists to exclude.
#[allow(dead_code)]
pub async fn delivered_in(app: &TestApp, user: Uuid, domain: &str, family: &str) {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, description, owner_type, owner_id)
         VALUES ($1, 'Projet mentor', 'x', 'user', $2) RETURNING id",
    )
    .bind(format!("mentor-p-{}", Uuid::new_v4()))
    .bind(user)
    .fetch_one(&app.db)
    .await
    .expect("project for a delivered work");

    // The surface the work lives on, per domain. A design slice additionally
    // has to say what came out of it and which trade it belongs to.
    // The surface the work lives on, and what came out of it. Each domain has
    // its own subtype column, and the pairing is enforced both ways: an
    // `ai_artifact` slice must name an `ai_subtype`, and a slice of any other
    // type must leave it NULL. Setting only `design_subtype` therefore failed
    // for every domain except design.
    let (slice_type, subtype_column, subtype) = match domain {
        "code" => ("code_artifact", "code_subtype", "library_published"),
        "ai" => ("ai_artifact", "ai_subtype", "ml_model"),
        "audio" => ("audio_artifact", "audio_subtype", "composition"),
        "design" => ("design_artifact", "design_subtype", "interface"),
        "ops" => ("ops_artifact", "ops_subtype", "iac_terraform"),
        "communication" => (
            "communication_artifact",
            "communication_subtype",
            "blog_post",
        ),
        "education" => (
            "education_artifact",
            "education_subtype",
            "workshop_material",
        ),
        other => panic!("no slice type known for the {other} domain"),
    };

    // `published_artifact_url` on every one of them. Two subtypes demand it —
    // an `ml_model` and a `library_published` are claims about something a
    // stranger can fetch, and the schema refuses one that says nowhere. Giving
    // it to all five is simpler than tracking which, and it is never wrong:
    // this helper exists to describe work that was delivered.
    let slice: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, orientation_id, published_artifact_url, {subtype_column})
         VALUES ($1, $2, 'Travail', 'Un brief.', $3, 3, 'validated',
                 (SELECT id FROM orientations
                   WHERE primary_domain = $3 AND reviewer_group = $4
                     AND is_archived = FALSE
                   ORDER BY slug LIMIT 1),
                 'https://example.test/delivered',
                 $5)
         RETURNING id"
    )))
    .bind(project)
    .bind(slice_type)
    .bind(domain)
    .bind(family)
    .bind(subtype)
    .fetch_one(&app.db)
    .await
    .unwrap_or_else(|e| panic!("slice in {domain}/{family}: {e}"));

    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at, public)
         VALUES ($1, $2, 'other', 'https://example.test/x', 'human_review',
                 'verified', NOW(), TRUE)",
    )
    .bind(slice)
    .bind(user)
    .execute(&app.db)
    .await
    .expect("verified deliverable");
}
