//! Enterprise B2B SSO — OIDC config storage, encryption, discovery cache.
//!
//! The IdP client_secret is stored encrypted at-rest in AES-256-GCM with a
//! dedicated 32-byte key (`SSO_ENCRYPTION_KEY` env var). Rotation of the key
//! invalidates all existing configs — plan a re-encrypt migration if you rotate.
//!
//! The actual OIDC flow (authorize URL, code exchange, ID token verification)
//! lives in `routes::enterprise_sso` where the `openidconnect` crate types
//! stay local. This module handles storage and provisioning only.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, AeadCore},
    Aes256Gcm, Key, Nonce,
};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

// ─── DB row ──────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SsoConfigRow {
    pub enterprise_id: Uuid,
    pub issuer: String,
    pub client_id: String,
    pub client_secret_encrypted: Vec<u8>,
    pub client_secret_nonce: Vec<u8>,
    pub email_domains: Vec<String>,
    pub enforce_sso: bool,
    pub auto_provision: bool,
    pub default_role: String,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SsoConfigRow {
    pub fn is_active(&self) -> bool {
        self.disabled_at.is_none()
    }
}

// ─── Encryption (AES-256-GCM with dedicated key) ─────────────────

pub fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| AppError::Internal("sso secret encrypt failed".into()))?;
    Ok((ciphertext, nonce.to_vec()))
}

pub fn decrypt_secret(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce_bytes: &[u8],
) -> Result<String, AppError> {
    if nonce_bytes.len() != 12 {
        return Err(AppError::Internal("sso invalid nonce length".into()));
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Internal("sso secret decrypt failed".into()))?;
    String::from_utf8(plain).map_err(|_| AppError::Internal("sso secret utf8 invalid".into()))
}

// ─── DB CRUD ─────────────────────────────────────────────────────

pub struct UpsertConfig<'a> {
    pub enterprise_id: Uuid,
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub client_secret_encrypted: &'a [u8],
    pub client_secret_nonce: &'a [u8],
    pub email_domains: &'a [String],
    pub enforce_sso: bool,
    pub auto_provision: bool,
    pub default_role: &'a str,
}

