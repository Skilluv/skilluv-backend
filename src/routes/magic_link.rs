//! Magic link authentication — Phase 4.17.
//!
//! Endpoints:
//!   POST /api/auth/magic-link/request  {email}
//!   POST /api/auth/magic-link/consume  {token}
//!
//! Flow:
//!   1. `request` generates a 128-bit random token, stores its SHA-256 hash + a 15-min TTL,
//!      and emails a link containing the raw token.
//!   2. `consume` hashes the incoming token, matches against the stored hash, marks it
//!      as consumed, and mints an access token cookie (creating the user if the intent
//!      was signup and no account exists yet).

use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{RateLimiter, extract_ip};
use crate::services::{AuthService, SessionService};

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type MagicLinkRow139 = (
    Uuid,
    String,
    String,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
);

pub const MAGIC_LINK_TTL_MIN: i64 = 15;

pub fn magic_link_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/magic-link/request", post(request_link))
        .route("/auth/magic-link/consume", post(consume_link))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn build_cookie(name: &str, value: &str, max_age_secs: i64, path: &str) -> String {
    format!("{name}={value}; HttpOnly; Secure; SameSite=Lax; Path={path}; Max-Age={max_age_secs}")
}

fn hash_token(token: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    h.finalize().to_vec()
}

#[derive(Deserialize)]
struct RequestBody {
    email: String,
    /// "login" | "signup". Defaults to "login" ; a signup-intent link creates the user
    /// on consumption if no account matches the email.
    intent: Option<String>,
}

