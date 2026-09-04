use std::env;

pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub database_url: String,
    pub redis_url: String,
    /// API origin — machine-facing callbacks (SSO `redirect_uri`, WebAuthn RP).
    pub base_url: String,
    /// Frontend origin — every link a human clicks (emails, post-SSO redirect).
    /// Split from `base_url`, which points at the API subdomain in staging/prod.
    /// Falls back to `base_url` so single-origin deployments keep working.
    pub frontend_url: String,
    pub minio_endpoint: String,
    pub minio_access_key: String,
    pub minio_secret_key: String,
    pub minio_bucket: String,
    /// Private bucket for KYC docs, RGPD exports, anything not meant to be
    /// reachable via a stable URL. Split from `minio_bucket` (public) so the
    /// public bucket can safely enable anonymous `GetObject`.
    pub minio_bucket_private: String,
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
    /// External PDF renderer service URL (e.g. http://skilluv-pdf-renderer:8000).
    /// If unset, the invoice `/pdf` endpoint returns 503 rather than a broken
    /// blob. The service is expected to accept `POST {url}/render` with HTML in
    /// the body and return `application/pdf`.
    pub pdf_renderer_url: Option<String>,
}

/// The site's public address, for the places that cannot read the config.
///
/// Twenty-two places hardcoded `https://skilluv.com` — a domain nobody here
/// owns, which anyone could register and which every drip email, every
/// legal link and `security.txt` pointed at. The domain is `skill-uv.com`.
///
/// Prefer `config.frontend_url` wherever a config is reachable; this is the
/// fallback for static documents, schema examples and environment defaults.
pub const PUBLIC_SITE_URL: &str = "https://skill-uv.com";

impl AppConfig {
    pub fn from_env() -> Self {
        let base_url = env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3001".to_string());
        let frontend_url = Self::resolve_frontend_url(env::var("FRONTEND_URL").ok(), &base_url);
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
            base_url,
            frontend_url,
            minio_endpoint: env::var("MINIO_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:9002".to_string()),
            minio_access_key: env::var("MINIO_ACCESS_KEY")
                .unwrap_or_else(|_| "skilluv".to_string()),
            minio_secret_key: env::var("MINIO_SECRET_KEY")
                .unwrap_or_else(|_| "skilluv_secret".to_string()),
            minio_bucket: env::var("MINIO_BUCKET").unwrap_or_else(|_| "avatars".to_string()),
            minio_bucket_private: env::var("MINIO_BUCKET_PRIVATE")
                .unwrap_or_else(|_| "documents".to_string()),
            avatar_cdn_base_url: env::var("AVATAR_CDN_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            grpc_ai_url: env::var("GRPC_AI_URL").ok(),
            brevo_api_key: env::var("BREVO_API_KEY").ok(),
            email_from: env::var("EMAIL_FROM")
                .unwrap_or_else(|_| "noreply@skill-uv.com".to_string()),
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
            pdf_renderer_url: env::var("PDF_RENDERER_URL").ok().filter(|s| !s.is_empty()),
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Hard-fail if the runtime environment is production and any insecure default is still in place.
    /// Call from `main` after `from_env`. Logs warnings for staging.
    pub fn assert_production_secrets(&self) {
        let is_prod = self.environment == "prod" || self.environment == "production";
        // Warn rather than panic: serving both origins behind one reverse proxy
        // is legitimate, so this heuristic must not be able to block a boot.
        if self.frontend_url == self.base_url && Self::looks_like_api_host(&self.base_url) {
            tracing::error!(
                base_url = %self.base_url,
                "FRONTEND_URL is unset and BASE_URL points at an API host — \
                 emailed links (verify-email, reset-password, invites) will 404. \
                 Set FRONTEND_URL to the user-facing origin."
            );
        }
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

    fn resolve_frontend_url(raw: Option<String>, base_url: &str) -> String {
        raw.map(|s| s.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| base_url.to_string())
    }

    /// `https://api.example.com` → true, `https://example.com` → false.
    /// Narrow on purpose: only the conventional `api.` prefix counts.
    fn looks_like_api_host(url: &str) -> bool {
        url.split("://").nth(1).unwrap_or(url).starts_with("api.")
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
        // Belt-and-braces : dev-mode helpers (see src/routes/dev.rs) must NEVER
        // be reachable in prod. `dev_routes` handlers self-gate on the same
        // env var, but this check refuses to boot at all — clearer signal for
        // an operator who mis-set the env.
        if std::env::var("SKILLUV_DEV_MODE").as_deref() == Ok("true") {
            issues.push(
                "SKILLUV_DEV_MODE=true is set — dev helper endpoints would be reachable. \
                 Never enable this in prod, only in staging/local"
                    .into(),
            );
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn frontend_url_falls_back_to_base_url_when_unset_or_blank() {
        let base = "http://localhost:3001";
        assert_eq!(AppConfig::resolve_frontend_url(None, base), base);
        assert_eq!(
            AppConfig::resolve_frontend_url(Some(String::new()), base),
            base
        );
    }

    #[test]
    fn frontend_url_wins_over_base_url_and_drops_trailing_slash() {
        // Trailing slashes would produce `https://skill-uv.com//auth/reset-password`.
        assert_eq!(
            AppConfig::resolve_frontend_url(
                Some("https://skill-uv.com/".to_string()),
                "https://api.skill-uv.com"
            ),
            "https://skill-uv.com"
        );
    }

    #[test]
    fn api_host_heuristic_only_matches_the_api_subdomain() {
        assert!(AppConfig::looks_like_api_host("https://api.skill-uv.com"));
        assert!(!AppConfig::looks_like_api_host("https://skill-uv.com"));
        assert!(!AppConfig::looks_like_api_host("http://localhost:3001"));
    }
}
