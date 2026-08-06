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
use crate::api_response::{ApiResponse, SimpleMessage};
use crate::errors::AppError;
use crate::services::session::SessionRow;

use crate::middleware::{
    AuthUser, RateLimiter, build_csrf_cookie, build_csrf_cookie_with_prefix, extract_ip,
    generate_csrf_token,
};
use crate::models::{User, UserPrivate};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::audit::{self, ActorType, AuditEntry};
use crate::services::{AuthService, LeaderboardService, SessionService};

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ListSessionsResponse {
    /// Every currently-active session for the authenticated user.
    pub sessions: Vec<SessionRow>,
    /// ID of the session the caller is currently using — `None` when the
    /// request arrived without a refresh cookie (e.g. mobile flows that
    /// only carry the access token).
    pub current_session_id: Option<Uuid>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct RankInfo {
    /// Global all-time rank across every skill domain. `None` when the
    /// user has not accumulated any activity yet.
    pub global: Option<i64>,
    /// Rank within the caller's current `skill_domain`. `None` when the
    /// user has not picked a domain (Pattern C SSO signups before
    /// onboarding).
    pub domain: Option<i64>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct TotpSetupResponse {
    /// otpauth:// URL — feed it to `qrcode` on the front and render.
    #[schema(example = "otpauth://totp/Skilluv:user@example.com?secret=JBSW…&issuer=Skilluv")]
    pub otpauth_url: String,
    /// Base-32 encoded TOTP secret. Displayed as a fallback for users
    /// whose authenticator can't scan the QR.
    pub secret_base32: String,
    pub message: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct TotpEnableResponse {
    pub message: String,
    /// One-time backup codes generated for this account. Formatted
    /// `XXXX-XXXX`. Displayed **once** — the server keeps only hashes.
    pub backup_codes: Vec<String>,
    pub backup_codes_note: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct RefreshResponse {
    /// Always `true` on success — kept as a redundant flag because a few
    /// legacy front-end callers still branch on `data.ok`.
    pub ok: bool,
    /// Freshly minted CSRF token — the front must store it and send it
    /// back as `X-CSRF-Token` on the next mutating request.
    pub csrf_token: String,
    /// Same enumeration as `MeResponse.login_method` — preserved across
    /// rotation so downstream policy stays faithful.
    pub login_method: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct CompleteProfileResponse {
    pub message: String,
    /// Always `true` on success — mirrors `UserPrivate.profile_completed`
    /// so the caller can update local state without a follow-up `/me`.
    pub profile_completed: bool,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    pub user: UserPrivate,
    /// Which primary factor authenticated the current session — one of
    /// `password`, `sso`, `webauthn`, `magic_link`. Frontends use it to
    /// skip the enterprise TOTP-setup redirect for non-password sessions.
    pub login_method: String,
    /// True when the account has at least one registered WebAuthn
    /// credential. Combined with `totp_enabled`, satisfies the
    /// enterprise/admin second-factor requirement.
    pub has_passkey: bool,
    pub rank: RankInfo,
}

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

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    pub user: UserPrivate,
    /// Fresh CSRF token — front sends it back as `X-CSRF-Token` on
    /// subsequent mutating requests.
    pub csrf_token: String,
    /// Always `"password"` for the register endpoint — the field is
    /// echoed to match the shape of `/login` so the front can reuse
    /// the same store logic.
    pub login_method: String,
    pub message: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct LoginSuccessResponse {
    pub user: UserPrivate,
    pub csrf_token: String,
    pub login_method: String,
    /// True when the account has at least one WebAuthn credential —
    /// satisfies the enterprise/admin second-factor requirement even
    /// without TOTP.
    pub has_passkey: bool,
    /// True when the account is enterprise / recruiter / admin AND has
    /// neither TOTP nor a passkey — front must route to the 2FA
    /// enrolment wizard before allowing admin surfaces.
    pub requires_totp_setup: bool,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct LoginPending2faResponse {
    /// Always `true` — signals the front should prompt for the email
    /// 2FA code and re-POST /login with it.
    pub requires_email_2fa: bool,
    pub user_id: Uuid,
    pub message: String,
}

/// The two possible shapes of a successful `/api/auth/login` (and
/// `/api/auth/email-2fa/verify`) response. Serialized untagged — the
/// front discriminates on the presence of `requires_email_2fa`. Utoipa
/// renders it as `oneOf` in the generated schema, so schemathesis can
/// fuzz both branches.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum LoginOutcome {
    Success(LoginSuccessResponse),
    Pending2fa(LoginPending2faResponse),
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct DeleteAccountResponse {
    /// Always `true` on success — the account row is already gone by
    /// the time the client parses this.
    pub account_deleted: bool,
    /// RFC 3339 timestamp of the deletion. Currently equal to `now()`
    /// (immediate deletion); future grace-period implementations will
    /// set it to `now() + N days` for the same wire shape.
    pub scheduled_for: String,
    pub message: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct DataExportResponse {
    /// Always `"queued"` on success — kept as a discriminated field so
    /// clients don't parse the human-facing message.
    #[schema(example = "queued")]
    pub status: String,
    pub message: String,
}

/// Request a full RGPD data export (background job — user gets an email
/// with a signed download link). Rate-limited to 1/24h per account to
/// prevent abuse of the archive generator.
#[utoipa::path(
    post,
    path = "/api/auth/me/data-export",
    tag = "auth",
    responses(
        (status = 200, description = "Export job queued", body = ApiResponse<DataExportResponse>),
        (status = 400, description = "Already requested in the last 24h", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_data_export(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<DataExportResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(DataExportResponse {
        status: "queued".to_string(),
        message: "Your archive is being prepared. You'll receive it by email within a few minutes."
            .to_string(),
    })))
}

// ─── Request types ───────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    // Constraints ci-dessous : miroir strict des `validate_*` handlers plus bas
    // dans ce fichier. Toute modification d'un validator doit se refléter ici
    // pour que le schéma OpenAPI reste opposable (contrat schemathesis).
    #[schema(
        format = "email",
        min_length = 5,
        max_length = 255,
        example = "user@example.com"
    )]
    pub email: String,
    #[schema(
        min_length = 3,
        max_length = 30,
        pattern = r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$",
        example = "jdoe"
    )]
    pub username: String,
    /// 10–128 chars, upper + lower + digit + symbol.
    #[schema(
        min_length = 10,
        max_length = 128,
        pattern = r"^(?=.*[a-z])(?=.*[A-Z])(?=.*[0-9])(?=.*[!-/:-@\[-`{-~]).+$"
    )]
    pub password: String,
    #[schema(min_length = 1, max_length = 50)]
    pub first_name: String,
    #[schema(min_length = 1, max_length = 50)]
    pub last_name: String,
    /// One of `code`, `design`, `game`, `security`.
    #[schema(pattern = r"^(code|design|game|security)$")]
    pub skill_domain: String,
    /// ISO 3166-1 alpha-2 country code (e.g. `SN`).
    #[schema(pattern = r"^[A-Z]{2}$")]
    pub country: Option<String>,
    #[schema(max_length = 100)]
    pub city: Option<String>,
    /// Must be `true` — user acknowledges Terms of Service and Privacy Policy.
    /// Custom deserializer ne laisse passer que `true` ; le schéma associé
    /// (schema_with) émet `{ type: boolean, const: true }` pour que
    /// schemathesis ne génère jamais `false`.
    // Pas de #[serde(default)] volontairement : sans lui, utoipa marque
    // le field 'required' dans le schema OpenAPI, donc schemathesis
    // l'inclut toujours dans ses payloads (avec value=true par le
    // const:true du schema). Un client qui omet le field recoit un 422
    // serde 'missing field' — semantiquement correct (payload malforme).
    // Un client qui envoie explicitement false recoit 400 via le check
    // metier serveur `if !body.terms_accepted`.
    #[schema(schema_with = terms_accepted_schema)]
    pub terms_accepted: bool,
}

