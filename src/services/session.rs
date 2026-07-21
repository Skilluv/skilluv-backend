//! Refresh-token session management with rotation + reuse detection.
//!
//! Each active device = one row in `user_sessions`. The refresh token is never stored in clear;
//! we keep its SHA-256 (`refresh_hash`) and the previous generation (`previous_hash`).
//!
//! On rotation:
//! - Token matches `refresh_hash` → rotate (`previous_hash := refresh_hash`, `refresh_hash := hash(new)`).
//! - Token matches `previous_hash` → **reuse detected** → revoke session and every other active session
//!   of the same user (defensive: a leaked token was replayed after we already issued a fresh one).
//! - Otherwise → invalid.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::AuthService;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type SessionRow107 = (Uuid, Vec<u8>, Option<Vec<u8>>, Option<DateTime<Utc>>);

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub device_label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

pub struct SessionService;

impl SessionService {
    /// Create a fresh session and return (session_id, opaque_refresh_token).
    /// The opaque token is what the client stores in the refresh cookie; the DB only sees its hash.
    pub async fn create(
        db: &PgPool,
        user_id: Uuid,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(Uuid, String), AppError> {
        Self::create_with_method(db, user_id, ip, user_agent, "password").await
    }

    /// If the incoming request already carries a `refresh_token` cookie for
    /// this user, revoke that prior session. Called from every login handler
    /// before minting a new session — otherwise the browser overwrites the
    /// cookie but the DB row stays `revoked_at IS NULL`, and every fresh
    /// login for the same account visibly accumulates a ghost row in the
    /// user's "active sessions" list.
    ///
    /// Best-effort: failures are swallowed on purpose. A stale cookie
    /// pointing to a non-existent or already-revoked session is expected on
    /// the happy path (fresh browser, prior logout, expired session).
    pub async fn revoke_prior_from_cookie(db: &PgPool, user_id: Uuid, cookie_header: Option<&str>) {
        let Some(header) = cookie_header else { return };
        let Some(sid) = header
            .split(';')
            .map(|s| s.trim())
            .find_map(|s| s.strip_prefix("refresh_token="))
            .and_then(|v| v.split_once(':').map(|(sid, _)| sid))
            .and_then(|s| Uuid::parse_str(s).ok())
        else {
            return;
        };
        let _ = Self::revoke_one(db, user_id, sid).await;
    }

    /// Same as `create` but records how this session was authenticated (see the
    /// `user_sessions.login_method` column added by migration 0050).
    pub async fn create_with_method(
        db: &PgPool,
        user_id: Uuid,
        ip: Option<&str>,
        user_agent: Option<&str>,
        login_method: &str,
    ) -> Result<(Uuid, String), AppError> {
        let token = AuthService::generate_refresh_token();
        let hash = sha256(&token);

        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO user_sessions (user_id, refresh_hash, ip, user_agent, login_method)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id",
        )
        .bind(user_id)
        .bind(&hash)
        .bind(ip)
        .bind(user_agent)
        .bind(login_method)
        .fetch_one(db)
        .await?;

        Ok((row.0, token))
    }

    /// Rotate the refresh token for a session, with reuse detection.
    /// Returns the new opaque token to place in the cookie.
    pub async fn rotate(
        db: &PgPool,
        session_id: Uuid,
        presented_token: &str,
    ) -> Result<(Uuid, String), AppError> {
        let presented_hash = sha256(presented_token);

        let row: Option<SessionRow107> = sqlx::query_as(
            "SELECT user_id, refresh_hash, previous_hash, revoked_at FROM user_sessions WHERE id = $1",
        )
        .bind(session_id)
        .fetch_optional(db)
        .await?;

        let (user_id, current, previous, revoked_at) = row.ok_or(AppError::Unauthorized)?;
        if revoked_at.is_some() {
            return Err(AppError::Unauthorized);
        }

        if constant_time_eq(&presented_hash, &current) {
            let new_token = AuthService::generate_refresh_token();
            let new_hash = sha256(&new_token);
            sqlx::query(
                "UPDATE user_sessions
                 SET previous_hash = refresh_hash,
                     refresh_hash = $1,
                     last_used_at = NOW()
                 WHERE id = $2",
            )
            .bind(&new_hash)
            .bind(session_id)
            .execute(db)
            .await?;
            return Ok((user_id, new_token));
        }

        if let Some(prev) = previous
            && constant_time_eq(&presented_hash, &prev)
        {
            // REUSE — a token we already rotated away has just been replayed.
            // Best defense: revoke the whole family of sessions for this user.
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
            )
            .bind(user_id)
            .execute(db)
            .await?;
            tracing::warn!(%user_id, %session_id, "Refresh-token reuse detected; revoked all sessions");
            return Err(AppError::Unauthorized);
        }

        Err(AppError::Unauthorized)
    }

    pub async fn revoke_one(db: &PgPool, user_id: Uuid, session_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = NOW() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(user_id)
        .execute(db)
        .await?;
        Ok(())
    }

    pub async fn revoke_all(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(db)
        .await?;
        Ok(())
    }

    pub async fn revoke_all_except(db: &PgPool, user_id: Uuid, keep: Uuid) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = NOW()
             WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(keep)
        .execute(db)
        .await?;
        Ok(())
    }

    pub async fn list_active(db: &PgPool, user_id: Uuid) -> Result<Vec<SessionRow>, AppError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, ip, user_agent, device_label, created_at, last_used_at
             FROM user_sessions
             WHERE user_id = $1 AND revoked_at IS NULL
             ORDER BY last_used_at DESC",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;
        Ok(rows)
    }
}

fn sha256(input: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    h.finalize().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}
