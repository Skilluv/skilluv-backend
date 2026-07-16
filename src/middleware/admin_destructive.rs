//! BE-D — Helpers pour les mutations admin sensibles.
//!
//! Deux fonctionnalités :
//!
//! 1. **Rate-limit destructif** (`enforce_admin_destructive`) : 10 req/min et
//!    100 req/heure par `admin_user_id`. À appeler au début de tout handler
//!    admin qui mute quelque chose de sensible (ban, revoke, dissolve,
//!    mark_valid, reject, reset-2fa, KYC decide, etc.).
//!
//!    Retourne `AppError::RateLimited(retry_after_secs)` (429) si dépassé.
//!
//! 2. **Dry-run mode** (`is_admin_dry_run`) : env `SKILLUV_ADMIN_DRY_RUN=1`
//!    active un mode "safe" où toutes les mutations sont loggées mais aucune
//!    ligne DB n'est modifiée. Utile pour les répétitions générales avant
//!    une action critique.
//!
//!    Contrat handler : si `is_admin_dry_run()`, SKIP les writes DB et
//!    retourner `Json({dry_run: true, would_have_done: {...}})`.

use crate::AppState;
use crate::errors::AppError;
use uuid::Uuid;

/// Rate-limit pour actions admin sensibles. Combine 2 fenêtres :
///   - 10 req / 60s (burst protection immédiat)
///   - 100 req / 3600s (protection horaire — anti-script)
///
/// Appelle-la au TOUT DÉBUT du handler, juste après `require_capability("admin")`.
pub async fn enforce_admin_destructive(
    state: &AppState,
    admin_user_id: Uuid,
) -> Result<(), AppError> {
    let mut redis = state.redis.clone();
    let id = admin_user_id.to_string();

    // Burst : 10 en 60s
    crate::middleware::RateLimiter::check(
        &mut redis,
        "admin_destructive_burst",
        &id,
        10,
        60,
    )
    .await?;

    // Long horizon : 100 en 3600s
    crate::middleware::RateLimiter::check(
        &mut redis,
        "admin_destructive_hourly",
        &id,
        100,
        3600,
    )
    .await?;

    Ok(())
}

/// True si l'env `SKILLUV_ADMIN_DRY_RUN=1`. Le handler doit alors :
///   - Skip toutes les mutations DB.
///   - Log l'intention via `tracing::info!(dry_run = true, ...)`.
///   - Retourner 200 avec `{"dry_run": true, "would_have_done": {...}}`.
///
/// Volontairement pas de mode "per-request" (via header) au MVP :
/// dry-run est global via env pour minimiser la surface d'erreur humaine.
pub fn is_admin_dry_run() -> bool {
    std::env::var("SKILLUV_ADMIN_DRY_RUN").as_deref() == Ok("1")
}
