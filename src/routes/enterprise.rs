use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{
    AuthUser, RateLimiter, build_csrf_cookie, extract_ip, generate_csrf_token,
};
use crate::models::{Enterprise, EnterpriseMember, User, UserPrivate};
use crate::routes::analytics_consent;
use crate::routes::auth::{validate_email, validate_name, validate_username};
use crate::services::analytics::{events, props};
use crate::services::audit::{self, ActorType, AuditEntry};
use crate::services::{AuthService, SessionService};

/// Redis payload for an enterprise recruiter invitation.
///
/// The membership row is created only when the invite is consumed (accept_invite
/// or OAuth callback carrying the invite_token) — this lets us invite emails that
/// don't yet have a Skilluv account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseInvitePayload {
    pub enterprise_id: Uuid,
    pub email: String,
    pub invited_by: Uuid,
}

fn enterprise_invite_key(token: &str) -> String {
    format!("enterprise_invite:{token}")
}

const ENTERPRISE_INVITE_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Load an enterprise invite payload without deleting it. Callers must call
/// `delete_enterprise_invite` after the invite has been successfully applied.
pub async fn peek_enterprise_invite(
    redis: &mut ConnectionManager,
    token: &str,
) -> Result<EnterpriseInvitePayload, AppError> {
    let key = enterprise_invite_key(token);
    let raw: Option<String> = redis.get(&key).await?;
    let raw = raw.ok_or(AppError::Validation(
        "Invalid or expired invite token".to_string(),
    ))?;
    serde_json::from_str(&raw)
        .map_err(|e| AppError::Internal(format!("Corrupted invite payload: {e}")))
}

pub async fn delete_enterprise_invite(
    redis: &mut ConnectionManager,
    token: &str,
) -> Result<(), AppError> {
    let key = enterprise_invite_key(token);
    let _: () = redis.del(&key).await?;
    Ok(())
}

/// Materialise the recruiter membership for `user_id` from an accepted invite.
/// Idempotent: re-running for an already-active member is a no-op.
pub async fn attach_recruiter_to_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
    user_id: Uuid,
    invited_by: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO enterprise_members (enterprise_id, user_id, role, invited_by, status, accepted_at)
        VALUES ($1, $2, 'recruiter', $3, 'active', NOW())
        ON CONFLICT (enterprise_id, user_id) DO UPDATE SET
            status = 'active',
            accepted_at = COALESCE(enterprise_members.accepted_at, NOW())
        "#,
    )
    .bind(enterprise_id)
    .bind(user_id)
    .bind(invited_by)
    .execute(db)
    .await?;

    // Promote the user's global role to recruiter unless they're already an enterprise owner.
    sqlx::query(
        "UPDATE users SET role = 'recruiter', updated_at = NOW() WHERE id = $1 AND role NOT IN ('enterprise', 'admin')",
    )
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(())
}

pub fn enterprise_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/register", post(register_enterprise))
        .route("/enterprise/profile", get(get_profile))
        .route("/enterprise/profile", put(update_profile))
        .route("/enterprise/logo", post(upload_logo))
        .route("/enterprise/logo", delete(delete_logo))
        .route("/enterprise/invite", post(invite_recruiter))
        .route("/enterprise/invite/accept", post(accept_invite))
        .route("/enterprise/invite/preview", get(invite_preview))
        .route(
            "/enterprise/invite/register-and-accept",
            post(invite_register_and_accept),
        )
        .route("/enterprise/members", get(list_members))
        .route("/enterprise/members/{user_id}", delete(revoke_member))
        .route("/enterprise/memberships", get(list_memberships))
        .route(
            "/enterprise/switch/{enterprise_id}",
            post(switch_enterprise),
        )
}

const MAX_ENTERPRISE_LOGO_SIZE: usize = 2 * 1024 * 1024; // 2MB

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