/// Génère `{ type: boolean, enum: [true] }` — équivalent à `const: true`
/// que utoipa 5 n'a pas encore comme attribut direct. `enum_values` est une
/// forme JSON Schema Draft 2020-12 pleinement supportée par schemathesis.
pub fn terms_accepted_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::SchemaType::Type(Type::Boolean))
        .enum_values(Some(vec![serde_json::json!(true)]))
        .description(Some(
            "Must be true — user acknowledges Terms of Service and Privacy Policy",
        ))
        .build()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    /// Email or username.
    #[schema(min_length = 3, max_length = 255)]
    pub identifier: String,
    #[schema(min_length = 1, max_length = 128)]
    pub password: String,
    /// Live 6-digit TOTP code — required when the account has TOTP 2FA
    /// enabled and no `backup_code` is provided.
    #[schema(pattern = r"^[0-9]{6}$")]
    pub totp_code: Option<String>,
    /// Email 2FA code — required on the second call to /login when the
    /// account has email 2FA enabled.
    #[schema(pattern = r"^[0-9]{6}$")]
    pub email_2fa_code: Option<String>,
    /// One-time TOTP backup code (used when the user lost their authenticator).
    #[schema(min_length = 8, max_length = 32)]
    pub backup_code: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct VerifyEmailQuery {
    /// One-shot verification token sent by email. Consumed after first use.
    #[param(min_length = 20, max_length = 128)]
    pub token: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ForgotPasswordRequest {
    /// Email address whose account should receive the reset link. The
    /// endpoint always returns 200 to prevent account enumeration.
    #[schema(
        format = "email",
        min_length = 5,
        max_length = 255,
        example = "user@example.com"
    )]
    pub email: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResetPasswordRequest {
    /// One-shot token from the reset email (valid 1h, single-use).
    #[schema(min_length = 20, max_length = 128)]
    pub token: String,
    /// New password. Must meet policy: 10–128 chars, upper+lower+digit+symbol.
    #[schema(
        min_length = 10,
        max_length = 128,
        pattern = r"^(?=.*[a-z])(?=.*[A-Z])(?=.*[0-9])(?=.*[!-/:-@\[-`{-~]).+$"
    )]
    pub new_password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    /// Existing password, re-entered to authorize the change.
    #[schema(min_length = 1, max_length = 128)]
    pub current_password: String,
    /// New password. Must meet policy: 10–128 chars, upper+lower+digit+symbol.
    #[schema(
        min_length = 10,
        max_length = 128,
        pattern = r"^(?=.*[a-z])(?=.*[A-Z])(?=.*[0-9])(?=.*[!-/:-@\[-`{-~]).+$"
    )]
    pub new_password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangeEmailRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub current_password: String,
    /// New email address — must not already be in use. A confirmation
    /// email is sent there; the change only lands once the recipient
    /// clicks the confirmation link.
    #[schema(
        format = "email",
        min_length = 5,
        max_length = 255,
        example = "new-address@example.com"
    )]
    pub new_email: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CompleteProfileRequest {
    /// One of `code`, `design`, `game`, `security`.
    #[schema(pattern = r"^(code|design|game|security)$", example = "code")]
    pub skill_domain: String,
    /// Must be `true` — user acknowledges ToS + Privacy Policy. Voir
    /// `deserialize_true_bool` + `terms_accepted_schema` sur RegisterRequest
    /// pour l'explication du couple serde/utoipa.
    // Pas de #[serde(default)] volontairement : sans lui, utoipa marque
    // le field 'required' dans le schema OpenAPI, donc schemathesis
    // l'inclut toujours dans ses payloads (avec value=true par le
    // const:true du schema). Un client qui omet le field recoit un 422
    // serde 'missing field' — semantiquement correct (payload malforme).
    // Un client qui envoie explicitement false recoit 400 via le check
    // metier serveur `if !body.terms_accepted`.
    #[schema(schema_with = terms_accepted_schema)]
    pub terms_accepted: bool,
    /// ISO 3166-1 alpha-2 country code (e.g. `SN`, `CI`, `FR`).
    #[schema(pattern = r"^[A-Z]{2}$")]
    pub country: Option<String>,
    #[schema(max_length = 100)]
    pub city: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConfirmEmailChangeQuery {
    /// One-shot confirmation token from the email sent to the new
    /// address. Valid 1h, single-use.
    #[param(min_length = 20, max_length = 128)]
    pub token: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TotpCodeRequest {
    /// Live 6-digit TOTP code from the authenticator app.
    #[schema(pattern = r"^[0-9]{6}$", example = "123456")]
    pub code: String,
}

/// Disabling 2FA is a sensitive downgrade. We require BOTH factors — the
/// current TOTP code AND the account password — so that a stolen session
/// alone can't unlock the account. Modeled on GitHub / Google's flow.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TotpDisableRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub password: String,
    /// Live 6-digit TOTP code from the authenticator app.
    #[schema(pattern = r"^[0-9]{6}$")]
    pub code: String,
}

/// Body for any endpoint that gates a sensitive action on password re-entry
/// (enable/disable email 2FA, other sudo-mode toggles). Avoids leaking the
/// unrelated fields of `ChangePasswordRequest` that caused BE-P0-04.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PasswordConfirmRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeleteAccountRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub password: String,
    /// Required when TOTP 2FA is enabled — otherwise ignored.
    #[schema(pattern = r"^[0-9]{6}$")]
    pub totp_code: Option<String>,
    /// Free-text reason captured for the audit trail (RGPD compliance).
    /// Optional — front sends it when the user filled the "why leaving?" prompt.
    #[schema(max_length = 2000)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Email2faVerifyRequest {
    /// 6-digit code received by email.
    #[schema(pattern = r"^[0-9]{6}$")]
    pub code: String,
    /// User id issued by `/auth/login` when 2FA is required.
    pub user_id: Uuid,
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
    let char_count = password.chars().count();
    if char_count < 10 {
        return Err(AppError::Validation(
            "Password must be at least 10 characters".to_string(),
        ));
    }
    if char_count > 128 {
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
    crate::validators::validate_bounded_line(name, field, 1, 50)
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

/// Register a new user account. Sends the email verification link,
/// mints session + CSRF cookies, returns the private user record.
/// Rate-limited to 5 registrations per IP per hour.
#[utoipa::path(
    post,
    path = "/api/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created — verification email sent", body = ApiResponse<RegisterResponse>),
        (status = 400, description = "Validation error (email, username, password policy, terms not accepted, duplicate)", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Rate limit hit (5/h per IP)", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn register(
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
        // 409 Conflict : ressource existe deja (email/username unique
        // conflict). Semantiquement REST + accepte par schemathesis
        // positive_data_acceptance (409 dans le set attendu).
        return Err(AppError::Conflict(
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
            &state.config.frontend_url,
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
        Json(ApiResponse::new(RegisterResponse {
            user: user_private,
            csrf_token: csrf,
            login_method: "password".to_string(),
            message: "Account created. Please verify your email.".to_string(),
        })),
    ))
}

const LOGIN_LOCKOUT_THRESHOLD: i32 = 5;
const LOGIN_LOCKOUT_MINUTES: i64 = 15;

/// Authenticate with password + optional 2FA. Response is `oneOf` two
/// shapes:
///
/// - **Success** — cookies set, `LoginSuccessResponse` returned.
/// - **Pending 2FA** — the account has email 2FA enabled and no
///   `email_2fa_code` was sent; the server sends the code by email and
///   returns `LoginPending2faResponse` so the front can prompt.
///
/// TOTP is checked inline (never returns pending) — the front sends
/// `totp_code` or `backup_code` on the first call.
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login success OR email 2FA pending", body = ApiResponse<LoginOutcome>),
        (status = 400, description = "Account locked after too many failed attempts", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Invalid credentials, TOTP invalid, email 2FA invalid", body = crate::api_response::ErrorResponse),
        (status = 403, description = "SSO required (enterprise), account banned, or TOTP required (no code sent)", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Rate limit hit (20/min per IP)", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn login(
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
    if let Some(domain) = user.email.split('@').nth(1).map(str::to_lowercase)
        && let Some((cfg, slug)) =
            crate::services::enterprise_sso::find_by_email_domain(&state.db, &domain).await?
        && cfg.enforce_sso
    {
        let start_url = format!(
            "{}/api/enterprise/sso/{}/start",
            state.config.base_url, slug
        );
        return Err(AppError::SsoRequired { start_url });
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
                Json(ApiResponse::new(LoginOutcome::Pending2fa(
                    LoginPending2faResponse {
                        requires_email_2fa: true,
                        user_id: user.id,
                        message: "A verification code has been sent to your email".to_string(),
                    },
                ))),
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
        Json(ApiResponse::new(LoginOutcome::Success(
            LoginSuccessResponse {
                user: user_private,
                csrf_token: csrf,
                login_method: "password".to_string(),
                has_passkey,
                requires_totp_setup,
            },
        ))),
    ))
}

/// Complete a login that was gated on email 2FA. Same success shape as
/// `/api/auth/login` — the pending-2FA branch is never returned here.
#[utoipa::path(
    post,
    path = "/api/auth/email-2fa/verify",
    tag = "auth",
    request_body = Email2faVerifyRequest,
    responses(
        (status = 200, description = "Login complete — cookies issued", body = ApiResponse<LoginSuccessResponse>),
        (status = 400, description = "Missing user_id or no pending 2FA", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Code invalid or user not found", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Account banned", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn email_2fa_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Email2faVerifyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = body.user_id;

    // Check there's a pending 2FA
    let mut redis = state.redis.clone();
    let pending_key = login_pending_2fa_key(user_id);
    let pending: Option<String> = redis.get(&pending_key).await?;
    if pending.is_none() {
        return Err(AppError::NotFound(
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
        Json(ApiResponse::new(LoginSuccessResponse {
            user: user_private,
            csrf_token: csrf,
            login_method: "password".to_string(),
            has_passkey,
            requires_totp_setup,
        })),
    ))
}

/// Rotate the refresh + access tokens. Refresh token is read from the
/// httpOnly cookie (either `refresh_token` or `admin_refresh_token`
/// depending on the caller's origin). Emits fresh access + refresh +
/// CSRF cookies on the caller's namespace.
#[utoipa::path(
    post,
    path = "/api/auth/refresh",
    tag = "auth",
    responses(
        (status = 200, description = "Tokens rotated, new cookies issued", body = ApiResponse<RefreshResponse>),
        (status = 401, description = "No refresh cookie, reuse detected, or session revoked", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Account banned since last refresh", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn refresh(
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
        Json(ApiResponse::new(RefreshResponse {
            ok: true,
            csrf_token: csrf,
            login_method: login_method.0,
        })),
    ))
}

/// Log the current session out and clear every auth cookie in both
/// namespaces (public + admin). Deliberately does NOT require a valid
/// `AuthUser`: an expired access_token would otherwise 401 before we
/// reach the revocation code, leaving the DB row orphaned even though
/// the client considers itself logged out. The refresh_token cookie
/// carries a `session_id` we can trust structurally (uuid + opaque token)
/// — we look the row up ourselves and revoke it, regardless of whether
/// the JWT is still valid.
#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "auth",
    responses(
        (status = 200, description = "Session revoked and cookies cleared", body = ApiResponse<SimpleMessage>),
    ),
)]
pub async fn logout(
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
        Json(ApiResponse::new(SimpleMessage::new(
            "Logged out successfully",
        ))),
    ))
}

