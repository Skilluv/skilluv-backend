use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
    /// How this session was authenticated. Read by `require_enterprise` to
    /// bypass the mandatory-TOTP gate for SSO-authenticated users (the IdP
    /// already enforces MFA in that case).
    ///
    /// Values: "password" | "oauth" | "sso" | "magic_link" | "webauthn".
    /// Missing (older tokens) is treated as "password".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_method: Option<String>,
}

pub struct AuthService;

impl AuthService {
    pub fn hash_password(password: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(format!("Password hashing failed: {e}")))
    }

    pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid hash: {e}")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    pub fn generate_access_token(
        user_id: Uuid,
        role: &str,
        secret: &str,
    ) -> Result<String, AppError> {
        Self::generate_access_token_with_method(user_id, role, "password", secret)
    }

    pub fn generate_access_token_with_method(
        user_id: Uuid,
        role: &str,
        login_method: &str,
        secret: &str,
    ) -> Result<String, AppError> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            role: role.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::minutes(15)).timestamp(),
            login_method: Some(login_method.to_string()),
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|e| AppError::Internal(format!("JWT generation failed: {e}")))
    }

    pub fn verify_access_token(token: &str, secret: &str) -> Result<Claims, AppError> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|_| AppError::Unauthorized)
    }

    pub async fn store_refresh_token(
        redis: &mut redis::aio::ConnectionManager,
        user_id: Uuid,
        refresh_token: &str,
    ) -> Result<(), AppError> {
        let key = format!("refresh:{user_id}");
        let ttl_seconds: u64 = 7 * 24 * 60 * 60; // 7 days
        let () = redis.set_ex(&key, refresh_token, ttl_seconds).await?;
        Ok(())
    }

    pub async fn validate_refresh_token(
        redis: &mut redis::aio::ConnectionManager,
        user_id: Uuid,
        token: &str,
    ) -> Result<bool, AppError> {
        let key = format!("refresh:{user_id}");
        let stored: Option<String> = redis.get(&key).await?;
        Ok(stored.as_deref() == Some(token))
    }

    pub async fn revoke_refresh_token(
        redis: &mut redis::aio::ConnectionManager,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let key = format!("refresh:{user_id}");
        let () = redis.del(&key).await?;
        Ok(())
    }

    pub fn generate_refresh_token() -> String {
        Uuid::new_v4().to_string()
    }
}
