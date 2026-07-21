//! Enterprise B2B SSO — OIDC config CRUD + login flow.
//!
//! Endpoints:
//! - POST   /api/enterprise/sso/config           (owner) upsert IdP config
//! - GET    /api/enterprise/sso/config           (owner) get current config (secret masked)
//! - DELETE /api/enterprise/sso/config           (owner) disable SSO
//! - GET    /api/enterprise/sso/discover?email=  (public) discovery for frontend
//! - GET    /api/enterprise/sso/{slug}/start     (public) redirect to IdP authorize
//! - GET    /api/enterprise/sso/{slug}/callback  (public) IdP code exchange + session

use axum::extract::{Path, Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::response::{AppendHeaders, IntoResponse, Redirect};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
use openidconnect::reqwest::async_http_client;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Enterprise;
use crate::services::enterprise_sso as sso;
use crate::services::{AuthService, SessionService};

pub fn enterprise_sso_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/sso/config", post(upsert_config))
        .route("/enterprise/sso/config", get(get_config))
        .route("/enterprise/sso/config", delete(disable_config))
        .route("/enterprise/sso/discover", get(discover))
        .route("/enterprise/sso/{slug}/start", get(start))
        .route("/enterprise/sso/{slug}/callback", get(callback))
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

fn require_key(state: &AppState) -> Result<[u8; 32], AppError> {
    state
        .config
        .sso_encryption_key
        .ok_or_else(|| AppError::Internal("SSO_ENCRYPTION_KEY not configured".into()))
}

fn email_domain(email: &str) -> Option<String> {
    email
        .trim()
        .to_lowercase()
        .split_once('@')
        .map(|(_, d)| d.to_string())
}

// ─── Config CRUD (owner-only) ────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UpsertConfigBody {
    issuer: String,
    client_id: String,
    client_secret: String,
    email_domains: Vec<String>,
    #[serde(default)]
    enforce_sso: bool,
    #[serde(default = "default_true")]
    auto_provision: bool,
    #[serde(default = "default_role")]
    default_role: String,
}

fn default_true() -> bool {
    true
}
fn default_role() -> String {
    "recruiter".to_string()
}

async fn require_enterprise_owner_direct(
    state: &AppState,
    auth: &AuthUser,
) -> Result<Enterprise, AppError> {
    crate::routes::enterprise::require_enterprise_owner_pub(state, auth).await
}

async fn upsert_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpsertConfigBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = require_enterprise_owner_direct(&state, &auth).await?;
    let key = require_key(&state)?;

    // OIDC spec requires HTTPS. Local development / integration tests are
    // allowed to point at http://127.0.0.1 or http://localhost so a mock IdP
    // (or Keycloak in Docker) can be exercised end-to-end.
    let issuer = body.issuer.trim();
    let is_https = issuer.starts_with("https://");
    let is_local_http =
        issuer.starts_with("http://127.0.0.1") || issuer.starts_with("http://localhost");
    if issuer.is_empty() || !(is_https || is_local_http) {
        return Err(AppError::Validation(
            "issuer must be an https URL (or http://localhost / http://127.0.0.1 for local testing)".into(),
        ));
    }
    if body.client_id.trim().is_empty() {
        return Err(AppError::Validation("client_id is required".into()));
    }
    if body.client_secret.trim().is_empty() {
        return Err(AppError::Validation("client_secret is required".into()));
    }
    if body.email_domains.is_empty() {
        return Err(AppError::Validation(
            "at least one email domain is required".into(),
        ));
    }
    if !matches!(body.default_role.as_str(), "recruiter" | "enterprise") {
        return Err(AppError::Validation(
            "default_role must be 'recruiter' or 'enterprise'".into(),
        ));
    }

    let domains: Vec<String> = body
        .email_domains
        .iter()
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !d.is_empty())
        .collect();

    let (ct, nonce) = sso::encrypt_secret(&key, &body.client_secret)?;

    let row = sso::upsert(
        &state.db,
        sso::UpsertConfig {
            enterprise_id: enterprise.id,
            issuer: body.issuer.trim(),
            client_id: body.client_id.trim(),
            client_secret_encrypted: &ct,
            client_secret_nonce: &nonce,
            email_domains: &domains,
            enforce_sso: body.enforce_sso,
            auto_provision: body.auto_provision,
            default_role: &body.default_role,
        },
    )
    .await?;

    Ok(Json(build_response(json!({
        "config": redact(&row),
        "redirect_uri": callback_url(&state, &enterprise.slug),
    }))))
}

