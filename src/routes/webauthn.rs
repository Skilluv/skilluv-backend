//! Passkeys / WebAuthn endpoints.
//!
//! Two ceremonies:
//! - Registration (authed): user enrols a new authenticator on their account.
//! - Authentication (public): user signs in with an already-registered passkey.
//!
//! Ceremony state (the server-side challenge + expected user handle) lives in Redis for 10 min.
//! Credential blobs are stored as JSONB in `webauthn_credentials`.

use axum::extract::{Path, State};
use axum::http::header::SET_COOKIE;
use axum::response::{AppendHeaders, IntoResponse};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, build_csrf_cookie, extract_ip, generate_csrf_token};
use crate::models::{User, UserPrivate};
use crate::services::webauthn as wa_state;
use crate::services::{AuthService, SessionService};

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type WebauthnRow209 = (
    Uuid,
    Option<String>,
    Option<chrono::DateTime<chrono::Utc>>,
    chrono::DateTime<chrono::Utc>,
);

pub fn webauthn_routes() -> Router<AppState> {
    Router::new()
        // Enrolment (authed)
        .route("/auth/webauthn/register/start", post(register_start))
        .route("/auth/webauthn/register/finish", post(register_finish))
        // Credential management (authed)
        .route("/auth/webauthn/credentials", get(list_credentials))
        .route("/auth/webauthn/credentials/{id}", delete(delete_credential))
        .route("/auth/webauthn/credentials/{id}", patch(rename_credential))
        // Login ceremony (public)
        .route("/auth/webauthn/login/start", post(login_start))
        .route("/auth/webauthn/login/finish", post(login_finish))
}

// ─── Response helpers ─────────────────────────────────────────────

fn envelope(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn build_access_cookie(access_token: &str) -> String {
    format!("access_token={access_token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=900")
}

fn build_refresh_cookie(session_id: Uuid, token: &str) -> String {
    format!(
        "refresh_token={session_id}:{token}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
        7 * 24 * 60 * 60
    )
}

// ─── Registration ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegisterStartRequest {
    /// Optional user-facing label ("MacBook Touch ID", "Yubikey 5C") — persisted at finish.
    label: Option<String>,
}

async fn register_start(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterStartRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    // Exclude existing credentials so the authenticator refuses to re-register itself.
    let existing_ids: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT credential_id FROM webauthn_credentials WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_all(&state.db)
            .await?;
    let exclude: Vec<CredentialID> = existing_ids.into_iter().map(|bytes| bytes.into()).collect();

    let (ccr, reg_state) = state
        .webauthn
        .inner()
        .start_passkey_registration(
            user.id,
            &user.username,
            &user.display_name,
            if exclude.is_empty() {
                None
            } else {
                Some(exclude)
            },
        )
        .map_err(|e| AppError::Internal(format!("start_passkey_registration: {e}")))?;

    let mut redis = state.redis.clone();
    let handle = wa_state::stash_registration(&mut redis, &reg_state).await?;

    // webauthn-rs's CreationChallengeResponse serializes as
    // `{ "publicKey": { challenge, user, ... } }`. `@simplewebauthn/browser`
    // expects the FLAT `PublicKeyCredentialCreationOptionsJSON` (challenge,
    // user, rp, …) at the top level of `optionsJSON`. Unwrap here so the
    // client can pass our `publicKey` field straight through.
    let ccr_json = serde_json::to_value(&ccr)
        .map_err(|e| AppError::Internal(format!("ccr serialize: {e}")))?;
    let public_key = ccr_json
        .get("publicKey")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    Ok(Json(envelope(json!({
        "ceremony_handle": handle,
        "publicKey": public_key,
        "label": body.label,
    }))))
}

#[derive(Debug, Deserialize)]
struct RegisterFinishRequest {
    ceremony_handle: String,
    credential: RegisterPublicKeyCredential,
    label: Option<String>,
}