pub async fn upsert(db: &PgPool, cfg: UpsertConfig<'_>) -> Result<SsoConfigRow, AppError> {
    let row = sqlx::query_as(
        r#"
        INSERT INTO enterprise_sso_configs
            (enterprise_id, issuer, client_id, client_secret_encrypted, client_secret_nonce,
             email_domains, enforce_sso, auto_provision, default_role)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (enterprise_id) DO UPDATE SET
            issuer = EXCLUDED.issuer,
            client_id = EXCLUDED.client_id,
            client_secret_encrypted = EXCLUDED.client_secret_encrypted,
            client_secret_nonce = EXCLUDED.client_secret_nonce,
            email_domains = EXCLUDED.email_domains,
            enforce_sso = EXCLUDED.enforce_sso,
            auto_provision = EXCLUDED.auto_provision,
            default_role = EXCLUDED.default_role,
            disabled_at = NULL,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(cfg.enterprise_id)
    .bind(cfg.issuer)
    .bind(cfg.client_id)
    .bind(cfg.client_secret_encrypted)
    .bind(cfg.client_secret_nonce)
    .bind(cfg.email_domains)
    .bind(cfg.enforce_sso)
    .bind(cfg.auto_provision)
    .bind(cfg.default_role)
    .fetch_one(db)
    .await?;
    Ok(row)
}

pub async fn get_by_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Option<SsoConfigRow>, AppError> {
    let row = sqlx::query_as("SELECT * FROM enterprise_sso_configs WHERE enterprise_id = $1")
        .bind(enterprise_id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

pub async fn get_by_slug(db: &PgPool, slug: &str) -> Result<Option<SsoConfigRow>, AppError> {
    let row = sqlx::query_as(
        r#"
        SELECT c.* FROM enterprise_sso_configs c
        JOIN enterprises e ON e.id = c.enterprise_id
        WHERE e.slug = $1 AND c.disabled_at IS NULL
        "#,
    )
    .bind(slug)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

pub async fn disable(db: &PgPool, enterprise_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE enterprise_sso_configs SET disabled_at = NOW(), updated_at = NOW() WHERE enterprise_id = $1",
    )
    .bind(enterprise_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Find an active SSO config whose `email_domains` array contains the given
/// domain. Returns the config paired with the enterprise slug (needed for the
/// SSO start URL). Used both for the discovery endpoint and for `enforce_sso`
/// on login.
pub async fn find_by_email_domain(
    db: &PgPool,
    domain: &str,
) -> Result<Option<(SsoConfigRow, String)>, AppError> {
    let cfg: Option<SsoConfigRow> = sqlx::query_as(
        r#"
        SELECT c.* FROM enterprise_sso_configs c
        WHERE c.disabled_at IS NULL
          AND $1 = ANY(c.email_domains)
        LIMIT 1
        "#,
    )
    .bind(domain.to_lowercase())
    .fetch_optional(db)
    .await?;
    let Some(cfg) = cfg else { return Ok(None) };
    let slug: (String,) = sqlx::query_as("SELECT slug FROM enterprises WHERE id = $1")
        .bind(cfg.enterprise_id)
        .fetch_one(db)
        .await?;
    Ok(Some((cfg, slug.0)))
}

// ─── OIDC discovery cache (Redis) ────────────────────────────────

const DISCOVERY_TTL_SECS: u64 = 24 * 60 * 60;

fn discovery_cache_key(issuer: &str) -> String {
    format!("oidc_discovery:{issuer}")
}

/// Cache the raw discovery document (JSON). The OIDC client re-parses it on
/// each use — cheap compared to a network round-trip to the IdP.
pub async fn cache_discovery(
    redis: &mut ConnectionManager,
    issuer: &str,
    document: &str,
) -> Result<(), AppError> {
    let key = discovery_cache_key(issuer);
    let () = redis.set_ex(&key, document, DISCOVERY_TTL_SECS).await?;
    Ok(())
}

pub async fn get_cached_discovery(
    redis: &mut ConnectionManager,
    issuer: &str,
) -> Result<Option<String>, AppError> {
    let key = discovery_cache_key(issuer);
    let raw: Option<String> = redis.get(&key).await?;
    Ok(raw)
}

// ─── SSO login state (Redis) ─────────────────────────────────────

pub const SSO_STATE_TTL_SECS: u64 = 10 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoLoginState {
    pub enterprise_id: Uuid,
    pub enterprise_slug: String,
    pub pkce_verifier: String,
    pub nonce: String,
}

pub async fn store_login_state(
    redis: &mut ConnectionManager,
    state: &SsoLoginState,
) -> Result<String, AppError> {
    let token = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let key = format!("sso_state:{token}");
    let payload = serde_json::to_string(state)
        .map_err(|e| AppError::Internal(format!("sso state serialize: {e}")))?;
    let () = redis.set_ex(&key, payload, SSO_STATE_TTL_SECS).await?;
    Ok(token)
}

pub async fn consume_login_state(
    redis: &mut ConnectionManager,
    token: &str,
) -> Result<SsoLoginState, AppError> {
    let key = format!("sso_state:{token}");
    let raw: Option<String> = redis.get(&key).await?;
    let raw = raw.ok_or(AppError::Unauthorized)?;
    let _: () = redis.del(&key).await?;
    serde_json::from_str(&raw).map_err(|_| AppError::Unauthorized)
}

// ─── JIT provisioning ────────────────────────────────────────────

/// Find or create a user from SSO claims, then attach them to the enterprise.
/// Returns the resulting user_id.
///
/// Contracts:
/// - `email_verified` on the IdP claim must be true (caller enforces).
/// - `auto_provision` gating is enforced by the caller — this helper always
///   creates the user if missing and the caller decides whether to invoke it.
pub async fn provision_from_sso(
    db: &PgPool,
    enterprise_id: Uuid,
    email: &str,
    display_name: Option<&str>,
    default_role: &str,
) -> Result<Uuid, AppError> {
    let email_lower = email.trim().to_lowercase();

    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE LOWER(email) = $1")
            .bind(&email_lower)
            .fetch_optional(db)
            .await?;

    let user_id = if let Some((uid,)) = existing {
        uid
    } else {
        // Placeholder password (unusable). Users authenticate via SSO ; they can set
        // a password later via /auth/change-password if they want a fallback.
        let placeholder_hash =
            "$argon2id$v=19$m=19456,t=2,p=1$sso-placeholder$sso-placeholder";
        let base_username = display_name
            .and_then(|d| d.split_whitespace().next().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                email_lower
                    .split('@')
                    .next()
                    .unwrap_or("user")
                    .to_string()
            });
        let cleaned: String = base_username
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(20)
            .collect();
        let candidate = if cleaned.len() < 3 {
            format!("user{}", &Uuid::new_v4().simple().to_string()[..6])
        } else {
            let taken: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
                    .bind(&cleaned)
                    .fetch_optional(db)
                    .await?;
            if taken.is_some() {
                format!("{cleaned}-{}", &Uuid::new_v4().simple().to_string()[..4])
            } else {
                cleaned
            }
        };
        let display = display_name.unwrap_or(&candidate).to_string();
        let parts: Vec<&str> = display.split_whitespace().collect();
        let first = parts.first().copied().unwrap_or(&display).to_string();
        let last = if parts.len() >= 2 { parts[1..].join(" ") } else { String::new() };

        let inserted: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO users
                (email, username, password_hash, first_name, last_name, display_name,
                 skill_domain, email_verified, role)
            VALUES ($1, $2, $3, $4, $5, $6, NULL, TRUE, $7)
            RETURNING id
            "#,
        )
        .bind(&email_lower)
        .bind(&candidate)
        .bind(placeholder_hash)
        .bind(&first)
        .bind(&last)
        .bind(&display)
        .bind(default_role)
        .fetch_one(db)
        .await?;
        inserted.0
    };

    // Attach to enterprise (idempotent).
    sqlx::query(
        r#"
        INSERT INTO enterprise_members
            (enterprise_id, user_id, role, status, accepted_at)
        VALUES ($1, $2, $3, 'active', NOW())
        ON CONFLICT (enterprise_id, user_id) DO UPDATE SET
            status = 'active',
            accepted_at = COALESCE(enterprise_members.accepted_at, NOW())
        "#,
    )
    .bind(enterprise_id)
    .bind(user_id)
    .bind(default_role)
    .execute(db)
    .await?;

    // Ensure the global users.role reflects membership (skip for existing admins/owners).
    sqlx::query(
        "UPDATE users SET role = $1, updated_at = NOW() WHERE id = $2 AND role NOT IN ('enterprise', 'admin')",
    )
    .bind(default_role)
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(user_id)
}