async fn require_enterprise(state: &AppState, auth: &AuthUser) -> Result<Enterprise, AppError> {
    // Load the security flags we gate on in one round-trip.
    let row: Option<(bool, bool)> =
        sqlx::query_as("SELECT totp_enabled, email_verified FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
    let (totp_enabled, email_verified) = row.ok_or(AppError::Unauthorized)?;

    // Verified email is required for every enterprise route: an unverified
    // owner shouldn't be able to configure SSO, invite recruiters, or move
    // billing. SSO-authenticated sessions bypass this — the IdP already
    // asserted email ownership.
    if !email_verified && auth.login_method != "sso" {
        return Err(AppError::EmailVerificationRequired);
    }

    // Mandatory 2FA for enterprise/recruiter roles. The user satisfies this
    // in one of two ways: (a) the session was minted with a strong factor
    // (SSO / WebAuthn passkey), or (b) the account has at least one strong
    // factor enrolled — either TOTP or a passkey — for future logins.
    // If neither is true we send them to the onboarding wizard where they
    // pick and complete a method.
    let strong_factor_session = matches!(auth.login_method.as_str(), "sso" | "webauthn");
    if matches!(auth.role.as_str(), "enterprise" | "recruiter") && !strong_factor_session {
        // Passkey enrolled = future logins can bypass the gate via
        // login_method='webauthn'. Same 2FA guarantee as TOTP, so we accept
        // it here too.
        let has_passkey: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM webauthn_credentials WHERE user_id = $1)",
        )
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
        if !totp_enabled && !has_passkey {
            return Err(AppError::TotpSetupRequired);
        }
    }

    resolve_active_enterprise(&state.db, auth.user_id, auth.active_enterprise_id).await
}

