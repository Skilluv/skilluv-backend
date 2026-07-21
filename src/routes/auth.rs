use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use totp_rs::{Algorithm, Secret, TOTP};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{
    AuthUser, RateLimiter, build_csrf_cookie, build_csrf_cookie_with_prefix, extract_ip,
    generate_csrf_token,
};
use crate::models::{User, UserPrivate};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::audit::{self, ActorType, AuditEntry};
use crate::services::{AuthService, LeaderboardService, SessionService};

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        // Public
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/verify-email", get(verify_email))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password", post(reset_password))
        // Authenticated
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/auth/change-password", post(change_password))
        .route("/auth/change-email", post(request_email_change))
        .route("/auth/change-email/confirm", get(confirm_email_change))
        .route("/auth/complete-profile", post(complete_profile))
        .route("/auth/resend-verification", post(resend_verification))
        // TOTP 2FA
        .route("/auth/totp/setup", post(totp_setup))
        .route("/auth/totp/enable", post(totp_enable))
        .route("/auth/totp/disable", post(totp_disable))
        .route(
            "/auth/totp/backup-codes/regenerate",
            post(regenerate_backup_codes),
        )
        // Email 2FA
        .route("/auth/email-2fa/enable", post(email_2fa_enable))
        .route("/auth/email-2fa/disable", post(email_2fa_disable))
        .route("/auth/email-2fa/verify", post(email_2fa_verify))
        // Sessions / device management
        .route("/auth/sessions", get(list_sessions))
        .route("/auth/sessions/{id}", delete(revoke_session))
        .route("/auth/sessions/revoke-all", post(revoke_all_other_sessions))
        // Account deletion (RGPD)
        .route("/auth/account", delete(delete_account))
        // RGPD data export
        .route("/auth/me/data-export", post(request_data_export))
}

/// POST /api/auth/me/data-export — rate-limited 1/24h per user, spawns background task.
async fn request_data_export(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = state.redis.clone();
    let key = format!("rate:data_export:{}", auth.user_id);
    let exists: bool = redis::cmd("EXISTS")
        .arg(&key)
        .query_async::<i64>(&mut redis)
        .await?
        == 1;
    if exists {
        return Err(AppError::Validation(
            "Data export already requested in the last 24h. Check your email.".into(),
        ));
    }
    let () = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("EX")
        .arg(24 * 3600)
        .query_async(&mut redis)
        .await?;

    let db = state.db.clone();
    let storage = state.storage.clone();
    let email = state.email.clone();
    let user_id = auth.user_id;
    tokio::spawn(async move {
        match crate::services::data_export::generate_export(db, storage, email, user_id).await {
            Ok(artifact) => {
                tracing::info!(%user_id, key = %artifact.key, "data export delivered");
            }
            Err(err) => {
                tracing::error!(%user_id, error = %err, "data export failed");
                sentry::capture_error(&err);
            }
        }
    });

    Ok(Json(build_response(json!({
        "status": "queued",
        "message": "Your archive is being prepared. You'll receive it by email within a few minutes."
    }))))
}

