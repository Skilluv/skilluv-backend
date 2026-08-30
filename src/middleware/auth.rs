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
///   2. sous-domaine du header `Host` (ex: `acme.skill-uv.com` → tenant `acme`)
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

// ═══════════════════════════════════════════════════════════════════
// A caller who may be a browser or a program
// ═══════════════════════════════════════════════════════════════════

/// Somebody acting on their own behalf, whether from a session or a key.
///
/// ## Why this exists
///
/// `AuthUser` reads a session cookie. A cookie is a browser thing: an editor
/// extension, a CLI, a CI job or a GitHub Action has no session to present, so
/// every route guarded by `AuthUser` alone is a route a program cannot reach.
///
/// That is the gap behind SKI-172. The ticket asked for a VS Code extension
/// that submits security findings from the editor; the extension was never the
/// hard part. `POST /api/security/reports` takes `AuthUser`, so nothing
/// without a browser could call it, whatever the client was written in.
///
/// ## Why an extractor and not a second route
///
/// A `/api/v1/security/reports` twin would be two handlers to keep in step on
/// rate limits, validation and the shape of the answer — and the day they
/// diverge, one of them is the lenient one. One handler, two ways in.
///
/// ## The scope is not optional
///
/// A key reaches this only if it carries the named permission. `permissions`
/// on `api_keys` already means "what the holder may do on their own behalf",
/// which is exactly the question here, and `has_permission` already honours a
/// `*`. A key minted for reading a profile cannot file a vulnerability report.
pub struct Caller {
    pub user_id: Uuid,
    /// `None` for a session. `Some(id)` names the key, so a route that wants
    /// to log or rate-limit per key can, and so an audit can answer "which
    /// key filed this".
    pub api_key_id: Option<Uuid>,
}

impl Caller {
    /// Read a caller, accepting a session cookie or a key carrying `scope`.
    ///
    /// The session is tried first: it is the common case, it costs no query
    /// beyond what `AuthUser` already does, and a person in a browser should
    /// never be refused because a key would have needed a scope.
    pub async fn with_scope(
        parts: &mut Parts,
        state: &AppState,
        scope: &str,
    ) -> Result<Self, AppError> {
        if let Ok(auth) = AuthUser::from_request_parts(parts, state).await {
            return Ok(Self {
                user_id: auth.user_id,
                api_key_id: None,
            });
        }

        let key = crate::middleware::api_key::ApiKeyAuth::from_request_parts(parts, state).await?;
        // `require_permission` answers 403, not 401, and that distinction is
        // the whole message: the key is valid and it is not allowed to do
        // this. Answering 401 would send somebody looking for a bad token.
        key.require_permission(scope)?;
        Ok(Self {
            user_id: key.user_id,
            api_key_id: Some(key.key_id),
        })
    }
}

/// The scope a key needs to file a vulnerability report.
///
/// Named here rather than spelled at the call site so that the string a person
/// pastes into a key and the string the route checks are the same one.
pub const SCOPE_SECURITY_REPORT: &str = "security:report";

impl FromRequestParts<AppState> for Caller {
    type Rejection = AppError;

    /// The default scope is the security one, because that is the only route
    /// wired to `Caller` today and a default that silently allowed *any* scope
    /// would be the wrong kind of convenient.
    ///
    /// A second route wanting a different scope should call
    /// [`Caller::with_scope`] from its own extractor rather than widen this.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Caller::with_scope(parts, state, SCOPE_SECURITY_REPORT).await
    }
}
