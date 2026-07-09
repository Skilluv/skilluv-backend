//! OAuth login + linking routes — Phase 3.1 + 3.2.
//!
//! Unified across GitHub / Google / LinkedIn. The GitHub-specific token storage
//! (Sprint 5) remains in `routes::github` for the repo-sync flow ; this module
//! handles login/signup + account linking uniformly.

use axum::extract::{Path, Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse, Redirect};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::enterprise::attach_recruiter_to_enterprise;
use crate::services::{AuthService, SessionService};
use crate::services::oauth::{
    self, OAuthProfile, OAuthState, google as gp, linkedin as lp,
};

#[derive(Deserialize, Default)]
struct StartQuery {
    /// Optional enterprise recruiter invite token. When present, the OAuth
    /// callback consumes the invite and attaches the user to the enterprise
    /// (rejecting the flow if the provider email doesn't match the invited email).
    invite_token: Option<String>,
}

pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        // Provider agnostic
        .route("/auth/me/oauth-providers", get(list_my_providers))
        .route("/auth/me/oauth-providers/{provider}", axum::routing::delete(unlink_provider))
        // Google
        .route("/auth/google/start", get(google_start))
        .route("/auth/google/link", get(google_link_start))
        .route("/auth/google/callback", get(google_callback))
        // LinkedIn
        .route("/auth/linkedin/start", get(linkedin_start))
        .route("/auth/linkedin/link", get(linkedin_link_start))
        .route("/auth/linkedin/callback", get(linkedin_callback))
        // GitHub login (reuse Sprint 5 config): `/auth/github/login` = user-facing signup/login,
        // versus `/auth/github/start` which is for repo-sync linking (existing).
        .route("/auth/github/login", get(github_login_start))
        .route("/auth/github/login/callback", get(github_login_callback))
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

// ─── Provider listing / unlinking ────────────────────────────────

async fn list_my_providers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let list = oauth::list_for_user(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "providers": list }))))
}

async fn unlink_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
) -> Result<Json<Value>, AppError> {
    if !oauth::VALID_PROVIDERS.contains(&provider.as_str()) {
        return Err(AppError::Validation("unknown provider".into()));
    }
    oauth::unlink(&state.db, auth.user_id, &provider).await?;
    Ok(Json(build_response(json!({ "unlinked": true, "provider": provider }))))
}

// ─── Google ──────────────────────────────────────────────────────

async fn google_start(
    State(state): State<AppState>,
    Query(q): Query<StartQuery>,
) -> Result<Redirect, AppError> {
    start_flow(&state, "google", None, q.invite_token).await
}

async fn google_link_start(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Redirect, AppError> {
    start_flow(&state, "google", Some(auth.user_id), None).await
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn google_callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = gp::Config::from_env()
        .ok_or(AppError::Internal("Google OAuth not configured".into()))?;
    handle_callback(&state, "google", &q.state, |code| async move {
        gp::fetch_profile(&cfg, &code).await
    }, q.code).await
}

// ─── LinkedIn ────────────────────────────────────────────────────

async fn linkedin_start(
    State(state): State<AppState>,
    Query(q): Query<StartQuery>,
) -> Result<Redirect, AppError> {
    start_flow(&state, "linkedin", None, q.invite_token).await
}

async fn linkedin_link_start(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Redirect, AppError> {
    start_flow(&state, "linkedin", Some(auth.user_id), None).await
}

async fn linkedin_callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = lp::Config::from_env()
        .ok_or(AppError::Internal("LinkedIn OAuth not configured".into()))?;
    handle_callback(&state, "linkedin", &q.state, |code| async move {
        lp::fetch_profile(&cfg, &code).await
    }, q.code).await
}

// ─── GitHub login (distinct from Sprint 5 repo-sync flow) ────────

async fn github_login_start(
    State(state): State<AppState>,
    Query(q): Query<StartQuery>,
) -> Result<Redirect, AppError> {
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| AppError::Internal("GITHUB_CLIENT_ID not set".into()))?;
    let redirect_uri = std::env::var("GITHUB_LOGIN_REDIRECT_URI")
        .or_else(|_| std::env::var("GITHUB_REDIRECT_URI"))
        .map_err(|_| AppError::Internal("GITHUB_LOGIN_REDIRECT_URI not set".into()))?;
    let mut redis = state.redis.clone();
    let token = oauth::store_state(
        &mut redis,
        &OAuthState {
            provider: "github".into(),
            user_id: None,
            intent: "signup_login".into(),
            redirect_after: None,
            invite_token: q.invite_token,
        },
    )
    .await?;
    let url = crate::services::github::build_authorize_url(&client_id, &redirect_uri, &token);
    Ok(Redirect::to(&url))
}