// ─── Request types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    email: String,
    username: String,
    password: String,
    first_name: String,
    last_name: String,
    skill_domain: String,
    country: Option<String>,
    city: Option<String>,
    /// Must be true — user acknowledges Terms of Service and Privacy Policy.
    #[serde(default)]
    terms_accepted: bool,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    /// Email or username
    identifier: String,
    password: String,
    totp_code: Option<String>,
    email_2fa_code: Option<String>,
    /// One-time TOTP backup code (used when the user lost their authenticator).
    backup_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerifyEmailQuery {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ForgotPasswordRequest {
    email: String,
}

#[derive(Debug, Deserialize)]
struct ResetPasswordRequest {
    token: String,
    new_password: String,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

#[derive(Debug, Deserialize)]
struct ChangeEmailRequest {
    current_password: String,
    new_email: String,
}

#[derive(Debug, Deserialize)]
struct CompleteProfileRequest {
    skill_domain: String,
    #[serde(default)]
    terms_accepted: bool,
    country: Option<String>,
    city: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfirmEmailChangeQuery {
    token: String,
}

#[derive(Debug, Deserialize)]
struct TotpCodeRequest {
    code: String,
}

#[derive(Debug, Deserialize)]
struct DeleteAccountRequest {
    password: String,
    totp_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Email2faVerifyRequest {
    code: String,
    /// Needed for login flow (user not yet authenticated)
    user_id: Option<Uuid>,
}

// ─── Validation helpers ──────────────────────────────────────────

pub fn validate_email(email: &str) -> Result<(), AppError> {
    if !email.contains('@') || email.len() < 5 || email.len() > 255 {
        return Err(AppError::Validation("Invalid email address".to_string()));
    }
    Ok(())
}

/// Public wrapper for the strict password policy so sibling route modules
/// (enterprise register, admin flows) can reuse it without duplicating the
/// rules.
pub fn validate_password_pub(password: &str) -> Result<(), AppError> {
    validate_password(password)
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 10 {
        return Err(AppError::Validation(
            "Password must be at least 10 characters".to_string(),
        ));
    }
    if password.len() > 128 {
        return Err(AppError::Validation(
            "Password must be at most 128 characters".to_string(),
        ));
    }
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_symbol = password
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !c.is_whitespace());
    if !(has_upper && has_lower && has_digit && has_symbol) {
        return Err(AppError::Validation(
            "Password must contain at least one uppercase, one lowercase, one digit and one symbol"
                .to_string(),
        ));
    }
    Ok(())
}

pub fn validate_username(username: &str) -> Result<(), AppError> {
    if username.len() < 3 || username.len() > 30 {
        return Err(AppError::Validation(
            "Username must be between 3 and 30 characters".to_string(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "Username can only contain letters, numbers, underscores and hyphens".to_string(),
        ));
    }
    if username.starts_with('-') || username.starts_with('_') {
        return Err(AppError::Validation(
            "Username must start with a letter or number".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_name(name: &str, field: &str) -> Result<(), AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 50 {
        return Err(AppError::Validation(format!(
            "{field} must be between 1 and 50 characters"
        )));
    }
    Ok(())
}

fn validate_skill_domain(domain: &str) -> Result<(), AppError> {
    match domain {
        "code" | "design" | "game" | "security" => Ok(()),
        _ => Err(AppError::Validation(
            "skill_domain must be one of: code, design, game, security".to_string(),
        )),
    }
}

fn build_cookie(name: &str, value: &str, max_age_secs: i64, path: &str) -> String {
    format!(
        "{name}={value}; HttpOnly; Secure; SameSite=Strict; Path={path}; Max-Age={max_age_secs}"
    )
}

fn clear_cookie(name: &str, path: &str) -> String {
    format!("{name}=; HttpOnly; Secure; SameSite=Strict; Path={path}; Max-Age=0")
}

const REFRESH_COOKIE_PATH: &str = "/api/auth";
const REFRESH_COOKIE_MAX_AGE: i64 = 7 * 24 * 60 * 60;

/// True when the incoming request originated from the admin frontend (dev
/// server on :5174 or `admin.*` in prod). Login handlers use this to emit
/// admin-prefixed cookies so an admin session on `admin.skilluv.com` and a
/// candidate session on `skilluv.com` can coexist in the same browser cookie
/// jar without stepping on each other. The `AuthUser` extractor accepts
/// either prefix, so downstream endpoints don't have to care.
pub fn is_admin_origin(headers: &axum::http::HeaderMap) -> bool {
    let origin = headers
        .get("origin")
        .or_else(|| headers.get("referer"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if origin.is_empty() {
        return false;
    }
    // Match dev (`http://localhost:5174`) and prod (`https://admin.…`) alike.
    origin.contains("://admin.")
        || origin.starts_with("http://localhost:5174")
        || origin.starts_with("http://127.0.0.1:5174")
        || std::env::var("ADMIN_ORIGINS")
            .ok()
            .map(|list| {
                list.split(',')
                    .map(str::trim)
                    .any(|allowed| !allowed.is_empty() && origin.starts_with(allowed))
            })
            .unwrap_or(false)
}

/// Cookie name prefix bound to the caller's frontend. `""` for the public app,
/// `"admin_"` for the admin app. Kept as a helper so every login handler
/// converges on the same rule without duplicating origin parsing.
pub fn cookie_prefix(headers: &axum::http::HeaderMap) -> &'static str {
    if is_admin_origin(headers) {
        "admin_"
    } else {
        ""
    }
}

/// Refresh cookie encodes `{session_id}:{opaque_token}`. The server verifies the token against
/// the SHA-256 stored in `user_sessions.refresh_hash`. The `prefix` picks
/// between the public (`refresh_token`) and admin (`admin_refresh_token`)
/// cookie namespace.
fn build_refresh_cookie_with_prefix(prefix: &str, session_id: Uuid, token: &str) -> String {
    let value = format!("{session_id}:{token}");
    build_cookie(
        &format!("{prefix}refresh_token"),
        &value,
        REFRESH_COOKIE_MAX_AGE,
        REFRESH_COOKIE_PATH,
    )
}

/// Back-compat shorthand used by the SSO/OAuth/magic-link handlers that haven't
/// been migrated yet — they always emit public cookies.
fn build_refresh_cookie(session_id: Uuid, token: &str) -> String {
    build_refresh_cookie_with_prefix("", session_id, token)
}

fn parse_refresh_cookie(headers: &axum::http::HeaderMap) -> Option<(Uuid, String)> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    // Prefer admin_ when present — an admin session should be the one we
    // revoke/rotate when both cookies happen to live in the same jar (rare
    // but possible: dev with two frontends open, or a user who logged in on
    // both apps intentionally). The AuthUser extractor uses the same rule.
    let val = raw
        .split(';')
        .map(|s| s.trim())
        .find_map(|s| s.strip_prefix("admin_refresh_token="))
        .or_else(|| {
            raw.split(';')
                .map(|s| s.trim())
                .find_map(|s| s.strip_prefix("refresh_token="))
        })?;
    let (sid_str, token) = val.split_once(':')?;
    let sid = sid_str.parse::<Uuid>().ok()?;
    if token.is_empty() {
        return None;
    }
    Some((sid, token.to_string()))
}

fn extract_ua(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers.get("user-agent").and_then(|v| v.to_str().ok())
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).replace('-', "")
}

fn generate_6digit_code() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:06}", seed % 1_000_000)
}

// ─── Redis key helpers ───────────────────────────────────────────

fn email_verify_key(token: &str) -> String {
    format!("email_verify:{token}")
}

fn password_reset_key(token: &str) -> String {
    format!("password_reset:{token}")
}

fn email_2fa_key(user_id: Uuid) -> String {
    format!("email_2fa:{user_id}")
}

fn login_pending_2fa_key(user_id: Uuid) -> String {
    format!("login_pending_2fa:{user_id}")
}

// ─── Routes ──────────────────────────────────────────────────────

// POST /api/auth/register
async fn register(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ip = extract_ip(&headers);
    RateLimiter::check(&mut state.redis.clone(), "auth:register", &ip, 5, 3600).await?;

    if !body.terms_accepted {
        return Err(AppError::Validation(
            "You must accept the Terms of Service and Privacy Policy".to_string(),
        ));
    }

    validate_email(&body.email)?;
    validate_username(&body.username)?;
    validate_password(&body.password)?;
    validate_name(&body.first_name, "first_name")?;
    validate_name(&body.last_name, "last_name")?;
    validate_skill_domain(&body.skill_domain)?;

    let email_lower = body.email.trim().to_lowercase();
    let username_lower = body.username.trim().to_lowercase();

    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1 OR username = $2")
            .bind(&email_lower)
            .bind(&username_lower)
            .fetch_optional(&state.db)
            .await?;

    if existing.is_some() {
        return Err(AppError::Validation(
            "An account with this email or username already exists".to_string(),
        ));
    }

    let password_hash = AuthService::hash_password(&body.password)?;
    let display_name = format!("{} {}", body.first_name.trim(), body.last_name.trim());

    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, skill_domain, country, city, terms_accepted_at, password_changed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&email_lower)
    .bind(&username_lower)
    .bind(&password_hash)
    .bind(body.first_name.trim())
    .bind(body.last_name.trim())
    .bind(&display_name)
    .bind(&body.skill_domain)
    .bind(&body.country)
    .bind(body.city.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .fetch_one(&state.db)
    .await?;

    // Send email verification
    let verify_token = generate_token();
    let mut redis = state.redis.clone();
    let key = email_verify_key(&verify_token);
    let () = redis
        .set_ex(&key, user.id.to_string(), 24 * 60 * 60) // 24h
        .await?;

    state
        .email
        .send_email_verification(
            &user.email,
            &user.display_name,
            &verify_token,
            &state.config.base_url,
        )
        .await?;

    // Generate tokens
    let access_token =
        AuthService::generate_access_token(user.id, &user.role, &state.config.jwt_secret)?;
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user.id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh_token) =
        SessionService::create(&state.db, user.id, Some(&ip), extract_ua(&headers)).await?;

    if analytics_consent(&headers) {
        state.analytics.track(
            user.id,
            events::USER_SIGNUP,
            props(&[
                ("skill_domain", json!(user.skill_domain)),
                ("country", json!(user.country)),
                ("city", json!(user.city)),
            ]),
        );
    }
    // Register always sets skill_domain; unwrap_or fallback is defensive only.
    metrics::counter!(
        "skilluv_signups_total",
        "skill_domain" => user.skill_domain.clone().unwrap_or_else(|| "unknown".to_string())
    )
    .increment(1);

    audit::record(
        &state.db,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "user.signup",
            target_type: Some("user"),
            target_id: Some(user.id),
            metadata: Some(json!({ "skill_domain": user.skill_domain })),
            headers: Some(&headers),
        },
    )
    .await;

    let user_private: UserPrivate = user.into();
    let access_cookie = build_cookie("access_token", &access_token, 15 * 60, "/");
    let refresh_cookie = build_refresh_cookie(session_id, &refresh_token);
    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie(&csrf, "/api", 15 * 60);

    Ok((
        StatusCode::CREATED,
        AppendHeaders([
            (SET_COOKIE, access_cookie),
            (SET_COOKIE, refresh_cookie),
            (SET_COOKIE, csrf_cookie),
        ]),
        Json(build_response(json!({
            "user": user_private,
            "csrf_token": csrf,
            "login_method": "password",
            "message": "Account created. Please verify your email."
        }))),
    ))
}

const LOGIN_LOCKOUT_THRESHOLD: i32 = 5;
const LOGIN_LOCKOUT_MINUTES: i64 = 15;

// POST /api/auth/login
async fn login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ip = extract_ip(&headers);
    RateLimiter::check(&mut state.redis.clone(), "auth:login", &ip, 20, 60).await?;

    let identifier = body.identifier.trim().to_lowercase();

    let user: User = sqlx::query_as("SELECT * FROM users WHERE email = $1 OR username = $1")
        .bind(&identifier)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::InvalidCredentials)?;

    if user.is_banned {
        return Err(AppError::Forbidden);
    }

    // Enforced SSO: if the user's email domain matches an active SSO config with
    // enforce_sso=true, refuse the password login and hand back the SSO start URL.
    if let Some(domain) = user.email.split('@').nth(1).map(str::to_lowercase) {
        if let Some((cfg, slug)) =
            crate::services::enterprise_sso::find_by_email_domain(&state.db, &domain).await?
        {
            if cfg.enforce_sso {
                let start_url = format!(
                    "{}/api/enterprise/sso/{}/start",
                    state.config.base_url, slug
                );
                return Err(AppError::SsoRequired { start_url });
            }
        }
    }

    // Per-account lockout: if the account is currently locked, refuse.
    if let Some(locked_until) = user.locked_until
        && locked_until > chrono::Utc::now()
    {
        return Err(AppError::Validation(format!(
            "Account temporarily locked due to too many failed attempts. Try again in {} minutes.",
            (locked_until - chrono::Utc::now()).num_minutes().max(1)
        )));
    }

    let valid = AuthService::verify_password(&body.password, &user.password_hash)?;
    if !valid {
        // Increment failure counter and lock account if threshold reached.
        let new_count = user.failed_login_count + 1;
        if new_count >= LOGIN_LOCKOUT_THRESHOLD {
            let until = chrono::Utc::now() + chrono::Duration::minutes(LOGIN_LOCKOUT_MINUTES);
            sqlx::query(
                "UPDATE users SET failed_login_count = $1, locked_until = $2, updated_at = NOW() WHERE id = $3",
            )
            .bind(new_count)
            .bind(until)
            .bind(user.id)
            .execute(&state.db)
            .await?;
            tracing::warn!(user_id = %user.id, "Account locked after {} failed logins", new_count);
        } else {
            sqlx::query(
                "UPDATE users SET failed_login_count = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(new_count)
            .bind(user.id)
            .execute(&state.db)
            .await?;
        }
        return Err(AppError::InvalidCredentials);
    }

    // TOTP 2FA check — accepts either a live TOTP code OR a one-time backup code.
    if user.totp_enabled {
        if let Some(code) = body.totp_code.as_deref() {
            let secret = user
                .totp_secret
                .as_ref()
                .ok_or(AppError::Internal("TOTP enabled but no secret".to_string()))?;
            let totp = build_totp(secret, &user.email)?;
            if !totp
                .check_current(code)
                .map_err(|e| AppError::Internal(format!("TOTP check failed: {e}")))?
            {
                return Err(AppError::TotpInvalid);
            }
        } else if let Some(backup) = body.backup_code.as_deref() {
            consume_backup_code(&state.db, user.id, backup).await?;
            let _ = state
                .email
                .send_security_alert(
                    &user.email,
                    &user.display_name,
                    "Code de secours utilisé",
                    "Un code de secours TOTP a été utilisé pour te connecter. Si ce n'est pas toi, sécurise ton compte immédiatement.",
                )
                .await;
        } else {
            return Err(AppError::TotpRequired);
        }
    }

    // Email 2FA check
    if user.email_2fa_enabled {
        if let Some(code) = body.email_2fa_code.as_deref() {
            // Verify the code
            let mut redis = state.redis.clone();
            let key = email_2fa_key(user.id);
            let stored: Option<String> = redis.get(&key).await?;
            match stored {
                Some(stored_code) if stored_code == code => {
                    let () = redis.del(&key).await?;
                    // Also clear the pending login flag
                    let pending_key = login_pending_2fa_key(user.id);
                    let () = redis.del(&pending_key).await?;
                }
                _ => return Err(AppError::Email2faInvalid),
            }
        } else {
            // Send 2FA code by email and return pending status
            let code = generate_6digit_code();
            let mut redis = state.redis.clone();
            let key = email_2fa_key(user.id);
            let () = redis.set_ex(&key, &code, 10 * 60).await?; // 10 min

            // Store a flag that this user has a pending 2FA
            let pending_key = login_pending_2fa_key(user.id);
            let () = redis.set_ex(&pending_key, "1", 10 * 60).await?;

            state
                .email
                .send_email_2fa_code(&user.email, &user.display_name, &code)
                .await?;

            return Ok((
                AppendHeaders([
                    (SET_COOKIE, String::new()),
                    (SET_COOKIE, String::new()),
                    (SET_COOKIE, String::new()),
                ]),
                Json(build_response(json!({
                    "requires_email_2fa": true,
                    "user_id": user.id,
                    "message": "A verification code has been sent to your email"
                }))),
            ));
        }
    }

    // Successful password (and 2FA if any) — reset the failure counter and lock.
    if user.failed_login_count > 0 || user.locked_until.is_some() {
        sqlx::query(
            "UPDATE users SET failed_login_count = 0, locked_until = NULL, updated_at = NOW() WHERE id = $1",
        )
        .bind(user.id)
        .execute(&state.db)
        .await?;
    }

    let access_token =
        AuthService::generate_access_token(user.id, &user.role, &state.config.jwt_secret)?;
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user.id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh_token) =
        SessionService::create(&state.db, user.id, Some(&ip), extract_ua(&headers)).await?;

    let auth_method = if user.totp_enabled || user.email_2fa_enabled {
        "2fa"
    } else {
        "password"
    };
    if analytics_consent(&headers) {
        state.analytics.track(
            user.id,
            events::USER_LOGIN,
            props(&[("method", json!(auth_method))]),
        );
    }

    audit::record(
        &state.db,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "user.login",
            target_type: Some("user"),
            target_id: Some(user.id),
            metadata: Some(json!({ "method": auth_method })),
            headers: Some(&headers),
        },
    )
    .await;

    // Enterprise/recruiter accounts need SOME strong 2FA method — TOTP or a
    // passkey. If neither is present, the frontend routes them into the
    // /enterprise/onboarding wizard where they pick and complete one.
    let has_passkey: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM webauthn_credentials WHERE user_id = $1)")
            .bind(user.id)
            .fetch_one(&state.db)
            .await?;
    // BE-A : admin doit aussi configurer un second facteur (TOTP ou passkey).
    // Le login réussit pour permettre l'accès à /auth/setup-2fa ; les routes
    // /api/admin/* sont bloquées par le middleware `require_admin_2fa` tant
    // que le facteur n'est pas actif.
    let requires_totp_setup = matches!(user.role.as_str(), "enterprise" | "recruiter" | "admin")
        && !user.totp_enabled
        && !has_passkey;
    let user_private: UserPrivate = user.into();
    // Origin-aware cookie namespace — admin.skilluv.com → admin_* cookies,
    // everything else → the standard names.
    let prefix = cookie_prefix(&headers);
    let access_cookie = build_cookie(
        &format!("{prefix}access_token"),
        &access_token,
        15 * 60,
        "/",
    );
    let refresh_cookie = build_refresh_cookie_with_prefix(prefix, session_id, &refresh_token);
    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie_with_prefix(prefix, &csrf, "/api", 15 * 60);

    Ok((
        AppendHeaders([
            (SET_COOKIE, access_cookie),
            (SET_COOKIE, refresh_cookie),
            (SET_COOKIE, csrf_cookie),
        ]),
        Json(build_response(json!({
            "user": user_private,
            "csrf_token": csrf,
            "login_method": "password",
            "has_passkey": has_passkey,
            "requires_totp_setup": requires_totp_setup,
        }))),
    ))
}

