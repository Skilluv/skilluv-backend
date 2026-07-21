//! GitHub OAuth + sync — Phase 2 Sprint 5.
//!
//! - OAuth tokens are stored encrypted at-rest using ChaCha20-Poly1305 with a key derived
//!   from `JWT_SECRET` via HMAC-SHA256 (so rotating JWT_SECRET also invalidates tokens — a
//!   feature, not a bug).
//! - Sync pulls public repos only (scope `read:user`, `public_repo`).

use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, AeadCore, Generate, KeyInit},
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit as HmacKeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

type HmacSha256 = Hmac<Sha256>;

const GITHUB_API: &str = "https://api.github.com";
const GITHUB_OAUTH: &str = "https://github.com/login/oauth";
pub const USER_AGENT: &str = "skilluv-backend";

// ─── Encryption ───────────────────────────────────────────────────

/// Derive a 32-byte symmetric key from the JWT secret. Domain-separated so that the same
/// JWT_SECRET cannot be misused as both signing key and encryption key.
fn derive_token_key(jwt_secret: &str) -> [u8; 32] {
    let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(jwt_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"skilluv-github-token-v1");
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

pub fn encrypt_token(jwt_secret: &str, token: &str) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let key_bytes = derive_token_key(jwt_secret);
    let key = Key::try_from(key_bytes.as_slice())
        .map_err(|_| AppError::Internal("github token key size invalid".into()))?;
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = Nonce::generate();
    let ciphertext = cipher
        .encrypt(&nonce, token.as_bytes())
        .map_err(|_| AppError::Internal("github token encryption failed".into()))?;
    Ok((ciphertext, nonce.to_vec()))
}

pub fn decrypt_token(
    jwt_secret: &str,
    ciphertext: &[u8],
    nonce_bytes: &[u8],
) -> Result<String, AppError> {
    if nonce_bytes.len() != 12 {
        return Err(AppError::Internal("invalid nonce length".into()));
    }
    let key_bytes = derive_token_key(jwt_secret);
    let key = Key::try_from(key_bytes.as_slice())
        .map_err(|_| AppError::Internal("github token key size invalid".into()))?;
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = Nonce::try_from(nonce_bytes)
        .map_err(|_| AppError::Internal("nonce parse failed".into()))?;
    let plain = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| AppError::Internal("github token decryption failed".into()))?;
    String::from_utf8(plain).map_err(|_| AppError::Internal("github token utf8 invalid".into()))
}

// ─── OAuth state ─────────────────────────────────────────────────

/// Build the authorization URL for the GitHub OAuth flow.
pub fn build_authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    let qs = format!(
        "client_id={}&redirect_uri={}&scope=read:user%20public_repo&state={}",
        urlencoding(client_id),
        urlencoding(redirect_uri),
        urlencoding(state)
    );
    format!("{GITHUB_OAUTH}/authorize?{qs}")
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
    pub name: Option<String>,
}

pub async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<(String, Option<String>), AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{GITHUB_OAUTH}/access_token"))
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github token exchange failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github token exchange status {}",
            resp.status()
        )));
    }
    let body: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github token decode failed: {e}")))?;
    if let Some(err) = body.error {
        return Err(AppError::Validation(format!(
            "github oauth error: {} — {}",
            err,
            body.error_description.unwrap_or_default()
        )));
    }
    let token = body
        .access_token
        .ok_or_else(|| AppError::Internal("github returned no access_token".into()))?;
    Ok((token, body.scope))
}

pub async fn fetch_user(access_token: &str) -> Result<GitHubUser, AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{GITHUB_API}/user"))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github user fetch failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github /user status {}",
            resp.status()
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("github /user decode failed: {e}")))
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    id: i64,
    name: String,
    full_name: String,
    description: Option<String>,
    html_url: String,
    homepage: Option<String>,
    language: Option<String>,
    stargazers_count: i32,
    forks_count: i32,
    open_issues_count: i32,
    archived: bool,
    fork: bool,
    pushed_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

pub async fn fetch_public_repos(
    access_token: &str,
    github_login: &str,
) -> Result<Vec<GitHubRepo>, AppError> {
    let client = reqwest::Client::new();
    let mut all = Vec::new();
    for page in 1..=5 {
        // Cap at 5 pages (500 repos) to bound work per sync.
        let resp = client
            .get(format!(
                "{GITHUB_API}/users/{github_login}/repos?type=owner&sort=pushed&per_page=100&page={page}"
            ))
            .bearer_auth(access_token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("github repos fetch failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "github /repos status {}",
                resp.status()
            )));
        }
        let batch: Vec<GitHubRepo> = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("github /repos decode failed: {e}")))?;
        let was_full = batch.len() == 100;
        all.extend(batch);
        if !was_full {
            break;
        }
    }
    Ok(all)
}