async fn github_login_callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| AppError::Internal("GITHUB_CLIENT_ID not set".into()))?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| AppError::Internal("GITHUB_CLIENT_SECRET not set".into()))?;
    let redirect_uri = std::env::var("GITHUB_LOGIN_REDIRECT_URI")
        .or_else(|_| std::env::var("GITHUB_REDIRECT_URI"))
        .map_err(|_| AppError::Internal("GITHUB_LOGIN_REDIRECT_URI not set".into()))?;

    let mut redis = state.redis.clone();
    let oauth_state = oauth::consume_state(&mut redis, &q.state).await?;

    let (token, _scopes) = crate::services::github::exchange_code(
        &client_id, &client_secret, &redirect_uri, &q.code,
    )
    .await?;
    let gh_user = crate::services::github::fetch_user(&token).await?;
    let profile = OAuthProfile {
        provider: "github",
        provider_user_id: gh_user.id.to_string(),
        email: None,          // GitHub /user endpoint may hide it ; consider a second call to /user/emails
        email_verified: false,
        display_name: gh_user.name.clone(),
        avatar_url: gh_user.avatar_url,
        username: Some(gh_user.login.clone()),
    };
    finalise_login_or_link(&state, &oauth_state, profile).await
}

// ─── Common flow helpers ─────────────────────────────────────────

async fn start_flow(
    state: &AppState,
    provider: &str,
    linking_user: Option<Uuid>,
    invite_token: Option<String>,
) -> Result<Redirect, AppError> {
    let mut redis = state.redis.clone();
    let token = oauth::store_state(
        &mut redis,
        &OAuthState {
            provider: provider.into(),
            user_id: linking_user,
            intent: if linking_user.is_some() { "link".into() } else { "signup_login".into() },
            redirect_after: None,
            invite_token,
        },
    )
    .await?;
    let url = match provider {
        "google" => {
            let cfg = gp::Config::from_env()
                .ok_or(AppError::Internal("Google OAuth not configured".into()))?;
            gp::authorize_url(&cfg, &token)
        }
        "linkedin" => {
            let cfg = lp::Config::from_env()
                .ok_or(AppError::Internal("LinkedIn OAuth not configured".into()))?;
            lp::authorize_url(&cfg, &token)
        }
        _ => return Err(AppError::Validation("unsupported provider".into())),
    };
    Ok(Redirect::to(&url))
}

async fn handle_callback<F, Fut>(
    state: &AppState,
    provider: &'static str,
    state_token: &str,
    fetch: F,
    code: String,
) -> Result<axum::response::Response, AppError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<OAuthProfile, AppError>>,
{
    let mut redis = state.redis.clone();
    let oauth_state = oauth::consume_state(&mut redis, state_token).await?;
    if oauth_state.provider != provider {
        return Err(AppError::Unauthorized);
    }
    let profile = fetch(code).await?;
    finalise_login_or_link(state, &oauth_state, profile).await
}

