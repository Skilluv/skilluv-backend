use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::services::AuthService;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: String,
    /// How the current session was authenticated (from the JWT claim). Read by
    /// `require_enterprise` to bypass mandatory-TOTP when set to "sso" — the
    /// external IdP is responsible for MFA in that case.
    pub login_method: String,
    /// UUID of the enterprise the user has selected in the workspace switcher
    /// (`active_enterprise` cookie). `None` when the user has never picked one
    /// or was signed out — callers should fall back to the most recent
    /// membership. Also `None` for non-enterprise personas.
    pub active_enterprise_id: Option<Uuid>,
}

fn parse_active_enterprise(cookie_header: &str) -> Option<Uuid> {
    cookie_header
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("active_enterprise="))
        .and_then(|s| s.strip_prefix("active_enterprise="))
        .and_then(|v| Uuid::parse_str(v).ok())
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let cookie_header = parts
            .headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        // Admin app emits `admin_access_token`; public app emits `access_token`.
        // The JWT signing key is shared so verification is identical — the
        // separate cookie name is the isolation vector (different Set-Cookie
        // scope in the browser jar, defense-in-depth against XSS-hijack of an
        // admin session by JS running on the public origin).
        let token = cookie_header
            .split(';')
            .map(|s| s.trim())
            .find_map(|s| s.strip_prefix("admin_access_token="))
            .or_else(|| {
                cookie_header
                    .split(';')
                    .map(|s| s.trim())
                    .find_map(|s| s.strip_prefix("access_token="))
            })
            .ok_or(AppError::Unauthorized)?;

        let claims = AuthService::verify_access_token(token, &state.config.jwt_secret)?;

        let user_id = claims
            .sub
            .parse::<Uuid>()
            .map_err(|_| AppError::Unauthorized)?;

        // Tag the current Sentry scope so any error emitted later in the handler carries
        // the user_id (helps triage). Cheap no-op when Sentry is disabled.
        sentry::configure_scope(|scope| {
            scope.set_user(Some(sentry::User {
                id: Some(user_id.to_string()),
                ..Default::default()
            }));
            scope.set_tag("user.role", &claims.role);
        });

        Ok(AuthUser {
            user_id,
            role: claims.role,
            login_method: claims
                .login_method
                .unwrap_or_else(|| "password".to_string()),
            active_enterprise_id: parse_active_enterprise(cookie_header),
        })
    }
}

/// Extracteur du tenant courant — Phase 5.9.
///
/// Résolu depuis (dans l'ordre) :
///   1. header `X-Skilluv-Tenant` (slug)
///   2. sous-domaine du header `Host` (ex: `acme.skilluv.com` → tenant `acme`)
///   3. tenant racine (`00000000-...-0001`) par défaut
///
/// Ne rejette jamais — un tenant est toujours résolu, au pire c'est le racine.
#[derive(Debug, Clone, Copy)]
pub struct TenantContext {
    pub tenant_id: Uuid,
}

impl FromRequestParts<AppState> for TenantContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let tenant_id = crate::routes::resolve_tenant_from_headers(&state.db, &parts.headers)
            .await
            .unwrap_or(crate::routes::ROOT_TENANT_ID);
        Ok(TenantContext { tenant_id })
    }
}

/// Same as `AuthUser` but also enforces `profile_completed = true`.
/// Use on write endpoints (submissions, posts, DMs, follows...) so that OAuth/magic-link
/// signups can't participate in the product until they've picked a skill_domain and
/// accepted the terms.
#[derive(Debug, Clone)]
pub struct AuthUserComplete {
    pub user_id: Uuid,
    pub role: String,
}

impl FromRequestParts<AppState> for AuthUserComplete {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        let row: Option<(Option<String>, Option<chrono::DateTime<chrono::Utc>>, bool)> =
            sqlx::query_as(
                "SELECT skill_domain, terms_accepted_at, email_verified FROM users WHERE id = $1",
            )
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
        let (skill_domain, terms_accepted_at, email_verified) =
            row.ok_or(AppError::Unauthorized)?;
        // Gate write endpoints on verified email — a bounced/typo'd address
        // shouldn't be able to spam messages, invites, submissions, etc. The
        // gate is bypassed for enterprise SSO sessions since the IdP already
        // asserted email ownership (see login_method wiring).
        if !email_verified && auth.login_method != "sso" {
            return Err(AppError::EmailVerificationRequired);
        }
        if skill_domain.is_none() || terms_accepted_at.is_none() {
            return Err(AppError::ProfileIncomplete);
        }
        Ok(AuthUserComplete {
            user_id: auth.user_id,
            role: auth.role,
        })
    }
}

/// Optional authentication extractor — never rejects.
/// Returns `Some(AuthUser)` if a valid token is present, `None` otherwise.
#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<AuthUser>);

impl FromRequestParts<AppState> for OptionalAuth {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = extract_auth(parts, state);
        Ok(OptionalAuth(auth))
    }
}

fn extract_auth(parts: &Parts, state: &AppState) -> Option<AuthUser> {
    let cookie_header = parts.headers.get("cookie").and_then(|v| v.to_str().ok())?;

    // Same admin-first / public-fallback rule as the mandatory extractor.
    let token = cookie_header
        .split(';')
        .map(|s| s.trim())
        .find_map(|s| s.strip_prefix("admin_access_token="))
        .or_else(|| {
            cookie_header
                .split(';')
                .map(|s| s.trim())
                .find_map(|s| s.strip_prefix("access_token="))
        })?;

    let claims = AuthService::verify_access_token(token, &state.config.jwt_secret).ok()?;
    let user_id = claims.sub.parse::<Uuid>().ok()?;

    Some(AuthUser {
        user_id,
        role: claims.role,
        login_method: claims
            .login_method
            .unwrap_or_else(|| "password".to_string()),
        active_enterprise_id: parse_active_enterprise(cookie_header),
    })
}