/// Multi-workspace resolver used by every `require_enterprise*` variant. Prefers
/// the enterprise carried in `auth.active_enterprise_id` (the workspace the
/// user picked via the switcher, propagated as the `active_enterprise` cookie)
/// as long as they still have an ACTIVE membership there. When the cookie is
/// missing, invalid, or points to a revoked membership, we fall back to the
/// most recently accepted membership so single-workspace users never notice.
pub async fn resolve_active_enterprise(
    db: &PgPool,
    user_id: Uuid,
    preferred: Option<Uuid>,
) -> Result<Enterprise, AppError> {
    if let Some(id) = preferred {
        let pinned: Option<Enterprise> = sqlx::query_as(
            r#"
            SELECT e.* FROM enterprises e
            JOIN enterprise_members em ON em.enterprise_id = e.id
            WHERE em.user_id = $1 AND em.enterprise_id = $2 AND em.status = 'active'
            "#,
        )
        .bind(user_id)
        .bind(id)
        .fetch_optional(db)
        .await?;
        if let Some(e) = pinned {
            return Ok(e);
        }
    }

    let fallback: Option<Enterprise> = sqlx::query_as(
        r#"
        SELECT e.* FROM enterprises e
        JOIN enterprise_members em ON em.enterprise_id = e.id
        WHERE em.user_id = $1 AND em.status = 'active'
        ORDER BY em.accepted_at DESC NULLS LAST, em.invited_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    fallback.ok_or(AppError::NotFound("Enterprise not found".to_string()))
}

async fn require_enterprise_owner(
    state: &AppState,
    auth: &AuthUser,
) -> Result<Enterprise, AppError> {
    let enterprise = require_enterprise(state, auth).await?;
    if enterprise.owner_id != auth.user_id {
        return Err(AppError::Forbidden);
    }
    Ok(enterprise)
}

/// Public wrapper around `require_enterprise_owner` for use from sibling route
/// modules (e.g. `enterprise_sso`) that need the same gating (owner + TOTP +
/// SSO-bypass semantics) without duplicating the logic.
pub async fn require_enterprise_owner_pub(
    state: &AppState,
    auth: &AuthUser,
) -> Result<Enterprise, AppError> {
    require_enterprise_owner(state, auth).await
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ─── Request types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegisterEnterpriseRequest {
    email: String,
    username: String,
    password: String,
    first_name: String,
    last_name: String,
    company_name: String,
    website: Option<String>,
    industry: Option<String>,
    company_size: String,
    country: Option<String>,
    /// RGPD: owner must explicitly accept the Terms + Privacy Policy at signup.
    /// Kept optional for backwards compat during the deploy window, but the
    /// handler refuses without it.
    #[serde(default)]
    terms_accepted: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateProfileRequest {
    company_name: Option<String>,
    description: Option<String>,
    website: Option<String>,
    logo_url: Option<String>,
    industry: Option<String>,
    company_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InviteRequest {
    email: String,
}

#[derive(Debug, Deserialize)]
struct AcceptInviteRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct InvitePreviewQuery {
    token: String,
}

#[derive(Debug, Deserialize)]
struct RegisterAndAcceptRequest {
    token: String,
    first_name: String,
    last_name: String,
    password: String,
    #[serde(default)]
    terms_accepted: bool,
}

// ─── Routes ─────────────────────────────────────────────────────

// POST /api/enterprise/register
//
// Mirrors the shape of the candidate register handler in `routes::auth` —
// same rate-limit budget, same validators, same terms + verification-email +
// CSRF + audit + analytics wiring — with enterprise-specific fields laid on
// top (company profile + owner-tier membership).
async fn register_enterprise(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<RegisterEnterpriseRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ip = extract_ip(&headers);
    RateLimiter::check(
        &mut state.redis.clone(),
        "enterprise:register",
        &ip,
        5,
        3600,
    )
    .await?;

    if !body.terms_accepted {
        return Err(AppError::Validation(
            "You must accept the Terms of Service and Privacy Policy".to_string(),
        ));
    }

    // Same policy as the candidate register — see `routes::auth::validate_*`.
    validate_email(&body.email)?;
    validate_username(&body.username)?;
    crate::routes::auth::validate_password_pub(&body.password)?;
    validate_name(&body.first_name, "first_name")?;
    validate_name(&body.last_name, "last_name")?;

    if body.company_name.trim().is_empty() || body.company_name.len() > 200 {
        return Err(AppError::Validation(
            "company_name must be between 1 and 200 characters".to_string(),
        ));
    }
    let valid_sizes = ["1-10", "11-50", "51-200", "201-500", "501-1000", "1000+"];
    if !valid_sizes.contains(&body.company_size.as_str()) {
        return Err(AppError::Validation(format!(
            "company_size must be one of: {}",
            valid_sizes.join(", ")
        )));
    }

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

    // Create user (role=enterprise) with terms_accepted_at + password_changed_at
    // populated up-front, matching the candidate register handler.
    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, role, country, terms_accepted_at, password_changed_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'code', 'enterprise', $7, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&email_lower)
    .bind(&username_lower)
    .bind(&password_hash)
    .bind(body.first_name.trim())
    .bind(body.last_name.trim())
    .bind(&display_name)
    .bind(&body.country)
    .fetch_one(&state.db)
    .await?;

    let slug = slugify(&body.company_name);
    let enterprise: Enterprise = sqlx::query_as(
        r#"
        INSERT INTO enterprises (owner_id, company_name, slug, website, industry, company_size)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(user.id)
    .bind(body.company_name.trim())
    .bind(&slug)
    .bind(&body.website)
    .bind(&body.industry)
    .bind(&body.company_size)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO enterprise_members (enterprise_id, user_id, role, status, accepted_at) VALUES ($1, $2, 'owner', 'active', NOW())",
    )
    .bind(enterprise.id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    // Email verification — same 24 h Redis-backed token as the candidate flow.
    let verify_token = format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).replace('-', "");
    let mut redis = state.redis.clone();
    let () = redis
        .set_ex(
            &format!("email_verify:{verify_token}"),
            user.id.to_string(),
            24 * 60 * 60,
        )
        .await?;
    // Best-effort — a mail transport failure shouldn't roll back an otherwise
    // successful signup (the user can hit /auth/resend-verification later).
    if let Err(err) = state
        .email
        .send_email_verification(
            &user.email,
            &user.display_name,
            &verify_token,
            &state.config.base_url,
        )
        .await
    {
        tracing::warn!(user_id = %user.id, error = %err, "enterprise signup: verification mail failed");
    }

    // Sessions + cookies (access + refresh + CSRF).
    let access_token =
        AuthService::generate_access_token(user.id, &user.role, &state.config.jwt_secret)?;
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user.id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh_token) = SessionService::create(
        &state.db,
        user.id,
        Some(&ip),
        headers.get("user-agent").and_then(|v| v.to_str().ok()),
    )
    .await?;

    let access_cookie = format!(
        "access_token={access_token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=900"
    );
    let refresh_cookie = format!(
        "refresh_token={session_id}:{refresh_token}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
        7 * 24 * 60 * 60
    );
    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie(&csrf, "/api", 15 * 60);

    // Observability parity with the candidate register.
    if analytics_consent(&headers) {
        state.analytics.track(
            user.id,
            events::USER_SIGNUP,
            props(&[
                ("role", json!("enterprise")),
                ("company_size", json!(body.company_size)),
                ("country", json!(user.country)),
            ]),
        );
    }
    metrics::counter!(
        "skilluv_signups_total",
        "skill_domain" => "enterprise".to_string()
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
            metadata: Some(json!({
                "role": "enterprise",
                "enterprise_id": enterprise.id,
                "company_size": body.company_size,
            })),
            headers: Some(&headers),
        },
    )
    .await;

    Ok((
        StatusCode::CREATED,
        axum::response::AppendHeaders([
            (axum::http::header::SET_COOKIE, access_cookie),
            (axum::http::header::SET_COOKIE, refresh_cookie),
            (axum::http::header::SET_COOKIE, csrf_cookie),
        ]),
        // Return the full UserPrivate (same shape as /auth/register) so the
        // front sees email_verified / totp_enabled / role / title etc. and can
        // gate the layout without an extra /auth/me round-trip.
        Json(build_response(json!({
            "user": UserPrivate::from(user),
            "enterprise": enterprise,
            "csrf_token": csrf,
            "login_method": "password",
            "requires_totp_setup": true,
            "message": "Enterprise account created. Check your email to verify your address, then set up TOTP."
        }))),
    ))
}

// GET /api/enterprise/profile
async fn get_profile(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enterprise_members WHERE enterprise_id = $1 AND status = 'active'",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "enterprise": enterprise,
        "member_count": member_count,
    }))))
}