async fn register_finish(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterFinishRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = state.redis.clone();
    let reg_state = wa_state::pop_registration(&mut redis, &body.ceremony_handle).await?;

    let passkey = state
        .webauthn
        .inner()
        .finish_passkey_registration(&body.credential, &reg_state)
        .map_err(|e| AppError::Validation(format!("Passkey registration failed: {e}")))?;

    // Reject if this credential_id is already registered anywhere (spec requirement).
    let cred_id_bytes: Vec<u8> = passkey.cred_id().as_ref().to_vec();
    let clash: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM webauthn_credentials WHERE credential_id = $1")
            .bind(&cred_id_bytes)
            .fetch_optional(&state.db)
            .await?;
    if clash.is_some() {
        return Err(AppError::Validation(
            "This authenticator is already registered".into(),
        ));
    }

    let credential_json = serde_json::to_value(&passkey)
        .map_err(|e| AppError::Internal(format!("serialize passkey: {e}")))?;

    let label = body
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 60);

    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO webauthn_credentials (user_id, credential_id, credential, label)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(auth.user_id)
    .bind(&cred_id_bytes)
    .bind(&credential_json)
    .bind(label)
    .fetch_one(&state.db)
    .await?;

    let user_email: (String, String) =
        sqlx::query_as("SELECT email, display_name FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;
    let _ = state
        .email
        .send_security_alert(
            &user_email.0,
            &user_email.1,
            "Nouvelle passkey enregistrée",
            "Une nouvelle passkey a été ajoutée à ton compte Skilluv.",
        )
        .await;

    Ok(Json(envelope(json!({
        "id": row.0,
        "message": "Passkey registered"
    }))))
}

// ─── Credential management ────────────────────────────────────────

async fn list_credentials(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows: Vec<WebauthnRow209> = sqlx::query_as(
        "SELECT id, label, last_used_at, created_at
             FROM webauthn_credentials
             WHERE user_id = $1
             ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|(id, label, last_used_at, created_at)| {
            json!({
                "id": id,
                "label": label,
                "last_used_at": last_used_at,
                "created_at": created_at,
            })
        })
        .collect();
    Ok(Json(envelope(json!({ "credentials": items }))))
}

#[derive(Debug, Deserialize)]
struct RenameRequest {
    label: String,
}

async fn rename_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RenameRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let label = body.label.trim();
    if label.is_empty() || label.len() > 60 {
        return Err(AppError::Validation(
            "Label must be between 1 and 60 characters".into(),
        ));
    }
    let updated =
        sqlx::query("UPDATE webauthn_credentials SET label = $1 WHERE id = $2 AND user_id = $3")
            .bind(label)
            .bind(id)
            .bind(auth.user_id)
            .execute(&state.db)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound("Credential not found".into()));
    }
    Ok(Json(envelope(json!({ "message": "Renamed" }))))
}

async fn delete_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = sqlx::query("DELETE FROM webauthn_credentials WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound("Credential not found".into()));
    }
    Ok(Json(envelope(json!({ "message": "Deleted" }))))
}

// ─── Login ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginStartRequest {
    /// Email OR username. Used to look up the credentials to send back to the browser.
    identifier: String,
}

async fn login_start(
    State(state): State<AppState>,
    Json(body): Json<LoginStartRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let identifier = body.identifier.trim().to_lowercase();
    // Return the same shape regardless of whether the user exists / has passkeys, so no email
    // enumeration is possible: the browser only fails at the end if it can't produce a matching
    // credential. We still need to know the user_id server-side though.
    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1 OR username = $1")
            .bind(&identifier)
            .fetch_optional(&state.db)
            .await?;

    let user_id = user_id.ok_or_else(|| AppError::Validation("No passkey registered".into()))?;

    let cred_rows: Vec<(serde_json::Value,)> =
        sqlx::query_as("SELECT credential FROM webauthn_credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;

    if cred_rows.is_empty() {
        return Err(AppError::Validation("No passkey registered".into()));
    }

    let passkeys: Vec<Passkey> = cred_rows
        .into_iter()
        .map(|(v,)| serde_json::from_value::<Passkey>(v))
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Internal(format!("bad stored credential: {e}")))?;

    let (rcr, auth_state) = state
        .webauthn
        .inner()
        .start_passkey_authentication(&passkeys)
        .map_err(|e| AppError::Internal(format!("start_passkey_authentication: {e}")))?;

    // Bind the ceremony to this user_id — we look it up again at finish, we don't trust the client.
    let mut redis = state.redis.clone();
    let handle = wa_state::stash_authentication(&mut redis, &auth_state).await?;
    let () = redis::AsyncCommands::set_ex(
        &mut redis,
        format!("webauthn:auth_user:{handle}"),
        user_id.to_string(),
        10 * 60,
    )
    .await?;

    // Same unwrap as register_start: webauthn-rs wraps the options under a
    // `publicKey` field, but the frontend passes our `publicKey` straight to
    // `startAuthentication({ optionsJSON })` which expects the flat options.
    let rcr_json = serde_json::to_value(&rcr)
        .map_err(|e| AppError::Internal(format!("rcr serialize: {e}")))?;
    let public_key = rcr_json
        .get("publicKey")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    Ok(Json(envelope(json!({
        "ceremony_handle": handle,
        "publicKey": public_key,
    }))))
}