// POST /api/auth/email-2fa/verify — complete login after email 2FA
async fn email_2fa_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Email2faVerifyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = body.user_id.ok_or(AppError::Validation(
        "user_id is required for email 2FA verification".to_string(),
    ))?;

    // Check there's a pending 2FA
    let mut redis = state.redis.clone();
    let pending_key = login_pending_2fa_key(user_id);
    let pending: Option<String> = redis.get(&pending_key).await?;
    if pending.is_none() {
        return Err(AppError::Validation(
            "No pending 2FA for this user".to_string(),
        ));
    }

    // Verify code
    let key = email_2fa_key(user_id);
    let stored: Option<String> = redis.get(&key).await?;
    match stored {
        Some(stored_code) if stored_code == body.code => {
            let () = redis.del(&key).await?;
            let () = redis.del(&pending_key).await?;
        }
        _ => return Err(AppError::Email2faInvalid),
    }

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.is_banned {
        return Err(AppError::Forbidden);
    }

    let ip = extract_ip(&headers);
    let access_token =
        AuthService::generate_access_token(user.id, &user.role, &state.config.jwt_secret)?;
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user.id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh_token) =
        SessionService::create(&state.db, user.id, Some(&ip), extract_ua(&headers)).await?;

    // Same 2FA-satisfaction check as the password login handler above:
    // enterprise/recruiter needs TOTP OR a passkey — either counts.
    let has_passkey: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM webauthn_credentials WHERE user_id = $1)")
            .bind(user.id)
            .fetch_one(&state.db)
            .await?;
    // BE-A : admin doit aussi configurer un second facteur (TOTP ou passkey).
    // Le login réussit pour permettre l'accès à /auth/setup-2fa ; les routes
    // /api/admin/* sont bloquées par le middleware `require_admin_2fa` tant
    // que le facteur n'est pas actif.
    let requires_totp_setup = matches!(user.role.as_str(), "enterprise" | "recruiter" | "admin")
        && !user.totp_enabled
        && !has_passkey;
    let user_private: UserPrivate = user.into();
    let access_cookie = build_cookie("access_token", &access_token, 15 * 60, "/");
    let refresh_cookie = build_refresh_cookie(session_id, &refresh_token);
    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie(&csrf, "/api", 15 * 60);

    Ok((
        AppendHeaders([
            (SET_COOKIE, access_cookie),
            (SET_COOKIE, refresh_cookie),
            (SET_COOKIE, csrf_cookie),
        ]),
        Json(build_response(json!({
            "user": user_private,
            "csrf_token": csrf,
            "login_method": "password",
            "has_passkey": has_passkey,
            "requires_totp_setup": requires_totp_setup,
        }))),
    ))
}

