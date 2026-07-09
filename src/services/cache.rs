//! Thin Redis cache helpers — Phase 4.16.
//!
//! Deliberately small : we don't need a full Redis-backed cache layer, just a couple
//! of get/set-with-TTL helpers used by the hottest endpoints.

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use crate::errors::AppError;

pub async fn get_json<T: serde::de::DeserializeOwned>(
    redis: &mut ConnectionManager,
    key: &str,
) -> Result<Option<T>, AppError> {
    let raw: Option<String> = redis.get(key).await.ok().flatten();
    match raw {
        Some(s) => Ok(serde_json::from_str(&s).ok()),
        None => Ok(None),
    }
}

pub async fn set_json<T: serde::Serialize>(
    redis: &mut ConnectionManager,
    key: &str,
    value: &T,
    ttl_secs: u64,
) -> Result<(), AppError> {
    let s = serde_json::to_string(value)
        .map_err(|e| AppError::Internal(format!("cache serialize: {e}")))?;
    let () = redis.set_ex(key, s, ttl_secs).await?;
    Ok(())
}

pub async fn invalidate(
    redis: &mut ConnectionManager,
    key: &str,
) -> Result<(), AppError> {
    let _: () = redis.del(key).await?;
    Ok(())
}

/// Convenience wrapper that mirrors the "get-or-compute-and-set" pattern.
pub async fn get_or_compute<T, F, Fut>(
    redis: &mut ConnectionManager,
    key: &str,
    ttl_secs: u64,
    compute: F,
) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Clone,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, AppError>>,
{
    if let Some(v) = get_json::<T>(redis, key).await? {
        return Ok(v);
    }
    let v = compute().await?;
    let _ = set_json(redis, key, &v, ttl_secs).await;
    Ok(v)
}