// PUT /api/enterprise/profile
async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise_owner(&state, &auth).await?;

    if let Some(ref size) = body.company_size {
        let valid_sizes = ["1-10", "11-50", "51-200", "201-500", "501-1000", "1000+"];
        if !valid_sizes.contains(&size.as_str()) {
            return Err(AppError::Validation(format!(
                "company_size must be one of: {}",
                valid_sizes.join(", ")
            )));
        }
    }

    let new_slug = body.company_name.as_ref().map(|n| slugify(n));

    let updated: Enterprise = sqlx::query_as(
        r#"
        UPDATE enterprises SET
            company_name = COALESCE($1, company_name),
            slug = COALESCE($2, slug),
            description = COALESCE($3, description),
            website = COALESCE($4, website),
            logo_url = COALESCE($5, logo_url),
            industry = COALESCE($6, industry),
            company_size = COALESCE($7, company_size),
            updated_at = NOW()
        WHERE id = $8
        RETURNING *
        "#,
    )
    .bind(&body.company_name)
    .bind(&new_slug)
    .bind(&body.description)
    .bind(&body.website)
    .bind(&body.logo_url)
    .bind(&body.industry)
    .bind(&body.company_size)
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "enterprise": updated }))))
}

// POST /api/enterprise/logo — upload company logo (multipart)
async fn upload_logo(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise_owner(&state, &auth).await?;

    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Invalid multipart data: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name != "logo" {
            continue;
        }

        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        if !["image/jpeg", "image/png", "image/webp"].contains(&content_type.as_str()) {
            return Err(AppError::Validation(
                "Logo must be JPEG, PNG, or WebP".to_string(),
            ));
        }

        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::Validation(format!("Failed to read file: {e}")))?;

        if data.len() > MAX_ENTERPRISE_LOGO_SIZE {
            return Err(AppError::Validation("Logo must be at most 2MB".to_string()));
        }

        file_data = Some((data.to_vec(), content_type));
        break;
    }

    let (data, content_type) = file_data.ok_or(AppError::Validation(
        "No 'logo' field found in upload".to_string(),
    ))?;

    // Purge previous variants so a JPEG → PNG re-upload doesn't leave the old
    // object dangling under a different extension.
    state.storage.delete_enterprise_logo(enterprise.id).await?;

    let key = state
        .storage
        .upload_enterprise_logo(enterprise.id, &data, &content_type)
        .await?;

    let logo_url = state.storage.enterprise_logo_url(&key);

    let updated: Enterprise = sqlx::query_as(
        "UPDATE enterprises SET logo_url = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
    )
    .bind(&logo_url)
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "logo_url": logo_url,
        "enterprise": updated,
    }))))
}