// POST /api/auth/refresh — refresh token read from httpOnly cookie
async fn refresh(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let (session_id, token) = parse_refresh_cookie(&headers).ok_or(AppError::Unauthorized)?;

    let (user_id, new_refresh_token) =
        SessionService::rotate(&state.db, session_id, &token).await?;

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.is_banned {
        return Err(AppError::Forbidden);
    }

    // Preserve the session's original login_method across refresh so the JWT
    // claim stays faithful — otherwise a passkey / SSO / magic-link session
    // would silently downgrade to "password" on every refresh, losing the
    // enterprise TOTP-bypass semantics.
    let login_method: (String,) =
        sqlx::query_as("SELECT login_method FROM user_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&state.db)
            .await?;
    let access_token = AuthService::generate_access_token_with_method(
        user.id,
        &user.role,
        &login_method.0,
        &state.config.jwt_secret,
    )?;

    // Preserve the caller's namespace on rotate — if they refreshed via the
    // admin app we want to re-emit `admin_*` cookies so the SPA doesn't lose
    // its handle mid-session.
    let prefix = cookie_prefix(&headers);
    let access_cookie = build_cookie(
        &format!("{prefix}access_token"),
        &access_token,
        15 * 60,
        "/",
    );
    let refresh_cookie = build_refresh_cookie_with_prefix(prefix, session_id, &new_refresh_token);
    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie_with_prefix(prefix, &csrf, "/api", 15 * 60);

    Ok((
        AppendHeaders([
            (SET_COOKIE, access_cookie),
            (SET_COOKIE, refresh_cookie),
            (SET_COOKIE, csrf_cookie),
        ]),
        Json(build_response(json!({
            "ok": true,
            "csrf_token": csrf,
            "login_method": login_method.0,
        }))),
    ))
}

// POST /api/auth/logout
//
// Deliberately does NOT require a valid `AuthUser`: an expired access_token
// would otherwise 401 before we reach the revocation code, leaving the DB
// row orphaned even though the client considers itself logged out. The
// refresh_token cookie carries a `session_id` we can trust structurally
// (uuid + opaque token) — we look the row up ourselves and revoke it,
// regardless of whether the JWT is still valid.
async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if let Some((session_id, _)) = parse_refresh_cookie(&headers) {
        // Look up the owning user_id from the session row itself so we don't
        // need the JWT. `revoke_one` filters on user_id, so we must supply
        // it — but no need to trust anything the client sent.
        let owner: Option<(Uuid,)> = sqlx::query_as(
            "SELECT user_id FROM user_sessions WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(&state.db)
        .await?;
        if let Some((user_id,)) = owner {
            SessionService::revoke_one(&state.db, user_id, session_id).await?;
        }
    }

    // Clear BOTH cookie namespaces on logout — we don't know which app the
    // caller signed in on (or if both were set in the jar for whatever
    // reason), and leaving one orphaned would let a stale token linger.
    let clear_access = clear_cookie("access_token", "/");
    let clear_refresh = clear_cookie("refresh_token", REFRESH_COOKIE_PATH);
    let clear_csrf = "csrf_token=; Secure; SameSite=Strict; Path=/api; Max-Age=0".to_string();
    let clear_admin_access = clear_cookie("admin_access_token", "/");
    let clear_admin_refresh = clear_cookie("admin_refresh_token", REFRESH_COOKIE_PATH);
    let clear_admin_csrf =
        "admin_csrf_token=; Secure; SameSite=Strict; Path=/api; Max-Age=0".to_string();

    Ok((
        AppendHeaders([
            (SET_COOKIE, clear_access),
            (SET_COOKIE, clear_refresh),
            (SET_COOKIE, clear_csrf),
            (SET_COOKIE, clear_admin_access),
            (SET_COOKIE, clear_admin_refresh),
            (SET_COOKIE, clear_admin_csrf),
        ]),
        Json(build_response(json!({
            "message": "Logged out successfully"
        }))),
    ))
}

// GET /api/auth/me
async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    let skill_domain = user.skill_domain.clone();
    let user_private: UserPrivate = user.into();

    // Any strong-factor enrolment satisfies the enterprise 2FA gate, so the
    // frontend needs to know whether a passkey exists alongside TOTP.
    let has_passkey: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM webauthn_credentials WHERE user_id = $1)")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;

    // Fetch ranks from Redis. Users still onboarding (no skill_domain) get null domain rank.
    let mut redis = state.redis.clone();
    let global_rank =
        LeaderboardService::get_rank(&mut redis, "global", "alltime", auth.user_id).await?;
    let domain_rank = match skill_domain.as_deref() {
        Some(d) => LeaderboardService::get_rank(&mut redis, d, "alltime", auth.user_id).await?,
        None => None,
    };

    Ok(Json(build_response(json!({
        "user": user_private,
        // Surfaced so the frontend can decide policy without decoding the JWT
        // (e.g. skipping the enterprise TOTP redirect for `sso` / `webauthn`).
        "login_method": auth.login_method,
        "has_passkey": has_passkey,
        "rank": {
            "global": global_rank,
            "domain": domain_rank,
        }
    }))))
}