// ─── Storage ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GithubConnection {
    pub user_id: Uuid,
    pub github_user_id: i64,
    pub github_login: String,
    pub scopes: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn upsert_connection(
    db: &PgPool,
    user_id: Uuid,
    github_user_id: i64,
    github_login: &str,
    scopes: Option<&str>,
    encrypted_token: &[u8],
    nonce: &[u8],
) -> Result<GithubConnection, AppError> {
    let conn: GithubConnection = sqlx::query_as(
        r#"
        INSERT INTO github_connections
            (user_id, github_user_id, github_login, access_token_encrypted, access_token_nonce, scopes)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id) DO UPDATE SET
            github_user_id = EXCLUDED.github_user_id,
            github_login = EXCLUDED.github_login,
            access_token_encrypted = EXCLUDED.access_token_encrypted,
            access_token_nonce = EXCLUDED.access_token_nonce,
            scopes = EXCLUDED.scopes,
            updated_at = NOW()
        RETURNING user_id, github_user_id, github_login, scopes, last_synced_at, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(github_user_id)
    .bind(github_login)
    .bind(encrypted_token)
    .bind(nonce)
    .bind(scopes)
    .fetch_one(db)
    .await?;
    Ok(conn)
}

pub async fn load_token(
    db: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT access_token_encrypted, access_token_nonce FROM github_connections WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    match row {
        Some((ct, nonce)) => Ok(Some(decrypt_token(jwt_secret, &ct, &nonce)?)),
        None => Ok(None),
    }
}

pub async fn disconnect(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM github_connections WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM github_repos WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

// ─── Repos sync ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub repos_total: usize,
    pub repos_upserted: usize,
}

pub async fn sync_repos_for(
    db: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
) -> Result<SyncReport, AppError> {
    let conn: Option<(String,)> =
        sqlx::query_as("SELECT github_login FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let (github_login,) = conn.ok_or(AppError::NotFound("user has not connected GitHub".into()))?;
    let token = load_token(db, jwt_secret, user_id)
        .await?
        .ok_or(AppError::Internal("token missing".into()))?;
    let repos = fetch_public_repos(&token, &github_login).await?;
    let total = repos.len();

    let mut upserted = 0usize;
    for r in &repos {
        let res = sqlx::query(
            r#"
            INSERT INTO github_repos
                (id, user_id, full_name, name, description, html_url, homepage, language,
                 stargazers_count, forks_count, open_issues_count, archived, fork,
                 pushed_at, created_at_github, synced_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15, NOW())
            ON CONFLICT (id) DO UPDATE SET
                full_name = EXCLUDED.full_name,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                html_url = EXCLUDED.html_url,
                homepage = EXCLUDED.homepage,
                language = EXCLUDED.language,
                stargazers_count = EXCLUDED.stargazers_count,
                forks_count = EXCLUDED.forks_count,
                open_issues_count = EXCLUDED.open_issues_count,
                archived = EXCLUDED.archived,
                fork = EXCLUDED.fork,
                pushed_at = EXCLUDED.pushed_at,
                synced_at = NOW()
            "#,
        )
        .bind(r.id)
        .bind(user_id)
        .bind(&r.full_name)
        .bind(&r.name)
        .bind(&r.description)
        .bind(&r.html_url)
        .bind(&r.homepage)
        .bind(&r.language)
        .bind(r.stargazers_count)
        .bind(r.forks_count)
        .bind(r.open_issues_count)
        .bind(r.archived)
        .bind(r.fork)
        .bind(r.pushed_at)
        .bind(r.created_at)
        .execute(db)
        .await;
        if res.is_ok() {
            upserted += 1;
        }
    }

    sqlx::query("UPDATE github_connections SET last_synced_at = NOW() WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;

    Ok(SyncReport {
        repos_total: total,
        repos_upserted: upserted,
    })
}

pub async fn top_repos_for_user(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<RepoSummary>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, full_name, description, html_url, language, stargazers_count, pushed_at
        FROM github_repos
        WHERE user_id = $1 AND archived = FALSE AND fork = FALSE
        ORDER BY stargazers_count DESC, pushed_at DESC NULLS LAST
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit.clamp(1, 50))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RepoSummary {
    pub id: i64,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub language: Option<String>,
    pub stargazers_count: i32,
    pub pushed_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let (ct, nonce) = encrypt_token("super-secret", "gho_abc123").unwrap();
        let plain = decrypt_token("super-secret", &ct, &nonce).unwrap();
        assert_eq!(plain, "gho_abc123");
    }

    #[test]
    fn decrypt_fails_with_wrong_secret() {
        let (ct, nonce) = encrypt_token("secret-a", "gho_abc").unwrap();
        assert!(decrypt_token("secret-b", &ct, &nonce).is_err());
    }

    #[test]
    fn decrypt_fails_with_tampered_ciphertext() {
        let (mut ct, nonce) = encrypt_token("secret", "gho_abc").unwrap();
        ct[0] ^= 0xFF;
        assert!(decrypt_token("secret", &ct, &nonce).is_err());
    }

    #[test]
    fn authorize_url_includes_required_params() {
        let url = build_authorize_url("CID", "https://app.skilluv.com/cb", "rand-state");
        assert!(url.contains("client_id=CID"));
        assert!(url.contains("scope=read:user%20public_repo"));
        assert!(url.contains("state=rand-state"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp.skilluv.com%2Fcb"));
    }
}
