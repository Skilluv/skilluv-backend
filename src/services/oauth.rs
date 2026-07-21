//! Multi-provider OAuth — Phase 3.1 + 3.2.
//!
//! Unified linking + login logic across GitHub, Google, LinkedIn.
//! Each provider is a thin adapter that returns a normalised `OAuthProfile`.

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const VALID_PROVIDERS: &[&str] = &["github", "google", "linkedin"];

/// Normalised OAuth profile returned by any provider adapter.
#[derive(Debug, Clone)]
pub struct OAuthProfile {
    pub provider: &'static str,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub username: Option<String>, // only GitHub really has a username
}

// ─── State token (Redis) ──────────────────────────────────────────

pub const STATE_TTL_SECS: u64 = 15 * 60;

/// Kind of OAuth flow the state represents. `Link` requires an authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthState {
    pub provider: String,
    /// Optional: the Skilluv user this flow should attach to. If None, this is a
    /// signup/login flow ; the callback will find-or-create the user.
    pub user_id: Option<Uuid>,
    pub intent: String, // "signup_login" | "link"
    pub redirect_after: Option<String>,
    /// When set, the OAuth flow completes an enterprise recruiter invite.
    /// The provider-returned email MUST match the invited email (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite_token: Option<String>,
}

pub async fn store_state(
    redis: &mut ConnectionManager,
    state: &OAuthState,
) -> Result<String, AppError> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let key = format!("oauth_state:{token}");
    let payload = serde_json::to_string(state)
        .map_err(|e| AppError::Internal(format!("state serialize: {e}")))?;
    let () = redis.set_ex(&key, payload, STATE_TTL_SECS).await?;
    Ok(token)
}

pub async fn consume_state(
    redis: &mut ConnectionManager,
    token: &str,
) -> Result<OAuthState, AppError> {
    let key = format!("oauth_state:{token}");
    let raw: Option<String> = redis.get(&key).await?;
    let raw = raw.ok_or(AppError::Unauthorized)?;
    let _: () = redis.del(&key).await?;
    serde_json::from_str(&raw).map_err(|_| AppError::Unauthorized)
}

// ─── Storage ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LinkedProvider {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub linked_at: DateTime<Utc>,
}

pub async fn list_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<LinkedProvider>, AppError> {
    let rows =
        sqlx::query_as("SELECT * FROM user_oauth_providers WHERE user_id = $1 ORDER BY linked_at")
            .bind(user_id)
            .fetch_all(db)
            .await?;
    Ok(rows)
}

pub async fn upsert_link(
    db: &PgPool,
    user_id: Uuid,
    profile: &OAuthProfile,
) -> Result<LinkedProvider, AppError> {
    let row: LinkedProvider = sqlx::query_as(
        r#"
        INSERT INTO user_oauth_providers
            (user_id, provider, provider_user_id, email, display_name, avatar_url)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id, provider) DO UPDATE SET
            provider_user_id = EXCLUDED.provider_user_id,
            email = EXCLUDED.email,
            display_name = EXCLUDED.display_name,
            avatar_url = EXCLUDED.avatar_url,
            linked_at = NOW()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(profile.provider)
    .bind(&profile.provider_user_id)
    .bind(&profile.email)
    .bind(&profile.display_name)
    .bind(&profile.avatar_url)
    .fetch_one(db)
    .await?;
    Ok(row)
}

pub async fn unlink(db: &PgPool, user_id: Uuid, provider: &str) -> Result<(), AppError> {
    // Refuse to unlink if it's the user's only sign-in method (no password_hash + last provider).
    let (count, has_password): (i64, bool) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM user_oauth_providers WHERE user_id = $1)::BIGINT,
            (SELECT password_hash IS NOT NULL AND password_hash NOT LIKE '$argon2%$seed-placeholder$%' FROM users WHERE id = $1)
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if !has_password && count <= 1 {
        return Err(AppError::Validation(
            "Cannot unlink your last sign-in method. Add a password or link another provider first.".into(),
        ));
    }
    sqlx::query("DELETE FROM user_oauth_providers WHERE user_id = $1 AND provider = $2")
        .bind(user_id)
        .bind(provider)
        .execute(db)
        .await?;
    if provider == "github" {
        // Also clear the encrypted token blob (Sprint 5 table)
        let _ = sqlx::query("DELETE FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .execute(db)
            .await;
    }
    Ok(())
}

/// Look up an existing Skilluv user by provider identity. Falls back to email match.
pub async fn find_user_for_profile(
    db: &PgPool,
    profile: &OAuthProfile,
) -> Result<Option<Uuid>, AppError> {
    let by_link: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_oauth_providers WHERE provider = $1 AND provider_user_id = $2",
    )
    .bind(profile.provider)
    .bind(&profile.provider_user_id)
    .fetch_optional(db)
    .await?;
    if let Some((uid,)) = by_link {
        return Ok(Some(uid));
    }
    if profile.email_verified {
        if let Some(email) = &profile.email {
            let by_email: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM users WHERE LOWER(email) = LOWER($1)")
                    .bind(email)
                    .fetch_optional(db)
                    .await?;
            return Ok(by_email.map(|(id,)| id));
        }
    }
    Ok(None)
}