// GET /api/auth/verify-email?token=xxx
async fn verify_email(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = state.redis.clone();
    let key = email_verify_key(&query.token);
    let user_id_str: Option<String> = redis.get(&key).await?;

    let user_id_str = user_id_str.ok_or(AppError::Validation(
        "Invalid or expired verification token".to_string(),
    ))?;

    let user_id: Uuid = user_id_str
        .parse()
        .map_err(|_| AppError::Internal("Invalid user_id in token".to_string()))?;

    sqlx::query("UPDATE users SET email_verified = TRUE, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;

    let () = redis.del(&key).await?;

    Ok(Json(build_response(json!({
        "message": "Email verified successfully"
    }))))
}

// POST /api/auth/resend-verification
async fn resend_verification(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if user.email_verified {
        return Err(AppError::Validation(
            "Email is already verified".to_string(),
        ));
    }

    let verify_token = generate_token();
    let mut redis = state.redis.clone();
    let key = email_verify_key(&verify_token);
    let () = redis
        .set_ex(&key, user.id.to_string(), 24 * 60 * 60)
        .await?;

    state
        .email
        .send_email_verification(
            &user.email,
            &user.display_name,
            &verify_token,
            &state.config.base_url,
        )
        .await?;

    Ok(Json(build_response(json!({
        "message": "Verification email sent"
    }))))
}

// POST /api/auth/forgot-password
async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Always return success to prevent email enumeration
    let response = build_response(json!({
        "message": "If an account exists with this email, a reset link has been sent"
    }));

    let email = body.email.trim().to_lowercase();
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?;

    if let Some(user) = user {
        let token = generate_token();
        let mut redis = state.redis.clone();
        let key = password_reset_key(&token);
        let () = redis
            .set_ex(&key, user.id.to_string(), 60 * 60) // 1h
            .await?;

        state
            .email
            .send_password_reset(
                &user.email,
                &user.display_name,
                &token,
                &state.config.base_url,
            )
            .await?;
    }

    Ok(Json(response))
}