/// Return the currently authenticated user's private profile alongside
/// the leaderboard ranks (global + domain) and `login_method` /
/// `has_passkey` flags used by the frontend to gate enterprise flows.
#[utoipa::path(
    get,
    path = "/api/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Profile + rank + auth-method flags", body = ApiResponse<MeResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "User row missing (rare — stale JWT)", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MeResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(MeResponse {
        user: user_private,
        // Surfaced so the frontend can decide policy without decoding the JWT
        // (e.g. skipping the enterprise TOTP redirect for `sso` / `webauthn`).
        login_method: auth.login_method,
        has_passkey,
        rank: RankInfo {
            global: global_rank,
            domain: domain_rank,
        },
    })))
}

/// Verify the account email using the one-shot token sent at registration.
/// The link is emailed as `${base_url}/verify-email?token=...`; frontends
/// simply forward the query string to this endpoint.
#[utoipa::path(
    get,
    path = "/api/auth/verify-email",
    tag = "auth",
    params(VerifyEmailQuery),
    responses(
        (status = 200, description = "Email successfully verified", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Token invalid or expired", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let mut redis = state.redis.clone();
    let key = email_verify_key(&query.token);
    let user_id_str: Option<String> = redis.get(&key).await?;

    // 404 : le token est une ressource, "pas trouve/expire" = NotFound
    // (pas Validation qui vaudrait pour un payload malforme). Voir aussi
    // reset_password + confirm_email_change.
    let user_id_str = user_id_str.ok_or_else(|| {
        AppError::NotFound("Verification token not found, expired, or already used".to_string())
    })?;

    let user_id: Uuid = user_id_str
        .parse()
        .map_err(|_| AppError::Internal("Invalid user_id in token".to_string()))?;

    sqlx::query("UPDATE users SET email_verified = TRUE, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;

    let () = redis.del(&key).await?;

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Email verified successfully",
    ))))
}