#[derive(Debug, Deserialize)]
struct LoginFinishRequest {
    ceremony_handle: String,
    credential: PublicKeyCredential,
}

async fn login_finish(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<LoginFinishRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut redis = state.redis.clone();
    let auth_state = wa_state::pop_authentication(&mut redis, &body.ceremony_handle).await?;

    let user_id_str: Option<String> = redis::AsyncCommands::get(
        &mut redis,
        format!("webauthn:auth_user:{}", body.ceremony_handle),
    )
    .await?;
    let user_id: Uuid = user_id_str
        .ok_or(AppError::Unauthorized)?
        .parse()
        .map_err(|_| AppError::Internal("bad stored user id".into()))?;
    let _: () = redis::AsyncCommands::del(
        &mut redis,
        format!("webauthn:auth_user:{}", body.ceremony_handle),
    )
    .await?;

    let auth_result = state
        .webauthn
        .inner()
        .finish_passkey_authentication(&body.credential, &auth_state)
        .map_err(|e| AppError::Validation(format!("Passkey auth failed: {e}")))?;

    // Update the credential counter / backup-state if needed.
    let cred_id_bytes: Vec<u8> = auth_result.cred_id().as_ref().to_vec();
    let stored: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT id, credential FROM webauthn_credentials WHERE credential_id = $1 AND user_id = $2",
    )
    .bind(&cred_id_bytes)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    let (cred_row_id, cred_json) = stored.ok_or(AppError::Unauthorized)?;

    let mut passkey: Passkey = serde_json::from_value(cred_json)
        .map_err(|e| AppError::Internal(format!("bad stored credential: {e}")))?;
    if let Some(true) = passkey.update_credential(&auth_result) {
        let new_json = serde_json::to_value(&passkey)
            .map_err(|e| AppError::Internal(format!("re-serialize passkey: {e}")))?;
        sqlx::query(
            "UPDATE webauthn_credentials SET credential = $1, last_used_at = NOW() WHERE id = $2",
        )
        .bind(new_json)
        .bind(cred_row_id)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query("UPDATE webauthn_credentials SET last_used_at = NOW() WHERE id = $1")
            .bind(cred_row_id)
            .execute(&state.db)
            .await?;
    }

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    if user.is_banned {
        return Err(AppError::Forbidden);
    }

    let ip = extract_ip(&headers);
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    // Label the session as webauthn so the JWT claim + user_sessions row
    // reflect the actual factor. Downstream gates (require_enterprise,
    // AuthUserComplete) can then decide policy per method — today they treat
    // webauthn like password, but the labeling is a prerequisite for changing
    // that later without an audit rewrite.
    let access_token = AuthService::generate_access_token_with_method(
        user.id,
        &user.role,
        "webauthn",
        &state.config.jwt_secret,
    )?;
    SessionService::revoke_prior_from_cookie(
        &state.db,
        user.id,
        headers.get("cookie").and_then(|v| v.to_str().ok()),
    )
    .await;
    let (session_id, refresh_token) =
        SessionService::create_with_method(&state.db, user.id, Some(&ip), ua, "webauthn").await?;

    let csrf = generate_csrf_token();
    let csrf_cookie = build_csrf_cookie(&csrf, "/api", 15 * 60);

    let user_private: UserPrivate = user.into();

    Ok((
        AppendHeaders([
            (SET_COOKIE, build_access_cookie(&access_token)),
            (SET_COOKIE, build_refresh_cookie(session_id, &refresh_token)),
            (SET_COOKIE, csrf_cookie),
        ]),
        Json(envelope(json!({
            "user": user_private,
            "csrf_token": csrf,
            "login_method": "webauthn",
            // Kept for backwards-compat with older frontends that read
            // auth_method on the passkey login response.
            "auth_method": "passkey",
        }))),
    ))
}