async fn request_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RequestBody>,
) -> Result<Json<Value>, AppError> {
    let ip = extract_ip(&headers);
    RateLimiter::check(&mut state.redis.clone(), "magic_link", &ip, 5, 60).await?;
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 || email.len() > 255 {
        return Err(AppError::Validation("invalid email".into()));
    }
    let intent = body
        .intent
        .as_deref()
        .filter(|s| matches!(*s, "login" | "signup"))
        .unwrap_or("login")
        .to_string();
    // Generate a 128-bit token, base32 encoded — 26 chars, no padding.
    let raw1 = Uuid::new_v4().as_u128().to_be_bytes();
    let token = base32::encode(base32::Alphabet::Rfc4648 { padding: false }, &raw1);
    let token_hash = hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(MAGIC_LINK_TTL_MIN);
    sqlx::query(
        r#"
        INSERT INTO magic_links (email, token_hash, intent, requested_ip, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&email)
    .bind(&token_hash)
    .bind(&intent)
    .bind(&ip)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    // Send the email. To avoid enumeration we always respond 200, even when the send fails.
    let base_url = &state.config.base_url;
    let link = format!("{base_url}/auth/magic-link/consume?token={token}&intent={intent}");
    let html = format!(
        r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:auto;color:#1a1a2e;">
<h2>Ton lien Skilluv</h2>
<p>Connecte-toi en cliquant sur le bouton ci-dessous. Le lien expire dans {MAGIC_LINK_TTL_MIN} minutes.</p>
<p style="text-align:center;margin:30px 0;">
  <a href="{link}" style="background:#6c5ce7;color:white;padding:14px 28px;border-radius:8px;text-decoration:none;font-weight:bold;">Ouvrir Skilluv</a>
</p>
<p style="color:#666;font-size:12px;">Si tu n'as pas demandé ce lien, tu peux ignorer cet email.</p>
</div>"#
    );
    let _ = state
        .email
        // We deliberately don't have a user row here yet ; use a synthetic display name.
        .send_direct(&email, "Skilluv", "Skilluv — Ton lien de connexion", &html)
        .await;

    metrics::counter!("skilluv_magic_link_requested_total").increment(1);
    Ok(Json(build_response(json!({
        "sent": true,
        "expires_in_minutes": MAGIC_LINK_TTL_MIN,
    }))))
}

#[derive(Deserialize)]
struct ConsumeBody {
    token: String,
}

async fn consume_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ConsumeBody>,
) -> Result<impl IntoResponse, AppError> {
    let token_hash = hash_token(&body.token);
    let row: Option<MagicLinkRow139> = sqlx::query_as(
        "SELECT id, email, intent, expires_at, consumed_at FROM magic_links WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?;
    let (link_id, email, intent, expires_at, consumed_at) = row.ok_or(AppError::Unauthorized)?;
    if consumed_at.is_some() {
        return Err(AppError::Unauthorized);
    }
    if expires_at < chrono::Utc::now() {
        return Err(AppError::Unauthorized);
    }
    // Mark consumed atomically. If two consumers race, only one wins.
    let claim = sqlx::query(
        "UPDATE magic_links SET consumed_at = NOW() WHERE id = $1 AND consumed_at IS NULL",
    )
    .bind(link_id)
    .execute(&state.db)
    .await?;
    if claim.rows_affected() == 0 {
        return Err(AppError::Unauthorized);
    }

    // Resolve or create the user.
    let user: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT id, role, totp_enabled, email_2fa_enabled FROM users WHERE LOWER(email) = LOWER($1)",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;
    // Magic link cannot bypass 2FA — if the account has TOTP/email-2FA enabled,
    // the user must go through the classic password + 2FA flow. Otherwise anyone
    // with access to the mailbox would defeat 2FA.
    if let Some((_, _, totp_enabled, email_2fa_enabled)) = &user
        && (*totp_enabled || *email_2fa_enabled)
    {
        return Err(AppError::Validation(
            "This account uses two-factor authentication. Please sign in with your password."
                .to_string(),
        ));
    }
    let (user_id, role) = match user {
        Some((id, role, _, _)) => (id, role),
        None => {
            if intent != "signup" {
                return Err(AppError::Unauthorized);
            }
            let username_hint = email.split('@').next().unwrap_or("user").to_lowercase();
            let username: String = username_hint
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .take(24)
                .collect();
            let username = if username.len() < 3 {
                format!("user{}", &Uuid::new_v4().simple().to_string()[..6])
            } else {
                username
            };
            let display_name = username.clone();
            let placeholder = "$argon2id$v=19$m=19456,t=2,p=1$magic-placeholder$magic-placeholder";
            // Pattern C: skill_domain + terms captured later via /auth/complete-profile.
            let inserted: (Uuid,) = sqlx::query_as(
                r#"
                INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, skill_domain, email_verified)
                VALUES ($1, $2, $3, $4, '', $5, NULL, TRUE)
                RETURNING id
                "#,
            )
            .bind(&email)
            .bind(&username)
            .bind(placeholder)
            .bind(&display_name)
            .bind(&display_name)
            .fetch_one(&state.db)
            .await?;
            (inserted.0, "user".to_string())
        }
    };

    // Clicking the magic link is proof of email possession — flip email_verified
    // to true if it wasn't already. Without this, a candidate/enterprise who
    // signed up but never verified stays locked out of the write endpoints
    // and /enterprise/* even though they've now proven they own the address.
    sqlx::query("UPDATE users SET email_verified = TRUE WHERE id = $1 AND email_verified = FALSE")
        .bind(user_id)
        .execute(&state.db)
        .await?;

    // Label the session as magic_link so audit + downstream gates can tell it
    // apart from a password login.
    let access = AuthService::generate_access_token_with_method(
        user_id,
        &role,
        "magic_link",
        &state.config.jwt_secret,
    )?;
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let ip = crate::middleware::extract_ip(&headers);
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user_id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh) =
        SessionService::create_with_method(&state.db, user_id, Some(&ip), ua, "magic_link").await?;
    let cookie = build_cookie("access_token", &access, 15 * 60, "/");
    let refresh_cookie = format!(
        "refresh_token={session_id}:{refresh}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
        7 * 24 * 60 * 60
    );
    metrics::counter!("skilluv_magic_link_consumed_total").increment(1);
    Ok((
        AppendHeaders([(SET_COOKIE, cookie), (SET_COOKIE, refresh_cookie)]),
        Json(build_response(json!({
            "user_id": user_id,
            "login_method": "magic_link",
        }))),
    ))
}