/// Re-send the account verification email. Rate-limit lives on the email
/// service (Brevo throttles duplicates); we still refuse if the account is
/// already verified.
#[utoipa::path(
    post,
    path = "/api/auth/resend-verification",
    tag = "auth",
    responses(
        (status = 200, description = "Verification email queued", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Email is already verified", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
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
            &state.config.frontend_url,
        )
        .await?;

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Verification email sent",
    ))))
}

/// Kick off a password-reset email. Always returns 200 with the same message
/// whether the account exists or not — anti-enumeration.
#[utoipa::path(
    post,
    path = "/api/auth/forgot-password",
    tag = "auth",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Reset email queued if account exists", body = ApiResponse<SimpleMessage>),
    ),
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    // Valide d'abord le format email — schema-invalid input doit etre
    // rejete en 400 (RFC : negative_data_rejection). L'anti-enumeration
    // s'applique APRES : email format valide + user inexistant -> 200
    // silencieux. Email malforme (< 5 chars, pas d'@) -> 400, ne leake
    // pas d'info sur les comptes existants puisque tout email malforme
    // recoit 400.
    validate_email(&body.email)?;

    // Always return success to prevent email enumeration
    let response = ApiResponse::new(SimpleMessage::new(
        "If an account exists with this email, a reset link has been sent",
    ));

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
                &state.config.frontend_url,
            )
            .await?;
    }

    Ok(Json(response))
}