async fn get_config(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = require_enterprise_owner_direct(&state, &auth).await?;
    let cfg = sso::get_by_enterprise(&state.db, enterprise.id).await?;
    match cfg {
        Some(row) => Ok(Json(build_response(json!({
            "config": redact(&row),
            "redirect_uri": callback_url(&state, &enterprise.slug),
        })))),
        None => Ok(Json(build_response(json!({ "config": null })))),
    }
}

async fn disable_config(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = require_enterprise_owner_direct(&state, &auth).await?;
    sso::disable(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "disabled": true }))))
}

fn redact(row: &sso::SsoConfigRow) -> Value {
    json!({
        "enterprise_id": row.enterprise_id,
        "issuer": row.issuer,
        "client_id": row.client_id,
        "client_secret": "***REDACTED***",
        "email_domains": row.email_domains,
        "enforce_sso": row.enforce_sso,
        "auto_provision": row.auto_provision,
        "default_role": row.default_role,
        "disabled_at": row.disabled_at.map(|d| d.to_rfc3339()),
        "created_at": row.created_at.to_rfc3339(),
        "updated_at": row.updated_at.to_rfc3339(),
    })
}

// ─── Discovery (public) ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DiscoverQuery {
    email: String,
}

async fn discover(
    State(state): State<AppState>,
    Query(q): Query<DiscoverQuery>,
) -> Result<Json<Value>, AppError> {
    let Some(domain) = email_domain(&q.email) else {
        return Ok(Json(build_response(json!({ "sso_available": false }))));
    };
    let found = sso::find_by_email_domain(&state.db, &domain).await?;
    match found {
        Some((_cfg, slug)) => Ok(Json(build_response(json!({
            "sso_available": true,
            "start_url": start_url(&state, &slug),
        })))),
        None => Ok(Json(build_response(json!({ "sso_available": false })))),
    }
}

// ─── OIDC login flow (public) ────────────────────────────────────

fn callback_url(state: &AppState, slug: &str) -> String {
    format!(
        "{}/api/enterprise/sso/{}/callback",
        state.config.base_url, slug
    )
}

fn start_url(state: &AppState, slug: &str) -> String {
    format!(
        "{}/api/enterprise/sso/{}/start",
        state.config.base_url, slug
    )
}

async fn build_client(
    row: &sso::SsoConfigRow,
    key: &[u8; 32],
    redirect_uri: String,
) -> Result<CoreClient, AppError> {
    let secret_plain =
        sso::decrypt_secret(key, &row.client_secret_encrypted, &row.client_secret_nonce)?;

    let issuer_url = IssuerUrl::new(row.issuer.clone())
        .map_err(|e| AppError::Internal(format!("invalid issuer URL: {e}")))?;

    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, async_http_client)
        .await
        .map_err(|e| AppError::Internal(format!("OIDC discovery failed: {e}")))?;

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(row.client_id.clone()),
        Some(ClientSecret::new(secret_plain)),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect_uri)
            .map_err(|e| AppError::Internal(format!("invalid redirect URL: {e}")))?,
    );
    Ok(client)
}

