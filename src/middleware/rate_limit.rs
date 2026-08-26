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
    ///
    /// L'env var `SKILLUV_RATELIMIT_IP_WHITELIST=ip1,ip2,ip3` skip le check pour
    /// les IPs listées (comma-separated). Utile en staging pour laisser tourner
    /// les runs Playwright depuis une IP de dev sans se manger un 429 après
    /// ~5 tentatives. À ne PAS activer en prod pour des IPs publiques.
    ///
    /// `identifier` : IP client (via `extract_ip`) ou user_id. Une string
    /// vide `""` signale "pas d'IP fiable" (ex : appel direct sans reverse
    /// proxy en dev/CI) et désactive le check pour ce call — évite un
    /// bucket sentinel partagé qui bloquerait tout le monde après N req.
    /// En prod, le reverse proxy DOIT set `X-Forwarded-For` (sinon aucun
    /// rate limit ne s'applique aux requêtes non-authentifiées, à surveiller).
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
        if identifier.is_empty() {
            return Ok(());
        }
        if is_whitelisted_ip(identifier) {
            return Ok(());
        }
        // A declared research token multiplies the ceiling — see
        // `middleware::security_research`. Applied here rather than at every
        // call site because this function is called from a hundred handlers
        // and none of them sees the request.
        let max_requests =
            max_requests.saturating_mul(crate::middleware::security_research::rate_limit_multiplier());

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

/// Check if the given identifier (IP) is in the SKILLUV_RATELIMIT_IP_WHITELIST
/// env var (comma-separated). Reads the env fresh on every call — the list
/// is small (0-10 entries) and rate-limit checks are already cheap.
fn is_whitelisted_ip(identifier: &str) -> bool {
    let Ok(raw) = std::env::var("SKILLUV_RATELIMIT_IP_WHITELIST") else {
        return false;
    };
    if raw.is_empty() {
        return false;
    }
    raw.split(',').any(|ip| ip.trim() == identifier)
}

/// Extrait l'IP client depuis les headers set par un reverse proxy
/// (`X-Forwarded-For` en priorité, `X-Real-IP` en fallback). Renvoie
/// une string vide `""` quand aucun header n'est présent — ce qui
/// signale un appel direct (dev local, tests, ou déploiement mal
/// configuré derrière un proxy qui n'écrit pas ces headers).
///
/// Retourner `""` (au lieu de l'ancienne sentinel `"unknown"`) évite un
/// bucket rate-limit partagé qui bloquerait tous les callers sans IP
/// après N requêtes. Cf. `RateLimiter::check` qui skip naturellement
/// quand l'identifier est vide. Pour les usages hors rate-limit
/// (audit log, storage IP source), une string vide signale bien
/// "IP inconnue" et se sérialise proprement.
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
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}