// ─── Provider adapters ───────────────────────────────────────────

pub mod google {
    use super::*;

    pub struct Config {
        pub client_id: String,
        pub client_secret: String,
        pub redirect_uri: String,
    }

    impl Config {
        pub fn from_env() -> Option<Self> {
            Some(Self {
                client_id: std::env::var("GOOGLE_CLIENT_ID")
                    .ok()
                    .filter(|s| !s.is_empty())?,
                client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
                    .ok()
                    .filter(|s| !s.is_empty())?,
                redirect_uri: std::env::var("GOOGLE_REDIRECT_URI")
                    .ok()
                    .filter(|s| !s.is_empty())?,
            })
        }
    }

    pub fn authorize_url(cfg: &Config, state: &str) -> String {
        let params = [
            ("client_id", cfg.client_id.as_str()),
            ("redirect_uri", cfg.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "openid email profile"),
            ("access_type", "online"),
            ("state", state),
            ("prompt", "select_account"),
        ];
        let qs = params
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencode(v)))
            .collect::<Vec<_>>()
            .join("&");
        format!("https://accounts.google.com/o/oauth2/v2/auth?{qs}")
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    #[derive(Deserialize)]
    struct UserInfo {
        sub: String,
        email: Option<String>,
        email_verified: Option<bool>,
        name: Option<String>,
        picture: Option<String>,
    }

    pub async fn fetch_profile(cfg: &Config, code: &str) -> Result<OAuthProfile, AppError> {
        let client = reqwest::Client::new();
        let token_resp: TokenResponse = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", cfg.client_id.as_str()),
                ("client_secret", cfg.client_secret.as_str()),
                ("redirect_uri", cfg.redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("google token exchange: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("google token exchange status: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("google token decode: {e}")))?;

        let info: UserInfo = client
            .get("https://www.googleapis.com/oauth2/v3/userinfo")
            .bearer_auth(&token_resp.access_token)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("google userinfo: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("google userinfo status: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("google userinfo decode: {e}")))?;

        Ok(OAuthProfile {
            provider: "google",
            provider_user_id: info.sub,
            email: info.email,
            email_verified: info.email_verified.unwrap_or(false),
            display_name: info.name,
            avatar_url: info.picture,
            username: None,
        })
    }
}

pub mod linkedin {
    use super::*;

    pub struct Config {
        pub client_id: String,
        pub client_secret: String,
        pub redirect_uri: String,
    }

    impl Config {
        pub fn from_env() -> Option<Self> {
            Some(Self {
                client_id: std::env::var("LINKEDIN_CLIENT_ID")
                    .ok()
                    .filter(|s| !s.is_empty())?,
                client_secret: std::env::var("LINKEDIN_CLIENT_SECRET")
                    .ok()
                    .filter(|s| !s.is_empty())?,
                redirect_uri: std::env::var("LINKEDIN_REDIRECT_URI")
                    .ok()
                    .filter(|s| !s.is_empty())?,
            })
        }
    }

    pub fn authorize_url(cfg: &Config, state: &str) -> String {
        // Uses the OpenID Connect flow (`openid profile email`), available on all
        // LinkedIn OAuth apps since Aug 2023 without needing partner access.
        let params = [
            ("response_type", "code"),
            ("client_id", cfg.client_id.as_str()),
            ("redirect_uri", cfg.redirect_uri.as_str()),
            ("scope", "openid profile email"),
            ("state", state),
        ];
        let qs = params
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencode(v)))
            .collect::<Vec<_>>()
            .join("&");
        format!("https://www.linkedin.com/oauth/v2/authorization?{qs}")
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    #[derive(Deserialize)]
    struct UserInfo {
        sub: String,
        email: Option<String>,
        email_verified: Option<bool>,
        name: Option<String>,
        picture: Option<String>,
    }

    pub async fn fetch_profile(cfg: &Config, code: &str) -> Result<OAuthProfile, AppError> {
        let client = reqwest::Client::new();
        let token_resp: TokenResponse = client
            .post("https://www.linkedin.com/oauth/v2/accessToken")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", cfg.client_id.as_str()),
                ("client_secret", cfg.client_secret.as_str()),
                ("redirect_uri", cfg.redirect_uri.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("linkedin token exchange: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("linkedin token status: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("linkedin token decode: {e}")))?;

        let info: UserInfo = client
            .get("https://api.linkedin.com/v2/userinfo")
            .bearer_auth(&token_resp.access_token)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("linkedin userinfo: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("linkedin userinfo status: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("linkedin userinfo decode: {e}")))?;

        Ok(OAuthProfile {
            provider: "linkedin",
            provider_user_id: info.sub,
            email: info.email,
            email_verified: info.email_verified.unwrap_or(false),
            display_name: info.name,
            avatar_url: info.picture,
            username: None,
        })
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
