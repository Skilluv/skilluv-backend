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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
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

fn build_cookie(name: &str, value: &str, max_age_secs: i64, path: &str) -> String {
    format!("{name}={value}; HttpOnly; Secure; SameSite=Lax; Path={path}; Max-Age={max_age_secs}")
}

fn hash_token(token: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    h.finalize().to_vec()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MagicLinkRequestBody {
    #[schema(
        format = "email",
        min_length = 5,
        max_length = 255,
        example = "user@example.com"
    )]
    pub email: String,
    /// `"login"` (default) or `"signup"`. A signup-intent link creates
    /// the user on consumption if no account matches the email.
    #[schema(pattern = r"^(login|signup)$", example = "login")]
    pub intent: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MagicLinkRequestResponse {
    /// Always `true` on success — the endpoint never leaks whether the
    /// email exists, so this flag is symbolic (200 = "we attempted").
    pub sent: bool,
    /// TTL of the emailed link, in minutes.
    pub expires_in_minutes: i64,
}

/// Request a passwordless login link by email. Rate-limited to 5/min
/// per IP. Always returns 200 (anti-enumeration) even when the email
/// send fails silently — the front should not branch on this response
/// beyond confirming the user to check their inbox.
#[utoipa::path(
    post,
    path = "/api/auth/magic-link/request",
    tag = "auth",
    request_body = MagicLinkRequestBody,
    responses(
        (status = 200, description = "Link email attempted", body = ApiResponse<MagicLinkRequestResponse>),
        (status = 400, description = "Invalid email", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Rate limit hit", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn request_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MagicLinkRequestBody>,
) -> Result<Json<ApiResponse<MagicLinkRequestResponse>>, AppError> {
    let ip = extract_ip(&headers);
    RateLimiter::check(&mut state.redis.clone(), "magic_link", &ip, 5, 60).await?;
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 || email.len() > 255 {
        return Err(AppError::Validation("invalid email".into()));
    }
    // Reject invalid intent explicitement — le schema declare
    // pattern ^(login|signup)$, donc tout autre string est schema-invalide.
    // Anciennement on faisait un fallback silencieux sur "login" mais
    // schemathesis negative_data_rejection flaggait car un input schema-
    // invalide doit etre rejete (4xx), pas accepte.
    let intent = match body.intent.as_deref() {
        None => "login".to_string(),
        Some(s) if matches!(s, "login" | "signup") => s.to_string(),
        Some(_) => {
            return Err(AppError::Validation(
                "intent must be one of: login, signup".into(),
            ));
        }
    };
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
    Ok(Json(ApiResponse::new(MagicLinkRequestResponse {
        sent: true,
        expires_in_minutes: MAGIC_LINK_TTL_MIN,
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MagicLinkConsumeBody {
    /// Raw 26-char base-32 token from the emailed link.
    #[schema(min_length = 20, max_length = 40, pattern = r"^[A-Z2-7]+$")]
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MagicLinkConsumeResponse {
    pub user_id: Uuid,
    /// Always `"magic_link"` — echoed so the front can tag the session
    /// with the auth method without decoding the JWT.
    pub login_method: String,
}

/// Consume a magic-link token: rotates cookies and mints a session
/// labeled `magic_link`. Refuses accounts that have TOTP or email 2FA
/// enabled — those must go through the classic password flow so the
/// second factor is not bypassed. Creates the user on the fly when the
/// original intent was `signup` and no matching account exists.
#[utoipa::path(
    post,
    path = "/api/auth/magic-link/consume",
    tag = "auth",
    request_body = MagicLinkConsumeBody,
    responses(
        (status = 200, description = "Session issued via magic link", body = ApiResponse<MagicLinkConsumeResponse>),
        (status = 400, description = "Account has 2FA — magic link refused", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Token invalid, expired, or already consumed", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn consume_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MagicLinkConsumeBody>,
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
        Json(ApiResponse::new(MagicLinkConsumeResponse {
            user_id,
            login_method: "magic_link".to_string(),
        })),
    ))
}