/// Complete a password reset with the one-shot token. Revokes every
/// existing session (all devices signed out) as a defensive measure —
/// mirrors GitHub / Google behaviour after credential reset.
#[utoipa::path(
    post,
    path = "/api/auth/reset-password",
    tag = "auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset — all sessions revoked", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Token invalid or password fails policy", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    validate_password(&body.new_password)?;

    let mut redis = state.redis.clone();
    let key = password_reset_key(&body.token);
    let user_id_str: Option<String> = redis.get(&key).await?;

    // Semantiquement le token est une ressource : "pas trouve" = 404
    // (pas 400 qui signalerait un payload malforme). Ce framing evite
    // aussi le fail schemathesis positive_data_acceptance : schema-valid
    // data ne doit pas etre refuse avec 400 (schema-invalid), mais avec
    // 404 (ressource inexistante). Voir aussi verify_email pour la
    // meme correction.
    let user_id_str = user_id_str.ok_or_else(|| {
        AppError::NotFound("Reset token not found, expired, or already used".to_string())
    })?;

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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Password reset successfully. Please log in with your new password.",
    ))))
}

/// Change the current password. Requires re-entering the existing one —
/// even for a logged-in user — so a hijacked session alone can't rotate
/// credentials silently.
#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    tag = "auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password updated", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "New password fails policy", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated or current_password wrong", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Password changed successfully",
    ))))
}