// DELETE /api/enterprise/logo
async fn delete_logo(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise_owner(&state, &auth).await?;

    state.storage.delete_enterprise_logo(enterprise.id).await?;

    let updated: Enterprise = sqlx::query_as(
        "UPDATE enterprises SET logo_url = NULL, updated_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "enterprise": updated }))))
}

// POST /api/enterprise/invite
async fn invite_recruiter(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<InviteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise_owner(&state, &auth).await?;

    let email = body.email.trim().to_lowercase();

    // If the user already exists and is already a member, short-circuit.
    let target_user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?;
    if let Some(ref user) = target_user {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM enterprise_members WHERE enterprise_id = $1 AND user_id = $2",
        )
        .bind(enterprise.id)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?;
        if let Some((status,)) = existing {
            if status == "active" {
                return Err(AppError::Validation(
                    "This user is already an active member".to_string(),
                ));
            }
        }
    }

    let invite_token = format!("{}{}", Uuid::new_v4(), Uuid::new_v4()).replace('-', "");

    // Store invite payload in Redis regardless of whether the user exists yet.
    // The membership row is created only when the invite is consumed.
    let payload = EnterpriseInvitePayload {
        enterprise_id: enterprise.id,
        email: email.clone(),
        invited_by: auth.user_id,
    };
    let mut redis = state.redis.clone();
    let key = enterprise_invite_key(&invite_token);
    let serialized = serde_json::to_string(&payload)
        .map_err(|e| AppError::Internal(format!("invite serialize: {e}")))?;
    let () = redis
        .set_ex(&key, &serialized, ENTERPRISE_INVITE_TTL_SECS)
        .await?;

    state
        .email
        .send_recruiter_invite(
            &email,
            &enterprise.company_name,
            &invite_token,
            &state.config.base_url,
        )
        .await?;

    Ok(Json(build_response(json!({
        "message": "Invitation sent",
        "invite_token": invite_token,
    }))))
}

// POST /api/enterprise/invite/accept
//
// Requires the invitee to be authenticated. Their email must match the invited
// email (case-insensitive), preventing anyone with the invite link from joining
// with an unrelated account. For invitees without an existing account, use the
// OAuth-with-invite flow instead (which creates the account and consumes the
// invite atomically).
async fn accept_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AcceptInviteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = state.redis.clone();
    let payload = peek_enterprise_invite(&mut redis, &body.token).await?;

    let user_email: (String,) = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    if user_email.0.to_lowercase() != payload.email.to_lowercase() {
        return Err(AppError::Forbidden);
    }

    attach_recruiter_to_enterprise(
        &state.db,
        payload.enterprise_id,
        auth.user_id,
        payload.invited_by,
    )
    .await?;

    delete_enterprise_invite(&mut redis, &body.token).await?;

    Ok(Json(build_response(json!({
        "message": "Invitation accepted. You are now a recruiter."
    }))))
}