async fn start(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Redirect, AppError> {
    let key = require_key(&state)?;
    let cfg = sso::get_by_slug(&state.db, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound("SSO not configured for this enterprise".into()))?;

    let client = build_client(&cfg, &key, callback_url(&state, &slug)).await?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf, nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let login_state = sso::SsoLoginState {
        enterprise_id: cfg.enterprise_id,
        enterprise_slug: slug.clone(),
        pkce_verifier: pkce_verifier.secret().to_string(),
        nonce: nonce.secret().to_string(),
    };
    // We key our Redis state by the CSRF token — the IdP echoes it back on callback.
    let mut redis = state.redis.clone();
    let key_token = csrf.secret().to_string();
    let payload = serde_json::to_string(&login_state)
        .map_err(|e| AppError::Internal(format!("state serialize: {e}")))?;
    use redis::AsyncCommands;
    let () = redis
        .set_ex(
            &format!("sso_state:{key_token}"),
            payload,
            sso::SSO_STATE_TTL_SECS,
        )
        .await?;

    Ok(Redirect::to(auth_url.as_str()))
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<CallbackQuery>,
) -> Result<axum::response::Response, AppError> {
    let key = require_key(&state)?;
    let cfg = sso::get_by_slug(&state.db, &slug)
        .await?
        .ok_or_else(|| AppError::NotFound("SSO not configured for this enterprise".into()))?;

    // Retrieve login state by the CSRF token echoed in `state`.
    let mut redis = state.redis.clone();
    let login_state = sso::consume_login_state(&mut redis, &q.state).await?;
    if login_state.enterprise_id != cfg.enterprise_id {
        return Err(AppError::Unauthorized);
    }

    let client = build_client(&cfg, &key, callback_url(&state, &slug)).await?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(q.code))
        .set_pkce_verifier(PkceCodeVerifier::new(login_state.pkce_verifier))
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::Internal(format!("OIDC token exchange failed: {e}")))?;

    let id_token = token_response
        .id_token()
        .ok_or_else(|| AppError::Unauthorized)?;

    let claims = id_token
        .claims(&client.id_token_verifier(), &Nonce::new(login_state.nonce))
        .map_err(|_| AppError::Unauthorized)?;

    let email = claims
        .email()
        .ok_or_else(|| AppError::Validation("IdP did not return an email claim".into()))?
        .to_string();

    // Refuse unverified emails — critical: matches how we treat OAuth signups.
    if !claims.email_verified().unwrap_or(false) {
        return Err(AppError::Forbidden);
    }

    let display_name = claims
        .name()
        .and_then(|localised| localised.get(None).map(|s| s.to_string()));

    // Gate JIT provisioning on the config flag.
    let user_id = if cfg.auto_provision {
        sso::provision_from_sso(
            &state.db,
            cfg.enterprise_id,
            &email,
            display_name.as_deref(),
            &cfg.default_role,
        )
        .await?
    } else {
        // Non-provisioning: the user must already exist AND be a member of this enterprise.
        let existing: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM users WHERE LOWER(email) = LOWER($1)")
                .bind(&email)
                .fetch_optional(&state.db)
                .await?;
        let (uid,) = existing.ok_or(AppError::Forbidden)?;

        let member: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM enterprise_members WHERE enterprise_id = $1 AND user_id = $2",
        )
        .bind(cfg.enterprise_id)
        .bind(uid)
        .fetch_optional(&state.db)
        .await?;
        match member {
            Some((status,)) if status == "active" => {}
            _ => return Err(AppError::Forbidden),
        }
        uid
    };

    // Load role for the JWT.
    let user_role: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    // Mint session with login_method='sso' so the TOTP gate can honour the SSO trust.
    let access = AuthService::generate_access_token_with_method(
        user_id,
        &user_role.0,
        "sso",
        &state.config.jwt_secret,
    )?;
    let (session_id, refresh) =
        SessionService::create_with_method(&state.db, user_id, None, None, "sso").await?;

    let access_cookie = format!(
        "access_token={access}; HttpOnly; Secure; SameSite=Lax; Path=/api; Max-Age={}",
        15 * 60
    );
    let refresh_cookie = format!(
        "refresh_token={session_id}:{refresh}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
        7 * 24 * 60 * 60
    );

    // Redirect back to the frontend home (or a post-login destination). The SPA
    // sees the cookies and knows the user is authenticated.
    let target = format!("{}/", state.config.base_url);
    Ok((
        AppendHeaders([
            (SET_COOKIE, access_cookie),
            (SET_COOKIE, refresh_cookie),
            (LOCATION, target),
        ]),
        axum::http::StatusCode::FOUND,
    )
        .into_response())
}
