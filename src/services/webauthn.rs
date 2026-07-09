//! WebAuthn / passkey relying-party service.
//!
//! Builds a shared [`Webauthn`] instance from `BASE_URL` (RP origin) and stores active
//! registration/authentication ceremonies in Redis (short TTL, keyed by opaque handle).

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::errors::AppError;

pub struct WebauthnService {
    inner: Webauthn,
}

impl WebauthnService {
    /// Build from `base_url`. RP ID is derived as the hostname (without port).
    /// RP display name is the app name.
    pub fn new(base_url: &str) -> Result<Self, AppError> {
        let url = url::Url::parse(base_url)
            .map_err(|e| AppError::Internal(format!("Invalid BASE_URL for WebAuthn: {e}")))?;
        let rp_id = url
            .host_str()
            .ok_or_else(|| AppError::Internal("BASE_URL has no host".into()))?
            .to_string();
        let inner = WebauthnBuilder::new(&rp_id, &url)
            .map_err(|e| AppError::Internal(format!("WebauthnBuilder: {e}")))?
            .rp_name("Skilluv")
            .build()
            .map_err(|e| AppError::Internal(format!("Webauthn build: {e}")))?;
        Ok(Self { inner })
    }

    pub fn inner(&self) -> &Webauthn {
        &self.inner
    }
}

// ─── Ceremony state storage (Redis, 10 min TTL) ─────────────────────

const CEREMONY_TTL_SECS: u64 = 10 * 60;

fn reg_key(handle: &str) -> String {
    format!("webauthn:reg:{handle}")
}

fn auth_key(handle: &str) -> String {
    format!("webauthn:auth:{handle}")
}

pub async fn stash_registration(
    redis: &mut ConnectionManager,
    state: &PasskeyRegistration,
) -> Result<String, AppError> {
    let handle = Uuid::new_v4().simple().to_string();
    let json = serde_json::to_string(state)
        .map_err(|e| AppError::Internal(format!("serialize reg state: {e}")))?;
    let () = redis.set_ex(reg_key(&handle), json, CEREMONY_TTL_SECS).await?;
    Ok(handle)
}

pub async fn pop_registration(
    redis: &mut ConnectionManager,
    handle: &str,
) -> Result<PasskeyRegistration, AppError> {
    let json: Option<String> = redis.get(reg_key(handle)).await?;
    let json = json.ok_or_else(|| AppError::Validation("Ceremony expired or unknown".into()))?;
    let () = redis.del(reg_key(handle)).await?;
    serde_json::from_str(&json)
        .map_err(|e| AppError::Internal(format!("deserialize reg state: {e}")))
}

pub async fn stash_authentication(
    redis: &mut ConnectionManager,
    state: &PasskeyAuthentication,
) -> Result<String, AppError> {
    let handle = Uuid::new_v4().simple().to_string();
    let json = serde_json::to_string(state)
        .map_err(|e| AppError::Internal(format!("serialize auth state: {e}")))?;
    let () = redis.set_ex(auth_key(&handle), json, CEREMONY_TTL_SECS).await?;
    Ok(handle)
}

pub async fn pop_authentication(
    redis: &mut ConnectionManager,
    handle: &str,
) -> Result<PasskeyAuthentication, AppError> {
    let json: Option<String> = redis.get(auth_key(handle)).await?;
    let json = json.ok_or_else(|| AppError::Validation("Ceremony expired or unknown".into()))?;
    let () = redis.del(auth_key(handle)).await?;
    serde_json::from_str(&json)
        .map_err(|e| AppError::Internal(format!("deserialize auth state: {e}")))
}