// GET /api/enterprise/invite/preview?token=...
//
// Public: the token IS the secret. Returns just enough to render the landing
// page ("Join {company_name} as {email}") — no membership state, no user info,
// no PII beyond the invited email itself.
async fn invite_preview(
    State(state): State<AppState>,
    Query(query): Query<InvitePreviewQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = state.redis.clone();
    let payload = peek_enterprise_invite(&mut redis, &query.token).await?;

    let company: Option<(String,)> =
        sqlx::query_as("SELECT company_name FROM enterprises WHERE id = $1")
            .bind(payload.enterprise_id)
            .fetch_optional(&state.db)
            .await?;
    let company_name = company
        .map(|c| c.0)
        .ok_or(AppError::NotFound("Enterprise not found".to_string()))?;

    // Flag whether the invited email already has an account — the frontend uses
    // it to swap "Rejoindre" (create + accept) for "Se connecter" (log in +
    // accept) so we don't waste a user's time on the register form.
    let account_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
            .bind(payload.email.to_lowercase())
            .fetch_one(&state.db)
            .await?;

    Ok(Json(build_response(json!({
        "email": payload.email,
        "company_name": company_name,
        "account_exists": account_exists,
    }))))
}

// POST /api/enterprise/invite/register-and-accept
//
// Public: consumes the invite token, creates a recruiter account for the
// invited email, and attaches the membership in a single transaction. Email is
// marked verified because the invite was delivered by us to that inbox — the
// user proving they received the token IS the verification. The invite token
// is deleted only after the membership is committed.
async fn invite_register_and_accept(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<RegisterAndAcceptRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ip = extract_ip(&headers);
    RateLimiter::check(&mut state.redis.clone(), "invite:register", &ip, 10, 3600).await?;

    if !body.terms_accepted {
        return Err(AppError::Validation(
            "You must accept the Terms of Service and Privacy Policy".to_string(),
        ));
    }
    validate_name(&body.first_name, "first_name")?;
    validate_name(&body.last_name, "last_name")?;
    crate::routes::auth::validate_password_pub(&body.password)?;

    let mut redis = state.redis.clone();
    let payload = peek_enterprise_invite(&mut redis, &body.token).await?;
    let email_lower = payload.email.trim().to_lowercase();

    // Refuse if an account already exists for that email — the frontend flows
    // this case to the "j'ai déjà un compte" login path instead.
    let existing: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email_lower)
        .fetch_optional(&state.db)
        .await?;
    if existing.is_some() {
        return Err(AppError::Validation(
            "An account already exists for this email. Please sign in instead.".to_string(),
        ));
    }

    // Auto-generate a username from the email local-part. Retry with a numeric
    // suffix on collision — usernames must be unique but the invitee never
    // picked one, so we don't want to fail the whole flow on it.
    let base = email_lower
        .split('@')
        .next()
        .unwrap_or("recruiter")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect::<String>();
    let base = if base.len() < 3 {
        format!("recruiter_{}", &Uuid::new_v4().simple().to_string()[..8])
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut username_lower = String::new();
    for attempt in 0..8 {
        let try_name = if attempt == 0 {
            candidate.clone()
        } else {
            format!("{candidate}{}", &Uuid::new_v4().simple().to_string()[..4])
        };
        let taken: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
            .bind(&try_name)
            .fetch_optional(&state.db)
            .await?;
        if taken.is_none() {
            username_lower = try_name;
            break;
        }
        candidate = base.clone();
    }
    if username_lower.is_empty() {
        return Err(AppError::Internal(
            "Failed to generate a unique username".to_string(),
        ));
    }

    let password_hash = AuthService::hash_password(&body.password)?;
    let display_name = format!("{} {}", body.first_name.trim(), body.last_name.trim());

    // role='recruiter' up-front — the invite acceptance path never applies to
    // an existing candidate here (we bailed above), so we don't need
    // attach_recruiter_to_enterprise's role-guard. email_verified=true because
    // receiving the invite email is proof of ownership.
    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, role, email_verified, terms_accepted_at, password_changed_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'code', 'recruiter', TRUE, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&email_lower)
    .bind(&username_lower)
    .bind(&password_hash)
    .bind(body.first_name.trim())
    .bind(body.last_name.trim())
    .bind(&display_name)
    .fetch_one(&state.db)
    .await?;

    // Insert membership directly — no role mutation needed since we just
    // created the user with role='recruiter'.
    sqlx::query(
        r#"
        INSERT INTO enterprise_members (enterprise_id, user_id, role, invited_by, status, accepted_at)
        VALUES ($1, $2, 'recruiter', $3, 'active', NOW())
        "#,
    )
    .bind(payload.enterprise_id)
    .bind(user.id)
    .bind(payload.invited_by)
    .execute(&state.db)
    .await?;

    // Now safe to burn the token.
    delete_enterprise_invite(&mut redis, &body.token).await?;

    // Session + cookies (same shape as register_enterprise).
    let access_token =
        AuthService::generate_access_token(user.id, &user.role, &state.config.jwt_secret)?;
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user.id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh_token) = SessionService::create(
        &state.db,
        user.id,
        Some(&ip),
        headers.get("user-agent").and_then(|v| v.to_str().ok()),
    )
    .await?;

    let access_cookie = format!(
        "access_token={access_token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=900"
    );
    let refresh_cookie = format!(
        "refresh_token={session_id}:{refresh_token}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
        7 * 24 * 60 * 60
    );
    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie(&csrf, "/api", 15 * 60);

    if analytics_consent(&headers) {
        state.analytics.track(
            user.id,
            events::USER_SIGNUP,
            props(&[
                ("role", json!("recruiter")),
                ("via", json!("enterprise_invite")),
                ("enterprise_id", json!(payload.enterprise_id)),
            ]),
        );
    }
    audit::record(
        &state.db,
        AuditEntry {
            actor_type: ActorType::User,
            actor_id: Some(user.id),
            action: "user.signup",
            target_type: Some("user"),
            target_id: Some(user.id),
            metadata: Some(json!({
                "role": "recruiter",
                "via": "enterprise_invite",
                "enterprise_id": payload.enterprise_id,
            })),
            headers: Some(&headers),
        },
    )
    .await;

    Ok((
        StatusCode::CREATED,
        axum::response::AppendHeaders([
            (axum::http::header::SET_COOKIE, access_cookie),
            (axum::http::header::SET_COOKIE, refresh_cookie),
            (axum::http::header::SET_COOKIE, csrf_cookie),
        ]),
        Json(build_response(json!({
            "user": UserPrivate::from(user),
            "enterprise_id": payload.enterprise_id,
            "csrf_token": csrf,
            "login_method": "password",
            "requires_totp_setup": true,
            "message": "Account created and invitation accepted."
        }))),
    ))
}