// POST /api/auth/reset-password
async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_password(&body.new_password)?;

    let mut redis = state.redis.clone();
    let key = password_reset_key(&body.token);
    let user_id_str: Option<String> = redis.get(&key).await?;

    let user_id_str = user_id_str.ok_or(AppError::Validation(
        "Invalid or expired reset token".to_string(),
    ))?;

    let user_id: Uuid = user_id_str
        .parse()
        .map_err(|_| AppError::Internal("Invalid user_id in token".to_string()))?;

    let password_hash = AuthService::hash_password(&body.new_password)?;

    sqlx::query(
        "UPDATE users SET password_hash = $1, password_changed_at = NOW(), failed_login_count = 0, locked_until = NULL, updated_at = NOW() WHERE id = $2",
    )
    .bind(&password_hash)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    // Invalidate token
    let () = redis.del(&key).await?;

    // Revoke all sessions (all devices signed out)
    SessionService::revoke_all(&state.db, user_id).await?;

    if let Ok(Some(u)) =
        sqlx::query_as::<_, (String, String)>("SELECT email, display_name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
    {
        let _ = state
            .email
            .send_security_alert(
                &u.0,
                &u.1,
                "Ton mot de passe a été réinitialisé",
                "Un nouveau mot de passe a été défini via le lien de réinitialisation.",
            )
            .await;
    }

    Ok(Json(build_response(json!({
        "message": "Password reset successfully. Please log in with your new password."
    }))))
}

// POST /api/auth/change-password
async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_password(&body.new_password)?;

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    let valid = AuthService::verify_password(&body.current_password, &user.password_hash)?;
    if !valid {
        return Err(AppError::InvalidCredentials);
    }

    let password_hash = AuthService::hash_password(&body.new_password)?;

    sqlx::query(
        "UPDATE users SET password_hash = $1, password_changed_at = NOW(), updated_at = NOW() WHERE id = $2",
    )
    .bind(&password_hash)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    let _ = state
        .email
        .send_security_alert(
            &user.email,
            &user.display_name,
            "Ton mot de passe a été modifié",
            "Le mot de passe de ton compte Skilluv vient d'être changé.",
        )
        .await;

    Ok(Json(build_response(json!({
        "message": "Password changed successfully"
    }))))
}

// ─── TOTP 2FA ────────────────────────────────────────────────────

// POST /api/auth/totp/setup
async fn totp_setup(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if user.totp_enabled {
        return Err(AppError::Validation(
            "TOTP 2FA is already enabled".to_string(),
        ));
    }

    let secret = Secret::generate_secret();
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| AppError::Internal(format!("Failed to generate TOTP secret: {e}")))?;

    let totp = build_totp(&secret_bytes, &user.email)?;

    sqlx::query("UPDATE users SET totp_secret = $1, updated_at = NOW() WHERE id = $2")
        .bind(&secret_bytes)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "otpauth_url": totp.get_url(),
        "secret_base32": secret.to_encoded().to_string(),
        "message": "Scan the QR code with your authenticator app, then confirm with /auth/totp/enable"
    }))))
}

// POST /api/auth/totp/enable
async fn totp_enable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if user.totp_enabled {
        return Err(AppError::Validation(
            "TOTP 2FA is already enabled".to_string(),
        ));
    }

    let secret = user.totp_secret.as_ref().ok_or(AppError::Validation(
        "Run /auth/totp/setup first".to_string(),
    ))?;

    let totp = build_totp(secret, &user.email)?;
    let valid = totp
        .check_current(&body.code)
        .map_err(|e| AppError::Internal(format!("TOTP verification failed: {e}")))?;

    if !valid {
        return Err(AppError::TotpInvalid);
    }

    sqlx::query("UPDATE users SET totp_enabled = TRUE, updated_at = NOW() WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    let codes = issue_backup_codes(&state.db, auth.user_id).await?;

    let _ = state
        .email
        .send_security_alert(
            &user.email,
            &user.display_name,
            "2FA (application) activée",
            "L'authentification à deux facteurs par application a été activée sur ton compte.",
        )
        .await;

    Ok(Json(build_response(json!({
        "message": "TOTP 2FA enabled successfully",
        "backup_codes": codes,
        "backup_codes_note": "Store these codes somewhere safe — they will not be shown again."
    }))))
}

// POST /api/auth/totp/disable
async fn totp_disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !user.totp_enabled {
        return Err(AppError::Validation("TOTP 2FA is not enabled".to_string()));
    }

    let secret = user
        .totp_secret
        .as_ref()
        .ok_or(AppError::Internal("TOTP enabled but no secret".to_string()))?;

    let totp = build_totp(secret, &user.email)?;
    let valid = totp
        .check_current(&body.code)
        .map_err(|e| AppError::Internal(format!("TOTP verification failed: {e}")))?;

    if !valid {
        return Err(AppError::TotpInvalid);
    }

    sqlx::query(
        "UPDATE users SET totp_enabled = FALSE, totp_secret = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    sqlx::query("DELETE FROM totp_backup_codes WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    let _ = state
        .email
        .send_security_alert(
            &user.email,
            &user.display_name,
            "2FA (application) désactivée",
            "L'authentification à deux facteurs par application a été désactivée sur ton compte.",
        )
        .await;

    Ok(Json(build_response(json!({
        "message": "TOTP 2FA disabled successfully"
    }))))
}

// ─── Email 2FA ───────────────────────────────────────────────────