// ─── TOTP 2FA ────────────────────────────────────────────────────

/// Kick off TOTP 2FA enrolment. Generates a fresh secret, persists it,
/// and returns the `otpauth://` URL + base-32 secret so the front can
/// render a QR code. The user must then confirm with `/totp/enable`
/// within a reasonable window.
#[utoipa::path(
    post,
    path = "/api/auth/totp/setup",
    tag = "auth",
    responses(
        (status = 200, description = "TOTP secret staged — scan and confirm", body = ApiResponse<TotpSetupResponse>),
        (status = 400, description = "TOTP is already enabled", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn totp_setup(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<TotpSetupResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(TotpSetupResponse {
        otpauth_url: totp.get_url(),
        secret_base32: secret.to_encoded().to_string(),
        message:
            "Scan the QR code with your authenticator app, then confirm with /auth/totp/enable"
                .to_string(),
    })))
}

/// Confirm TOTP enrolment with a live code from the authenticator app.
/// Issues a fresh set of backup codes — displayed **once**, since we
/// only persist their hashes.
#[utoipa::path(
    post,
    path = "/api/auth/totp/enable",
    tag = "auth",
    request_body = TotpCodeRequest,
    responses(
        (status = 200, description = "TOTP enabled — save the backup codes", body = ApiResponse<TotpEnableResponse>),
        (status = 400, description = "TOTP already enabled or setup not run first", body = crate::api_response::ErrorResponse),
        (status = 401, description = "TOTP code invalid or unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn totp_enable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<ApiResponse<TotpEnableResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(TotpEnableResponse {
        message: "TOTP 2FA enabled successfully".to_string(),
        backup_codes: codes,
        backup_codes_note: "Store these codes somewhere safe — they will not be shown again."
            .to_string(),
    })))
}

/// Disable TOTP 2FA. Requires **both** the current password AND a live
/// TOTP code — a stolen session alone can't drop the second factor.
/// Deletes all backup codes as well so a leaked code sheet is neutralised.
#[utoipa::path(
    post,
    path = "/api/auth/totp/disable",
    tag = "auth",
    request_body = TotpDisableRequest,
    responses(
        (status = 200, description = "TOTP 2FA disabled", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "TOTP not enabled", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Password wrong or TOTP invalid", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn totp_disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TotpDisableRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !user.totp_enabled {
        return Err(AppError::Validation("TOTP 2FA is not enabled".to_string()));
    }

    // Password check first — cheap and rate-limited by argon2 cost.
    if !AuthService::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::InvalidCredentials);
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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "TOTP 2FA disabled successfully",
    ))))
}

// ─── Email 2FA ───────────────────────────────────────────────────

/// Enable email-based 2FA. Requires the account email to already be
/// verified — otherwise the user could lock themselves out. Password
/// re-entry gates against session hijack.
#[utoipa::path(
    post,
    path = "/api/auth/email-2fa/enable",
    tag = "auth",
    request_body = PasswordConfirmRequest,
    responses(
        (status = 200, description = "Email 2FA enabled", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Email not verified or already enabled", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Password wrong", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn email_2fa_enable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PasswordConfirmRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    // Confirm the user really typed the password (protect against a stolen
    // session flipping 2FA silently).
    if !AuthService::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::InvalidCredentials);
    }

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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Email 2FA enabled. A code will be sent to your email on each login.",
    ))))
}