// GET /api/enterprise/members
async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let members: Vec<EnterpriseMember> = sqlx::query_as(
        "SELECT * FROM enterprise_members WHERE enterprise_id = $1 ORDER BY invited_at",
    )
    .bind(enterprise.id)
    .fetch_all(&state.db)
    .await?;

    // Fetch user info for members
    let user_ids: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
    let users: Vec<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id, username, display_name, email FROM users WHERE id = ANY($1)")
            .bind(&user_ids)
            .fetch_all(&state.db)
            .await?;

    let user_map: std::collections::HashMap<Uuid, _> =
        users.into_iter().map(|u| (u.0, u)).collect();

    let members_with_info: Vec<serde_json::Value> = members
        .iter()
        .filter_map(|m| {
            let user = user_map.get(&m.user_id)?;
            Some(json!({
                "id": m.id,
                "user_id": m.user_id,
                "username": user.1,
                "display_name": user.2,
                "email": user.3,
                "role": m.role,
                "status": m.status,
                "invited_at": m.invited_at.to_rfc3339(),
                "accepted_at": m.accepted_at.map(|d| d.to_rfc3339()),
            }))
        })
        .collect();

    Ok(Json(build_response(json!({
        "members": members_with_info,
    }))))
}

// GET /api/enterprise/memberships
//
// Every enterprise the caller belongs to (active status only), enriched with
// the fields the workspace switcher needs. `is_active` marks the one currently
// selected via `active_enterprise` cookie (or the fallback used when the
// cookie is missing/invalid), so the UI can highlight it without a separate
// round-trip.
async fn list_memberships(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        enterprise_id: Uuid,
        company_name: String,
        slug: Option<String>,
        logo_url: Option<String>,
        role: String,
        accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT e.id AS enterprise_id, e.company_name, e.slug, e.logo_url,
               em.role, em.accepted_at
        FROM enterprises e
        JOIN enterprise_members em ON em.enterprise_id = e.id
        WHERE em.user_id = $1 AND em.status = 'active'
        ORDER BY em.accepted_at DESC NULLS LAST, em.invited_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    // Same tie-break rule as resolve_active_enterprise: cookie wins if it
    // still points to an active membership, otherwise the most recent one.
    let active_id = auth
        .active_enterprise_id
        .filter(|id| rows.iter().any(|r| r.enterprise_id == *id))
        .or_else(|| rows.first().map(|r| r.enterprise_id));

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "enterprise_id": r.enterprise_id,
                "company_name": r.company_name,
                "slug": r.slug,
                "logo_url": r.logo_url,
                "role": r.role,
                "accepted_at": r.accepted_at.map(|d| d.to_rfc3339()),
                "is_active": Some(r.enterprise_id) == active_id,
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "memberships": items,
        "active_enterprise_id": active_id,
    }))))
}

