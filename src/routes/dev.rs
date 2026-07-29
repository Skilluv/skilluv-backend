//! Dev-mode-only helper endpoints — gated by `SKILLUV_DEV_MODE=true` env var.
//!
//! Purpose : let e2e test tooling (Playwright, curl scripts) programmatically
//! read state that would normally require an email client — e.g. the current
//! email verification token for a freshly-registered user, so a test can call
//! `GET /api/auth/verify-email?token=<...>` without scraping Gmail.
//!
//! **NEVER** enable `SKILLUV_DEV_MODE=true` in prod. The health of these
//! endpoints assumes the caller is trusted (they can bypass email ownership).
//! `assert_production_secrets` refuses to boot if `SKILLUV_DEV_MODE=true` AND
//! `ENVIRONMENT=prod` — see src/config/app.rs.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use redis::AsyncCommands;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;

pub fn dev_routes() -> Router<AppState> {
    Router::new().route("/dev/verify-tokens/{email}", get(get_verify_token))
}

/// GET /api/dev/verify-tokens/{email}
///
/// Look up the current pending email-verify token for the given email
/// address. Iterates Redis keys `email_verify:*` (SCAN, not KEYS — the
/// deprecated blocking variant), matches the user_id stored under each key
/// against the user_id of the given email, returns the first match.
///
/// Responses :
/// - 200 `{ token, user_id, ttl_seconds }` — token found
/// - 404 — no pending token (either the user doesn't exist, is already
///   verified, or the token expired)
/// - 403 `AUTH_FORBIDDEN` — SKILLUV_DEV_MODE isn't `true`
async fn get_verify_token(
    State(state): State<AppState>,
    Path(email): Path<String>,
) -> Result<Json<Value>, AppError> {
    if std::env::var("SKILLUV_DEV_MODE").as_deref() != Ok("true") {
        return Err(AppError::Forbidden);
    }

    // Fetch user_id from email — case-insensitive to match /auth/register.
    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE LOWER(email) = LOWER($1)")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?;
    let Some(user_id) = user_id else {
        return Err(AppError::NotFound(format!("no user with email {email}")));
    };
    let target_user_id_str = user_id.to_string();

    let mut redis = state.redis.clone();

    // SCAN through email_verify:* keys.
    let mut cursor: u64 = 0;
    loop {
        let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg("email_verify:*")
            .arg("COUNT")
            .arg(200)
            .query_async(&mut redis)
            .await?;
        for key in &keys {
            let val: Option<String> = redis.get(key).await?;
            if val.as_deref() == Some(target_user_id_str.as_str()) {
                let token = key.strip_prefix("email_verify:").unwrap_or(key);
                let ttl: i64 = redis::cmd("TTL").arg(key).query_async(&mut redis).await?;
                return Ok(Json(json!({
                    "data": {
                        "token": token,
                        "user_id": target_user_id_str,
                        "ttl_seconds": ttl,
                    },
                    "meta": {
                        "request_id": Uuid::new_v4().to_string(),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }
                })));
            }
        }
        if next_cursor == 0 {
            break;
        }
        cursor = next_cursor;
    }

    Err(AppError::NotFound(format!(
        "no pending email_verify token for {email} (already verified or expired ?)"
    )))
}