/// Disable email-based 2FA. Password re-entry required.
#[utoipa::path(
    post,
    path = "/api/auth/email-2fa/disable",
    tag = "auth",
    request_body = PasswordConfirmRequest,
    responses(
        (status = 200, description = "Email 2FA disabled", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Email 2FA not enabled", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Password wrong", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn email_2fa_disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PasswordConfirmRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if !user.email_2fa_enabled {
        return Err(AppError::Validation("Email 2FA is not enabled".to_string()));
    }

    // Require password confirmation to disable
    let valid = AuthService::verify_password(&body.password, &user.password_hash)?;
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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Email 2FA disabled successfully",
    ))))
}

// ─── Account deletion (RGPD) ─────────────────────────────────────

/// Permanently delete the account and every piece of personal data
/// (RGPD right to erasure). Requires password re-entry, plus a live
/// TOTP code when TOTP 2FA is enabled. Cascading deletes: user_skills,
/// challenge_submissions, users. Also removes the user from every
/// leaderboard and clears all auth cookies (both namespaces).
#[utoipa::path(
    delete,
    path = "/api/auth/account",
    tag = "auth",
    request_body = DeleteAccountRequest,
    responses(
        (status = 200, description = "Account and all personal data deleted", body = ApiResponse<DeleteAccountResponse>),
        (status = 401, description = "Password wrong, TOTP invalid, or unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "TOTP required (2FA enabled but no code sent)", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn delete_account(
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

    let deleted_at = chrono::Utc::now();
    tracing::info!(
        user_id = %auth.user_id,
        email = %user.email,
        reason = body.reason.as_deref().unwrap_or("(no reason)"),
        "Account deleted (RGPD right to erasure)"
    );

    Ok((
        AppendHeaders([
            (SET_COOKIE, clear_access),
            (SET_COOKIE, clear_refresh),
            (SET_COOKIE, clear_csrf),
            (SET_COOKIE, clear_admin_access),
            (SET_COOKIE, clear_admin_refresh),
            (SET_COOKIE, clear_admin_csrf),
        ]),
        // Deletion is immediate ; `scheduled_for` == now for symmetry with
        // any future grace-period implementation the front can already
        // display ("deleted at").
        Json(ApiResponse::new(DeleteAccountResponse {
            account_deleted: true,
            scheduled_for: deleted_at.to_rfc3339(),
            message: "Your account and all personal data have been permanently deleted."
                .to_string(),
        })),
    ))
}

// ─── Onboarding (Pattern C) ───────────────────────────────────────

/// Complete the onboarding profile (skill_domain + ToS + optional
/// geo) for users whose signup path didn't collect these fields
/// (OAuth + magic link). Refuses to run twice — once the profile is
/// complete the caller must go through `/change-*` endpoints instead.
#[utoipa::path(
    post,
    path = "/api/auth/complete-profile",
    tag = "auth",
    request_body = CompleteProfileRequest,
    responses(
        (status = 200, description = "Profile completed", body = ApiResponse<CompleteProfileResponse>),
        (status = 400, description = "Invalid skill_domain, terms not accepted, or profile already complete", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn complete_profile(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    auth: AuthUser,
    Json(body): Json<CompleteProfileRequest>,
) -> Result<Json<ApiResponse<CompleteProfileResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(CompleteProfileResponse {
        message: "Profile completed".to_string(),
        profile_completed: true,
    })))
}

// ─── Email change (double confirmation) ──────────────────────────

fn email_change_key(user_id: Uuid) -> String {
    format!("email_change_hash:{user_id}")
}

fn email_change_token_lookup(token: &str) -> String {
    format!("email_change_token:{token}")
}

/// Kick off an email change. Confirmation link is sent to the NEW
/// address (proving control of it), notification email lands on the OLD
/// address (proving to the current owner that a change is underway).
/// Password re-entry gates against session hijack.
#[utoipa::path(
    post,
    path = "/api/auth/change-email",
    tag = "auth",
    request_body = ChangeEmailRequest,
    responses(
        (status = 200, description = "Confirmation email sent to new address", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Invalid email or already in use", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Password wrong or unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_email_change(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangeEmailRequest>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
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
        state.config.frontend_url, token
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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Confirmation email sent to the new address",
    ))))
}