// POST /api/auth/email-2fa/enable
async fn email_2fa_enable(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !user.email_verified {
        return Err(AppError::Validation(
            "You must verify your email before enabling email 2FA".to_string(),
        ));
    }

    if user.email_2fa_enabled {
        return Err(AppError::Validation(
            "Email 2FA is already enabled".to_string(),
        ));
    }

    sqlx::query("UPDATE users SET email_2fa_enabled = TRUE, updated_at = NOW() WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    let _ = state
        .email
        .send_security_alert(
            &user.email,
            &user.display_name,
            "2FA (email) activée",
            "L'authentification à deux facteurs par email a été activée sur ton compte.",
        )
        .await;

    Ok(Json(build_response(json!({
        "message": "Email 2FA enabled. A code will be sent to your email on each login."
    }))))
}

// POST /api/auth/email-2fa/disable
async fn email_2fa_disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !user.email_2fa_enabled {
        return Err(AppError::Validation("Email 2FA is not enabled".to_string()));
    }

    // Require password confirmation to disable
    let valid = AuthService::verify_password(&body.current_password, &user.password_hash)?;
    if !valid {
        return Err(AppError::InvalidCredentials);
    }

    sqlx::query("UPDATE users SET email_2fa_enabled = FALSE, updated_at = NOW() WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    let _ = state
        .email
        .send_security_alert(
            &user.email,
            &user.display_name,
            "2FA (email) désactivée",
            "L'authentification à deux facteurs par email a été désactivée sur ton compte.",
        )
        .await;

    Ok(Json(build_response(json!({
        "message": "Email 2FA disabled successfully"
    }))))
}

// ─── Account deletion (RGPD) ─────────────────────────────────────

// DELETE /api/auth/account
async fn delete_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DeleteAccountRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    // Verify password
    let valid = AuthService::verify_password(&body.password, &user.password_hash)?;
    if !valid {
        return Err(AppError::InvalidCredentials);
    }

    // If TOTP is enabled, require TOTP code
    if user.totp_enabled {
        let code = body.totp_code.as_deref().ok_or(AppError::TotpRequired)?;
        let secret = user
            .totp_secret
            .as_ref()
            .ok_or(AppError::Internal("TOTP enabled but no secret".to_string()))?;
        let totp = build_totp(secret, &user.email)?;
        if !totp
            .check_current(code)
            .map_err(|e| AppError::Internal(format!("TOTP check failed: {e}")))?
        {
            return Err(AppError::TotpInvalid);
        }
    }

    // Delete all user data (cascade order matters)
    // 1. User skills (P8.7 : skill_fragments legacy retiré)
    sqlx::query("DELETE FROM user_skills WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    // 2. Challenge submissions
    sqlx::query("DELETE FROM challenge_submissions WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    // 3. User record
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    // 4. Remove from all leaderboards
    LeaderboardService::remove_user(&mut state.redis.clone(), auth.user_id).await?;

    // 5. Revoke tokens in Redis
    AuthService::revoke_refresh_token(&mut state.redis.clone(), auth.user_id).await?;

    // 6. Clear any pending Redis keys
    let mut redis = state.redis.clone();
    let email_2fa = email_2fa_key(auth.user_id);
    let pending = login_pending_2fa_key(auth.user_id);
    let _: Result<(), _> = redis.del(&email_2fa).await;
    let _: Result<(), _> = redis.del(&pending).await;

    let clear_access = clear_cookie("access_token", "/");
    let clear_refresh = clear_cookie("refresh_token", REFRESH_COOKIE_PATH);
    let clear_csrf = "csrf_token=; Secure; SameSite=Strict; Path=/api; Max-Age=0".to_string();
    let clear_admin_access = clear_cookie("admin_access_token", "/");
    let clear_admin_refresh = clear_cookie("admin_refresh_token", REFRESH_COOKIE_PATH);
    let clear_admin_csrf =
        "admin_csrf_token=; Secure; SameSite=Strict; Path=/api; Max-Age=0".to_string();

    tracing::info!(user_id = %auth.user_id, email = %user.email, "Account deleted (RGPD right to erasure)");

    Ok((
        AppendHeaders([
            (SET_COOKIE, clear_access),
            (SET_COOKIE, clear_refresh),
            (SET_COOKIE, clear_csrf),
            (SET_COOKIE, clear_admin_access),
            (SET_COOKIE, clear_admin_refresh),
            (SET_COOKIE, clear_admin_csrf),
        ]),
        Json(build_response(json!({
            "message": "Your account and all personal data have been permanently deleted."
        }))),
    ))
}

// ─── Onboarding (Pattern C) ───────────────────────────────────────

// POST /api/auth/complete-profile
// Called by any user whose signup path didn't collect skill_domain / terms_accepted
// (OAuth + magic link). Idempotent when the profile is already complete: 400 explains why.
async fn complete_profile(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    auth: AuthUser,
    Json(body): Json<CompleteProfileRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_skill_domain(&body.skill_domain)?;
    if !body.terms_accepted {
        return Err(AppError::Validation(
            "You must accept the Terms of Service and Privacy Policy".into(),
        ));
    }

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    // Refuse if the profile is already complete — avoids retroactively rewriting the
    // skill_domain once the user has started earning fragments.
    if user.skill_domain.is_some() && user.terms_accepted_at.is_some() {
        return Err(AppError::Validation("Profile is already complete".into()));
    }

    sqlx::query(
        "UPDATE users
         SET skill_domain = $1,
             terms_accepted_at = COALESCE(terms_accepted_at, NOW()),
             country = COALESCE($2, country),
             city = COALESCE($3, city),
             updated_at = NOW()
         WHERE id = $4",
    )
    .bind(&body.skill_domain)
    .bind(&body.country)
    .bind(
        body.city
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    audit::record(
        &state.db,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(auth.user_id),
            action: "user.complete_profile",
            target_type: Some("user"),
            target_id: Some(auth.user_id),
            metadata: Some(json!({ "skill_domain": body.skill_domain })),
            headers: Some(&headers),
        },
    )
    .await;

    metrics::counter!(
        "skilluv_signups_total",
        "skill_domain" => body.skill_domain.clone(),
        "path" => "onboarding_completion"
    )
    .increment(1);

    Ok(Json(build_response(json!({
        "message": "Profile completed",
        "profile_completed": true,
    }))))
}

// ─── Email change (double confirmation) ──────────────────────────

fn email_change_key(user_id: Uuid) -> String {
    format!("email_change_hash:{user_id}")
}

fn email_change_token_lookup(token: &str) -> String {
    format!("email_change_token:{token}")
}

// POST /api/auth/change-email — kicks off a change; confirmation link goes to the NEW address,
// notification email goes to the OLD address.
async fn request_email_change(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangeEmailRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_email(&body.new_email)?;
    let new_email = body.new_email.trim().to_lowercase();

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    let valid = AuthService::verify_password(&body.current_password, &user.password_hash)?;
    if !valid {
        return Err(AppError::InvalidCredentials);
    }

    if new_email == user.email {
        return Err(AppError::Validation(
            "New email must be different from the current one".to_string(),
        ));
    }

    // Reject if another account already uses that email
    let taken: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&new_email)
        .fetch_optional(&state.db)
        .await?;
    if taken.is_some() {
        return Err(AppError::Validation(
            "This email is already in use".to_string(),
        ));
    }

    let token = generate_token();
    let token_hash = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(token.as_bytes());
        h.finalize().to_vec()
    };
    let expires = chrono::Utc::now() + chrono::Duration::hours(1);

    sqlx::query(
        "INSERT INTO pending_email_change (user_id, new_email, token_hash, expires_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET new_email = EXCLUDED.new_email,
                                              token_hash = EXCLUDED.token_hash,
                                              expires_at = EXCLUDED.expires_at,
                                              created_at = NOW()",
    )
    .bind(auth.user_id)
    .bind(&new_email)
    .bind(&token_hash)
    .bind(expires)
    .execute(&state.db)
    .await?;

    // Also store the raw token → user_id map in Redis for GET lookup by token
    let mut redis = state.redis.clone();
    let () = redis
        .set_ex(
            email_change_token_lookup(&token),
            auth.user_id.to_string(),
            60 * 60,
        )
        .await?;
    let () = redis
        .set_ex(
            email_change_key(auth.user_id),
            hex::encode(&token_hash),
            60 * 60,
        )
        .await?;

    let link = format!(
        "{}/auth/change-email/confirm?token={}",
        state.config.base_url, token
    );
    state
        .email
        .send_direct(
            &new_email,
            &user.display_name,
            "Skilluv — Confirme ton nouvel email",
            &format!(
                r#"<p>Salut {},</p><p>Confirme le changement d'email en cliquant : <a href="{link}">{link}</a></p><p>Ce lien expire dans 1h.</p>"#,
                user.display_name
            ),
        )
        .await?;

    let _ = state
        .email
        .send_security_alert(
            &user.email,
            &user.display_name,
            "Demande de changement d'email",
            &format!(
                "Une demande de changement d'email vers {new_email} a été enregistrée. Si ce n'est pas toi, change immédiatement ton mot de passe."
            ),
        )
        .await;

    Ok(Json(build_response(json!({
        "message": "Confirmation email sent to the new address"
    }))))
}

