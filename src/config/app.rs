use std::env;

pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub database_url: String,
    pub redis_url: String,
    pub base_url: String,
    pub judge0_url: String,
    pub minio_endpoint: String,
    pub minio_access_key: String,
    pub minio_secret_key: String,
    pub minio_bucket: String,
    pub avatar_cdn_base_url: Option<String>,
    pub grpc_ai_url: Option<String>,
    pub brevo_api_key: Option<String>,
    pub email_from: String,
    pub email_from_name: String,
    pub environment: String,
    /// Enterprise SSO client_secret encryption key. 32 bytes, base64.
    /// Required in production ; optional otherwise (SSO endpoints will 500
    /// if not configured when a config is created).
    pub sso_encryption_key: Option<[u8; 32]>,
    pub sentry_dsn: Option<String>,
    pub sentry_traces_sample_rate: f32,
    pub release: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a valid u16"),
            jwt_secret: env::var("JWT_SECRET").expect("JWT_SECRET must be set"),
            database_url: env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            base_url: env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3001".to_string()),
            judge0_url: env::var("JUDGE0_URL")
                .unwrap_or_else(|_| "http://localhost:2358".to_string()),
            minio_endpoint: env::var("MINIO_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:9002".to_string()),
            minio_access_key: env::var("MINIO_ACCESS_KEY")
                .unwrap_or_else(|_| "skilluv".to_string()),
            minio_secret_key: env::var("MINIO_SECRET_KEY")
                .unwrap_or_else(|_| "skilluv_secret".to_string()),
            minio_bucket: env::var("MINIO_BUCKET").unwrap_or_else(|_| "avatars".to_string()),
            avatar_cdn_base_url: env::var("AVATAR_CDN_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            grpc_ai_url: env::var("GRPC_AI_URL").ok(),
            brevo_api_key: env::var("BREVO_API_KEY").ok(),
            email_from: env::var("EMAIL_FROM")
                .unwrap_or_else(|_| "noreply@skilluv.com".to_string()),
            email_from_name: env::var("EMAIL_FROM_NAME").unwrap_or_else(|_| "Skilluv".to_string()),
            environment: env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".to_string()),
            sso_encryption_key: env::var("SSO_ENCRYPTION_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .and_then(|s| {
                    use base64::Engine;
                    let bytes = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
                    <[u8; 32]>::try_from(bytes.as_slice()).ok()
                }),
            sentry_dsn: env::var("SENTRY_DSN").ok().filter(|s| !s.is_empty()),
            sentry_traces_sample_rate: env::var("SENTRY_TRACES_SAMPLE_RATE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1),
            release: env::var("RELEASE")
                .ok()
                .or_else(|| option_env!("CARGO_PKG_VERSION").map(String::from)),
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Hard-fail if the runtime environment is production and any insecure default is still in place.
    /// Call from `main` after `from_env`. Logs warnings for staging.
    pub fn assert_production_secrets(&self) {
        let is_prod = self.environment == "prod" || self.environment == "production";
        let issues = self.audit_secrets();
        if issues.is_empty() {
            return;
        }
        if is_prod {
            for issue in &issues {
                tracing::error!(issue, "production secret check failed");
            }
            panic!(
                "Refusing to start in prod with insecure defaults: {}",
                issues.join(", ")
            );
        }
        for issue in &issues {
            tracing::warn!(issue, "secret hygiene warning (non-prod)");
        }
    }

    fn audit_secrets(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.jwt_secret == "change-this-to-a-secure-random-string" || self.jwt_secret.len() < 32
        {
            issues.push("JWT_SECRET is the default or under 32 chars".into());
        }
        if self.minio_access_key == "skilluv" {
            issues.push("MINIO_ACCESS_KEY uses dev default".into());
        }
        if self.minio_secret_key == "skilluv_secret" {
            issues.push("MINIO_SECRET_KEY uses dev default".into());
        }
        if self.database_url.contains("skilluv_secret") {
            issues.push("DATABASE_URL still contains 'skilluv_secret'".into());
        }
        if self.sso_encryption_key.is_none() {
            issues.push("SSO_ENCRYPTION_KEY is not set (32 bytes base64)".into());
        }
        issues
    }
}
