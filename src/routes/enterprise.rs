use axum::extract::{Path, State};
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
use crate::models::{Enterprise, EnterpriseMember, User};
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
    serde_json::from_str(&raw).map_err(|e| {
        AppError::Internal(format!("Corrupted invite payload: {e}"))
    })
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
        .route("/enterprise/invite", post(invite_recruiter))
        .route("/enterprise/invite/accept", post(accept_invite))
        .route("/enterprise/members", get(list_members))
        .route("/enterprise/members/{user_id}", delete(revoke_member))
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

    // Enforce mandatory TOTP for enterprise/recruiter roles before granting access,
    // except when the session was minted via SSO — the external IdP is responsible
    // for MFA in that case, so requiring an additional Skilluv-side TOTP would be
    // a pointless double 2FA.
    if matches!(auth.role.as_str(), "enterprise" | "recruiter")
        && auth.login_method != "sso"
        && !totp_enabled
    {
        return Err(AppError::TotpSetupRequired);
    }

    let enterprise: Option<Enterprise> = sqlx::query_as(
        "SELECT e.* FROM enterprises e JOIN enterprise_members em ON em.enterprise_id = e.id WHERE em.user_id = $1 AND em.status = 'active'",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;

    enterprise.ok_or(AppError::NotFound("Enterprise not found".to_string()))
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
    RateLimiter::check(&mut state.redis.clone(), "enterprise:register", &ip, 5, 3600).await?;

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
    let (session_id, refresh_token) =
        SessionService::create(&state.db, user.id, Some(&ip), headers.get("user-agent").and_then(|v| v.to_str().ok())).await?;

    let access_cookie = format!(
        "access_token={access_token}; HttpOnly; Secure; SameSite=Strict; Path=/api; Max-Age=900"
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
        Json(build_response(json!({
            "user": {
                "id": user.id,
                "email": user.email,
                "username": user.username,
                "display_name": display_name,
                "role": "enterprise",
            },
            "enterprise": enterprise,
            "csrf_token": csrf,
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
    let () = redis.set_ex(&key, &serialized, ENTERPRISE_INVITE_TTL_SECS).await?;

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