// GET /api/auth/change-email/confirm?token=...
async fn confirm_email_change(
    State(state): State<AppState>,
    Query(query): Query<ConfirmEmailChangeQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = state.redis.clone();
    let uid_str: Option<String> = redis.get(email_change_token_lookup(&query.token)).await?;
    let user_id: Uuid = uid_str
        .ok_or(AppError::Validation("Invalid or expired token".into()))?
        .parse()
        .map_err(|_| AppError::Internal("Bad user_id in token map".into()))?;

    let row: Option<(String, Vec<u8>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT new_email, token_hash, expires_at FROM pending_email_change WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let (new_email, token_hash, expires_at) =
        row.ok_or(AppError::Validation("No pending email change".into()))?;
    if expires_at < chrono::Utc::now() {
        return Err(AppError::Validation("Token expired".into()));
    }

    // Verify the token matches
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(query.token.as_bytes());
    let presented = h.finalize().to_vec();
    if presented != token_hash {
        return Err(AppError::Validation("Invalid token".into()));
    }

    sqlx::query(
        "UPDATE users SET email = $1, email_verified = TRUE, updated_at = NOW() WHERE id = $2",
    )
    .bind(&new_email)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    sqlx::query("DELETE FROM pending_email_change WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    let () = redis.del(email_change_token_lookup(&query.token)).await?;
    let () = redis.del(email_change_key(user_id)).await?;

    // Revoke all sessions — force re-login with the new email
    SessionService::revoke_all(&state.db, user_id).await?;

    Ok(Json(build_response(json!({
        "message": "Email updated. Please log in again."
    }))))
}

// ─── TOTP backup codes ───────────────────────────────────────────

fn generate_backup_code() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // 8 chars from an unambiguous alphabet, formatted `XXXX-XXXX`.
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut out = String::with_capacity(9);
    for i in 0..8 {
        // Mix the seed with the position so successive calls give different chars.
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407 + i as u128);
        let idx = ((seed >> 33) as usize) % ALPHABET.len();
        out.push(ALPHABET[idx] as char);
        if i == 3 {
            out.push('-');
        }
    }
    out
}

async fn issue_backup_codes(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    // Wipe any existing (used or not) — regenerate replaces the full set.
    sqlx::query("DELETE FROM totp_backup_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;

    let mut plaintext = Vec::with_capacity(10);
    for _ in 0..10 {
        let code = generate_backup_code();
        let hash = AuthService::hash_password(&code)?;
        sqlx::query("INSERT INTO totp_backup_codes (user_id, code_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(&hash)
            .execute(db)
            .await?;
        plaintext.push(code);
    }
    Ok(plaintext)
}

async fn consume_backup_code(db: &PgPool, user_id: Uuid, presented: &str) -> Result<(), AppError> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, code_hash FROM totp_backup_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    for (id, hash) in rows {
        if AuthService::verify_password(presented, &hash)? {
            sqlx::query("UPDATE totp_backup_codes SET used_at = NOW() WHERE id = $1")
                .bind(id)
                .execute(db)
                .await?;
            return Ok(());
        }
    }
    Err(AppError::TotpInvalid)
}

// POST /api/auth/totp/backup-codes/regenerate — requires a valid live TOTP code.
async fn regenerate_backup_codes(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    if !user.totp_enabled {
        return Err(AppError::Validation(
            "Enable TOTP 2FA before generating backup codes".to_string(),
        ));
    }
    let secret = user
        .totp_secret
        .as_ref()
        .ok_or(AppError::Internal("TOTP enabled but no secret".to_string()))?;
    let totp = build_totp(secret, &user.email)?;
    if !totp
        .check_current(&body.code)
        .map_err(|e| AppError::Internal(format!("TOTP check failed: {e}")))?
    {
        return Err(AppError::TotpInvalid);
    }

    let codes = issue_backup_codes(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "backup_codes": codes,
        "message": "Store these codes somewhere safe. They will not be shown again."
    }))))
}

// ─── Sessions / device management ────────────────────────────────

// GET /api/auth/sessions
async fn list_sessions(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let sessions = SessionService::list_active(&state.db, auth.user_id).await?;
    let current = parse_refresh_cookie(&headers).map(|(sid, _)| sid);
    Ok(Json(build_response(json!({
        "sessions": sessions,
        "current_session_id": current,
    }))))
}

// DELETE /api/auth/sessions/:id
async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(session_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    SessionService::revoke_one(&state.db, auth.user_id, session_id).await?;
    Ok(Json(build_response(
        json!({ "message": "Session revoked" }),
    )))
}

// POST /api/auth/sessions/revoke-all — revoke every session except the current one
async fn revoke_all_other_sessions(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    match parse_refresh_cookie(&headers) {
        Some((current, _)) => {
            SessionService::revoke_all_except(&state.db, auth.user_id, current).await?;
        }
        None => {
            SessionService::revoke_all(&state.db, auth.user_id).await?;
        }
    }
    Ok(Json(build_response(json!({
        "message": "All other sessions revoked"
    }))))
}

// ─── Helpers ─────────────────────────────────────────────────────

fn build_totp(secret: &[u8], email: &str) -> Result<TOTP, AppError> {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_vec(),
        Some("Skilluv".to_string()),
        email.to_string(),
    )
    .map_err(|e| AppError::Internal(format!("Failed to create TOTP: {e}")))
}