async fn finalise_login_or_link(
    state: &AppState,
    oauth_state: &OAuthState,
    profile: OAuthProfile,
) -> Result<axum::response::Response, AppError> {
    match oauth_state.intent.as_str() {
        "link" => {
            let user_id = oauth_state
                .user_id
                .ok_or(AppError::Internal("link intent without user_id".into()))?;
            oauth::upsert_link(&state.db, user_id, &profile).await?;
            metrics::counter!(
                "skilluv_oauth_links_total",
                "provider" => profile.provider.to_string()
            )
            .increment(1);
            Ok(Json(build_response(json!({
                "linked": true,
                "provider": profile.provider,
            })))
            .into_response())
        }
        _ => {
            // If this flow carries an enterprise invite, verify the provider-returned
            // email matches the invited email BEFORE creating any account.
            let invite = if let Some(token) = oauth_state.invite_token.as_deref() {
                let mut redis = state.redis.clone();
                let payload = crate::routes::enterprise::peek_enterprise_invite(
                    &mut redis, token,
                )
                .await?;
                let provider_email = profile
                    .email
                    .as_deref()
                    .ok_or(AppError::Validation(
                        "Provider did not return an email — cannot verify invite.".into(),
                    ))?
                    .trim()
                    .to_lowercase();
                if provider_email != payload.email.to_lowercase() {
                    return Err(AppError::Forbidden);
                }
                Some((token.to_string(), payload))
            } else {
                None
            };

            // Signup / login
            let existing = oauth::find_user_for_profile(&state.db, &profile).await?;
            let user_id = if let Some(uid) = existing {
                oauth::upsert_link(&state.db, uid, &profile).await?;
                uid
            } else {
                let uid = create_user_from_profile(&state.db, &profile).await?;
                oauth::upsert_link(&state.db, uid, &profile).await?;
                metrics::counter!(
                    "skilluv_signups_total",
                    "skill_domain" => "code".to_string()
                )
                .increment(1);
                uid
            };

            // If an enterprise invite was carried through this flow, materialise the
            // recruiter membership now that the user exists, then invalidate the token.
            if let Some((token, payload)) = invite {
                attach_recruiter_to_enterprise(
                    &state.db,
                    payload.enterprise_id,
                    user_id,
                    payload.invited_by,
                )
                .await?;
                let mut redis = state.redis.clone();
                crate::routes::enterprise::delete_enterprise_invite(&mut redis, &token).await?;
            }

            // Mint access token cookie
            let user_row: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&state.db)
                .await?;
            // OAuth signup/login goes through the consumer providers (Google,
            // LinkedIn, GitHub) — distinct from enterprise SSO which is
            // labelled 'sso' in enterprise_sso.rs. Keeping them separate lets
            // require_enterprise apply the enforce_sso rule without treating
            // "signed in with my personal Google" as an enterprise IdP proof.
            let access = AuthService::generate_access_token_with_method(
                user_id,
                &user_row.0,
                "oauth",
                &state.config.jwt_secret,
            )?;
            let (session_id, refresh) =
                SessionService::create_with_method(&state.db, user_id, None, None, "oauth").await?;
            let cookie = build_cookie("access_token", &access, 15 * 60, "/");
            let refresh_cookie = format!(
                "refresh_token={session_id}:{refresh}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
                7 * 24 * 60 * 60
            );

            metrics::counter!(
                "skilluv_oauth_logins_total",
                "provider" => profile.provider.to_string()
            )
            .increment(1);

            Ok((
                AppendHeaders([
                    (SET_COOKIE, cookie),
                    (SET_COOKIE, refresh_cookie),
                ]),
                Json(build_response(json!({
                    "user_id": user_id,
                    "provider": profile.provider,
                    "login_method": "oauth",
                }))),
            )
                .into_response())
        }
    }
}

async fn create_user_from_profile(
    db: &sqlx::PgPool,
    profile: &OAuthProfile,
) -> Result<Uuid, AppError> {
    let email = profile
        .email
        .as_ref()
        .ok_or(AppError::Validation(
            "Provider did not return an email — cannot create account. Sign in with password first and link.".into(),
        ))?
        .trim()
        .to_lowercase();
    let base_username = profile
        .username
        .clone()
        .or_else(|| profile.display_name.clone())
        .or_else(|| email.split('@').next().map(String::from))
        .unwrap_or_else(|| "user".to_string())
        .to_lowercase();
    let cleaned: String = base_username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(24)
        .collect();
    let username = if cleaned.len() < 3 {
        format!("user{}", &Uuid::new_v4().simple().to_string()[..6])
    } else {
        // Ensure uniqueness by appending random suffix if needed.
        let candidate = cleaned.clone();
        let taken: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
                .bind(&candidate)
                .fetch_optional(db)
                .await?;
        if taken.is_some() {
            format!("{candidate}-{}", &Uuid::new_v4().simple().to_string()[..4])
        } else {
            candidate
        }
    };
    let display_name = profile
        .display_name
        .clone()
        .unwrap_or_else(|| username.clone());
    let parts: Vec<&str> = display_name.split_whitespace().collect();
    let first_name = parts.first().copied().unwrap_or(&display_name).to_string();
    let last_name = if parts.len() >= 2 {
        parts[1..].join(" ")
    } else {
        String::new()
    };
    // Placeholder password_hash (unusable) — the user can add a real password later.
    let password_hash = "$argon2id$v=19$m=19456,t=2,p=1$oauth-placeholder$oauth-placeholder";
    // Pattern C: skill_domain and terms_accepted_at are deliberately NULL.
    // The onboarding flow (POST /auth/complete-profile) fills them in and RGPD consent is
    // captured explicitly at that point — never assumed from the OAuth click.
    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, skill_domain, email_verified)
        VALUES ($1, $2, $3, $4, $5, $6, NULL, TRUE)
        RETURNING id
        "#,
    )
    .bind(&email)
    .bind(&username)
    .bind(password_hash)
    .bind(&first_name)
    .bind(&last_name)
    .bind(&display_name)
    .fetch_one(db)
    .await?;
    Ok(inserted.0)
}
