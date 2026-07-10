use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use crate::errors::AppError;

/// Redis-backed rate limiter using sliding window counter.
pub struct RateLimiter;

impl RateLimiter {
    /// Check if a request is allowed. Returns Ok(()) if allowed, Err(429) if exceeded.
    ///
    /// - `category`: e.g. "auth", "sandbox", "contact"
    /// - `identifier`: IP address or user_id
    /// - `max_requests`: max allowed in window
    /// - `window_secs`: window duration in seconds
    ///
    /// L'env var `SKILLUV_DISABLE_RATELIMIT=1` désactive complètement le check
    /// (utilisé exclusivement par la suite de tests d'intégration : plusieurs
    /// binaires en parallèle sur le même Redis heurtaient le bucket partagé et
    /// se mangeaient mutuellement les 5 registers/heure).
    pub async fn check(
        redis: &mut ConnectionManager,
        category: &str,
        identifier: &str,
        max_requests: u64,
        window_secs: u64,
    ) -> Result<(), AppError> {
        if std::env::var("SKILLUV_DISABLE_RATELIMIT").as_deref() == Ok("1") {
            return Ok(());
        }
        let key = format!("ratelimit:{category}:{identifier}");

        let count: i64 = redis.incr(&key, 1).await?;

        // Set expiry only on first request in window
        if count == 1 {
            let () = redis.expire(&key, window_secs as i64).await?;
        }

        if count as u64 > max_requests {
            let ttl: i64 = redis::cmd("TTL").arg(&key).query_async(redis).await?;

            return Err(AppError::RateLimited(ttl));
        }

        Ok(())
    }
}

/// Extract client IP from request headers (X-Forwarded-For or peer addr).
pub fn extract_ip(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}
