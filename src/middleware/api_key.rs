use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;

/// Authenticated API key context.
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    pub key_id: Uuid,
    pub user_id: Uuid,
    pub permissions: Vec<String>,
}

impl FromRequestParts<AppState> for ApiKeyAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Extract API key from Authorization header or query param
        let raw_key = extract_api_key(parts)?;

        // Parse prefix (first 12 chars: "sk_live_XXXX")
        if raw_key.len() < 12 || !raw_key.starts_with("sk_live_") {
            return Err(AppError::Unauthorized);
        }

        let prefix = &raw_key[..12];

        // Find matching key by prefix.
        //
        // `revoked_at` as well as `active`, and the asymmetry is worth naming.
        // Migration 0359 added `revoked_at` and `revoked_reason` to this table
        // *and* an index keyed on `revoked_at IS NULL` — which states, in the
        // schema, that a NULL there is what makes a key live. This
        // authenticator only ever read `active`.
        //
        // Nothing wrote `revoked_at`, so nothing was broken. But two
        // revocation mechanisms existed, one of them was the obvious one to
        // reach for, and reaching for it would have revoked a key that kept
        // authenticating. That is the shape of a bug nobody finds until it
        // matters.
        let key_row: Option<(Uuid, Uuid, String, serde_json::Value, bool, bool)> = sqlx::query_as(
            "SELECT id, user_id, key_hash, permissions, active, revoked_at IS NOT NULL
               FROM api_keys WHERE key_prefix = $1",
        )
        .bind(prefix)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Unauthorized)?;

        let (key_id, user_id, key_hash, permissions, active, revoked) =
            key_row.ok_or(AppError::Unauthorized)?;

        if !active || revoked {
            return Err(AppError::Unauthorized);
        }

        // Verify key hash
        let valid =
            crate::services::AuthService::verify_password(&raw_key, &key_hash).unwrap_or(false);

        if !valid {
            return Err(AppError::Unauthorized);
        }

        // Update usage stats (fire and forget)
        let db = state.db.clone();
        let kid = key_id;
        tokio::spawn(async move {
            let _ = sqlx::query(
                "UPDATE api_keys SET last_used_at = NOW(), request_count = request_count + 1 WHERE id = $1",
            )
            .bind(kid)
            .execute(&db)
            .await;
        });

        let perms: Vec<String> = serde_json::from_value(permissions).unwrap_or_default();

        Ok(ApiKeyAuth {
            key_id,
            user_id,
            permissions: perms,
        })
    }
}

impl ApiKeyAuth {
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm || p == "*")
    }

    pub fn require_permission(&self, perm: &str) -> Result<(), AppError> {
        if self.has_permission(perm) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

fn extract_api_key(parts: &Parts) -> Result<String, AppError> {
    // Try Authorization: Bearer sk_live_xxx
    if let Some(auth_header) = parts.headers.get("authorization")
        && let Ok(value) = auth_header.to_str()
        && let Some(key) = value.strip_prefix("Bearer ")
    {
        return Ok(key.trim().to_string());
    }

    // Try query param ?api_key=sk_live_xxx
    if let Some(query) = parts.uri.query() {
        for pair in query.split('&') {
            if let Some(key) = pair.strip_prefix("api_key=") {
                return Ok(key.to_string());
            }
        }
    }

    Err(AppError::Unauthorized)
}
