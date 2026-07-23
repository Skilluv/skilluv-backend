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

/// A test application instance with isolated database.
pub struct TestApp {
    pub addr: String,
    pub db: PgPool,
    pub client: Client,
    db_name: String,
}

impl TestApp {
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
            .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
            .await
            .expect("Failed to connect to admin DB");

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE \"{db_name}\""
        )))
        .execute(&admin_pool)
        .await
        .expect("Failed to create test DB");

        admin_pool.close().await;

        // Connect to test DB
        let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
        let db = PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test DB");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&db)
            .await
            .expect("Failed to run migrations on test DB");

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
            judge0_url: "http://localhost:2358".to_string(),
            minio_endpoint: "http://localhost:9004".to_string(),
            minio_access_key: "skilluv".to_string(),
            minio_secret_key: "skilluv_secret".to_string(),
            minio_bucket: format!("test-{}", &db_name[..20]),
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
        };

        let storage =
            Arc::new(skilluv_backend::services::StorageService::new(&storage_config).await);

        let ws = skilluv_backend::websocket::WsManager::new();

        let queue = Arc::new(skilluv_backend::services::QueueService::new(redis.clone()));

        let geo = Arc::new(
            skilluv_backend::services::GeoService::load(std::path::Path::new("data"))
                .expect("Failed to load GeoNames data from ./data"),
        );

        // Bind to random port FIRST so the app's `base_url` can be aligned with
        // the actual test server address. Otherwise features that mint absolute
        // URLs (SSO redirect_uri, webhook callbacks, etc.) point at a hardcoded
        // port that the tests don't listen on.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind");
        let addr = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());

        let state = AppState {
            db: db.clone(),
            redis,
            config: AppStateConfig {
                jwt_secret: "test-secret-key-for-testing".to_string(),
                base_url: addr.clone(),
                sso_encryption_key: Some([42u8; 32]),
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

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body: Value = resp
            .json()
            .await
            .expect("Failed to parse register response");

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
    pub async fn login(&self, identifier: &str) -> Value {
        let resp = self
            .client
            .post(format!("{}/api/auth/login", self.addr))
            .json(&json!({
                "identifier": identifier,
                "password": Self::TEST_PASSWORD,
            }))
            .send()
            .await
            .expect("Login request failed");

        assert_eq!(resp.status(), StatusCode::OK);
        resp.json().await.expect("Failed to parse login response")
    }

    /// Register a user and set them as admin in the DB.
    pub async fn register_admin(&self, username: &str) -> Value {
        let result = self.register_user(username).await;
        let user_id = result["data"]["user"]["id"].as_str().expect("No user id");

        // `role = 'admin'` + `totp_enabled = TRUE` en une passe : le middleware
        // `ensure_admin_2fa` bloque tout admin sans TOTP ni passkey (renvoi 403
        // AdminTwoFaSetupRequired). En prod l'admin a le pop-up d'activation TOTP
        // au premier login ; en tests on simule l'etat post-activation directement.
        sqlx::query("UPDATE users SET role = 'admin', totp_enabled = TRUE WHERE id = $1::UUID")
            .bind(user_id)
            .execute(&self.db)
            .await
            .expect("Failed to set admin role");

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

    /// DELETE helper.
    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}{}", self.addr, path))
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
                    .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
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