// POST /api/enterprise/switch/:enterprise_id
//
// Flip the workspace switcher: verify the caller is an active member of the
// target enterprise, then re-emit the `active_enterprise` cookie so future
// `require_enterprise` calls pin to that workspace.
async fn switch_enterprise(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let member: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND enterprise_id = $2 AND status = 'active'",
    )
    .bind(auth.user_id)
    .bind(enterprise_id)
    .fetch_optional(&state.db)
    .await?;
    if member.is_none() {
        return Err(AppError::Forbidden);
    }

    // 7-day TTL matches the refresh token — the switcher shouldn't reset every
    // time the access token renews. Path=/ so it flows to every enterprise
    // route (including SSR /api/* calls from the frontend).
    let cookie = format!(
        "active_enterprise={enterprise_id}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={}",
        7 * 24 * 60 * 60
    );

    Ok((
        axum::response::AppendHeaders([(axum::http::header::SET_COOKIE, cookie)]),
        Json(build_response(json!({
            "active_enterprise_id": enterprise_id,
        }))),
    ))
}

// DELETE /api/enterprise/members/:user_id
async fn revoke_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise_owner(&state, &auth).await?;

    if user_id == auth.user_id {
        return Err(AppError::Validation(
            "Cannot revoke yourself as owner".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE enterprise_members SET status = 'revoked' WHERE enterprise_id = $1 AND user_id = $2",
    )
    .bind(enterprise.id)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    // Reset user role back to 'user'
    sqlx::query("UPDATE users SET role = 'user', updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "message": "Member revoked"
    }))))
}