/// Confirm the email change with the one-shot token. Revokes every
/// active session (all devices signed out) — the old email is no longer
/// valid, so any credential-recovery flow keyed on it must fail closed.
#[utoipa::path(
    get,
    path = "/api/auth/change-email/confirm",
    tag = "auth",
    params(ConfirmEmailChangeQuery),
    responses(
        (status = 200, description = "Email updated, all sessions revoked", body = ApiResponse<SimpleMessage>),
        (status = 400, description = "Token invalid, expired, or no pending change", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn confirm_email_change(
    State(state): State<AppState>,
    crate::middleware::ValidatedQuery(query): crate::middleware::ValidatedQuery<
        ConfirmEmailChangeQuery,
    >,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let mut redis = state.redis.clone();
    let uid_str: Option<String> = redis.get(email_change_token_lookup(&query.token)).await?;
    // Tous les 4 cas "token pas trouve / expire / mismatch" sont
    // semantiquement des 404 (la ressource token n'existe pas OU n'est
    // pas assignee a cet appelant). 400 (Validation) est reserve aux
    // payloads malformes. Framing REST correct + evite fail schemathesis
    // positive_data_acceptance.
    let user_id: Uuid = uid_str
        .ok_or_else(|| AppError::NotFound("Token not found or expired".into()))?
        .parse()
        .map_err(|_| AppError::Internal("Bad user_id in token map".into()))?;

    let row: Option<(String, Vec<u8>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT new_email, token_hash, expires_at FROM pending_email_change WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let (new_email, token_hash, expires_at) =
        row.ok_or_else(|| AppError::NotFound("No pending email change".into()))?;
    if expires_at < chrono::Utc::now() {
        return Err(AppError::NotFound("Token expired".into()));
    }

    // Verify the token matches
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(query.token.as_bytes());
    let presented = h.finalize().to_vec();
    if presented != token_hash {
        return Err(AppError::NotFound("Token mismatch".into()));
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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Email updated. Please log in again.",
    ))))
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
/// Regenerate the set of one-time TOTP backup codes. Invalidates every
/// previously-issued code — including unused ones — as a defensive
/// rotation. TOTP code required to prove authenticator possession.
#[utoipa::path(
    post,
    path = "/api/auth/totp/backup-codes/regenerate",
    tag = "auth",
    request_body = TotpCodeRequest,
    responses(
        (status = 200, description = "Fresh backup codes — displayed once", body = ApiResponse<TotpEnableResponse>),
        (status = 400, description = "TOTP not enabled", body = crate::api_response::ErrorResponse),
        (status = 401, description = "TOTP code invalid or unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn regenerate_backup_codes(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<ApiResponse<TotpEnableResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(TotpEnableResponse {
        message: "Store these codes somewhere safe. They will not be shown again.".to_string(),
        backup_codes: codes,
        backup_codes_note: "Previously-issued codes are now invalid.".to_string(),
    })))
}

// ─── Sessions / device management ────────────────────────────────

// GET /api/auth/sessions
/// List every active session for the caller (device management screen).
/// The current session — the one making this request — is highlighted via
/// `current_session_id` so the frontend can render "this device" badges
/// without leaking cookie contents to JS.
#[utoipa::path(
    get,
    path = "/api/auth/sessions",
    tag = "auth",
    responses(
        (status = 200, description = "Active sessions list", body = ApiResponse<ListSessionsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_sessions(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    auth: AuthUser,
) -> Result<Json<ApiResponse<ListSessionsResponse>>, AppError> {
    let sessions = SessionService::list_active(&state.db, auth.user_id).await?;
    let current = parse_refresh_cookie(&headers).map(|(sid, _)| sid);
    Ok(Json(ApiResponse::new(ListSessionsResponse {
        sessions,
        current_session_id: current,
    })))
}

/// Revoke a specific session by ID. No-op (still 200) if the session
/// already ended or belongs to another user — the query is scoped on
/// `user_id`, so it silently ignores foreign IDs.
#[utoipa::path(
    delete,
    path = "/api/auth/sessions/{id}",
    tag = "auth",
    params(("id" = Uuid, Path, description = "Session UUID to revoke")),
    responses(
        (status = 200, description = "Session revoked", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(session_id): axum::extract::Path<Uuid>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    SessionService::revoke_one(&state.db, auth.user_id, session_id).await?;
    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Session revoked",
    ))))
}

/// Revoke every session except the current one (useful after a password
/// change or when the user notices unfamiliar devices). If no refresh
/// cookie is present, every session — including the current one — is
/// revoked.
#[utoipa::path(
    post,
    path = "/api/auth/sessions/revoke-all",
    tag = "auth",
    responses(
        (status = 200, description = "Other sessions revoked", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_all_other_sessions(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    auth: AuthUser,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    match parse_refresh_cookie(&headers) {
        Some((current, _)) => {
            SessionService::revoke_all_except(&state.db, auth.user_id, current).await?;
        }
        None => {
            SessionService::revoke_all(&state.db, auth.user_id).await?;
        }
    }
    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "All other sessions revoked",
    ))))
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
