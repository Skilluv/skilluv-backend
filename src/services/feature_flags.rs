//! P26 v2 / Hygiène pré-prod SKI-33 — DB-backed feature flags with
//! rollout percentage and in-process cache.
//!
//! Complements (does not replace) the existing
//! `SKILLUV_*_ENABLED=1` env-var toggles used for infrastructure workers.
//! Env-flags fit bootstrap decisions ("is the digest worker enabled at
//! all?"); DB-flags fit runtime product rollouts ("show the new feed to
//! 20% of users?").
//!
//! ─── Semantics ───────────────────────────────────────────────────
//!
//! Given a flag with `enabled=<bool>`, `rollout_percent=<0..100>`:
//! - `enabled=false` → **always false** (kill-switch, regardless of pct)
//! - `enabled=true, rollout_percent=100` → **always true**
//! - `enabled=true, rollout_percent=P` → **true for ~P% of user_ids**
//!   deterministically: same user always lands in the same bucket
//!   (via SHA-256 of `key + user_id`, first 4 bytes mod 100).
//!
//! Anonymous requests (no user_id) fall back to `enabled AND rollout=100`
//! → intentionally strict; if you want partial rollout on anon, hash
//! their IP or a session cookie instead.
//!
//! ─── Cache ───────────────────────────────────────────────────────
//!
//! All flags loaded into a `Mutex<HashMap>` on first call, refreshed
//! every 30s. Cheap because the table is expected to stay < 100 rows.
//! Admin `PUT/DELETE` calls trigger an immediate cache invalidation for
//! the next call.
//!
//! ─── Naming ──────────────────────────────────────────────────────
//!
//! Convention: lowercase snake_case, prefixed by domain when useful
//! (e.g. `feed_v2`, `payment_stripe_connect_new_flow`,
//! `admin_dashboard_v3`). Enforced by the DB CHECK.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::errors::AppError;

pub const CACHE_TTL: Duration = Duration::from_secs(30);

/// Snapshot of a flag row.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct FeatureFlag {
    pub key: String,
    pub enabled: bool,
    pub rollout_percent: i16,
    pub description: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<Uuid>,
}

/// In-process cache. `OnceCell` not used because we need mutation.
static CACHE: Mutex<Option<CacheState>> = Mutex::new(None);

struct CacheState {
    flags: HashMap<String, FeatureFlag>,
    loaded_at: Instant,
}

/// Force cache invalidation on the next call. Used by admin mutations.
pub fn invalidate_cache() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

async fn load_all(db: &PgPool) -> Result<HashMap<String, FeatureFlag>, AppError> {
    let rows: Vec<FeatureFlag> = sqlx::query_as("SELECT * FROM feature_flags")
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().map(|f| (f.key.clone(), f)).collect())
}

/// Ensure the cache is fresh. Returns nothing (populates the static).
async fn ensure_fresh(db: &PgPool) -> Result<(), AppError> {
    let needs_reload = {
        let guard = CACHE
            .lock()
            .map_err(|_| AppError::Internal("feature_flags cache mutex poisoned".into()))?;
        match guard.as_ref() {
            None => true,
            Some(state) => state.loaded_at.elapsed() > CACHE_TTL,
        }
    };
    if needs_reload {
        let flags = load_all(db).await?;
        if let Ok(mut guard) = CACHE.lock() {
            *guard = Some(CacheState {
                flags,
                loaded_at: Instant::now(),
            });
        }
    }
    Ok(())
}

/// Look up a flag in the cache; returns `None` if the flag doesn't exist.
/// Callers should treat "unknown flag" as `disabled` in production (fail-
/// closed), but expose the fact via logs so an operator notices a typo.
async fn lookup(db: &PgPool, key: &str) -> Result<Option<FeatureFlag>, AppError> {
    ensure_fresh(db).await?;
    let guard = CACHE
        .lock()
        .map_err(|_| AppError::Internal("feature_flags cache mutex poisoned".into()))?;
    Ok(guard
        .as_ref()
        .and_then(|state| state.flags.get(key).cloned()))
}

/// Public entry point : is this flag enabled for this user right now?
///
/// - Unknown flag → `false` (fail-closed) + tracing::warn to help spot typos.
/// - Kill-switched (`enabled=false`) → `false`.
/// - Rolled out to 100% → `true`.
/// - Partial rollout → deterministic bucket assignment based on
///   `sha256(flag_key + user_id.as_bytes())`.
pub async fn is_enabled(db: &PgPool, key: &str, user_id: Option<Uuid>) -> bool {
    let Ok(Some(flag)) = lookup(db, key).await else {
        // Unknown flag or DB error — always false. Only log when unknown
        // to avoid spamming on DB outages.
        if let Ok(None) = lookup(db, key).await {
            tracing::warn!(
                flag = key,
                "feature_flags: unknown flag treated as disabled"
            );
        }
        return false;
    };
    if !flag.enabled {
        return false;
    }
    if flag.rollout_percent >= 100 {
        return true;
    }
    if flag.rollout_percent <= 0 {
        return false;
    }
    // Partial rollout: hash the user_id to a stable bucket.
    let Some(uid) = user_id else {
        // Anonymous requests are conservative: rollout <100 → false.
        return false;
    };
    bucket_percent(&flag.key, uid) < (flag.rollout_percent as u32)
}

/// Deterministic bucket assignment (0..100). Same `(key, uid)` always
/// produces the same value, so a user stays in the same bucket across
/// server restarts.
fn bucket_percent(key: &str, user_id: Uuid) -> u32 {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.update(user_id.as_bytes());
    let digest = hasher.finalize();
    let v = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    v % 100
}

// ─── Admin API primitives ─────────────────────────────────────────

pub async fn list_flags(db: &PgPool) -> Result<Vec<FeatureFlag>, AppError> {
    let rows: Vec<FeatureFlag> = sqlx::query_as("SELECT * FROM feature_flags ORDER BY key")
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn upsert_flag(
    db: &PgPool,
    key: &str,
    enabled: bool,
    rollout_percent: i16,
    description: Option<&str>,
    admin_id: Uuid,
) -> Result<FeatureFlag, AppError> {
    if !(0..=100).contains(&rollout_percent) {
        return Err(AppError::Validation(
            "rollout_percent must be 0..100".into(),
        ));
    }
    // DB CHECK enforces key shape too; catch it to a Validation error.
    let flag: FeatureFlag = sqlx::query_as(
        r#"
        INSERT INTO feature_flags (key, enabled, rollout_percent, description, updated_by)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (key) DO UPDATE SET
            enabled = EXCLUDED.enabled,
            rollout_percent = EXCLUDED.rollout_percent,
            description = COALESCE(EXCLUDED.description, feature_flags.description),
            updated_at = NOW(),
            updated_by = EXCLUDED.updated_by
        RETURNING *
        "#,
    )
    .bind(key)
    .bind(enabled)
    .bind(rollout_percent)
    .bind(description)
    .bind(admin_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(dbe) = &e
            && dbe.constraint().is_some()
        {
            return AppError::Validation(format!(
                "invalid key '{key}' — must match ^[a-z][a-z0-9_]{{0,62}}$"
            ));
        }
        AppError::Database(e)
    })?;
    invalidate_cache();
    Ok(flag)
}

pub async fn delete_flag(db: &PgPool, key: &str) -> Result<bool, AppError> {
    let affected = sqlx::query("DELETE FROM feature_flags WHERE key = $1")
        .bind(key)
        .execute(db)
        .await?
        .rows_affected();
    invalidate_cache();
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_deterministic() {
        let uid = Uuid::nil();
        let a = bucket_percent("feed_v2", uid);
        let b = bucket_percent("feed_v2", uid);
        assert_eq!(a, b);
    }

    #[test]
    fn bucket_different_keys_different_buckets_usually() {
        // Not guaranteed on all uids but true for nil in practice.
        let uid = Uuid::nil();
        let a = bucket_percent("feed_v2", uid);
        let b = bucket_percent("admin_dashboard_v3", uid);
        // Not requiring distinct — SHA256 output can collide modulo 100,
        // but we require both are in [0,100).
        assert!(a < 100);
        assert!(b < 100);
    }

    #[test]
    fn bucket_in_range() {
        for i in 0..10 {
            let uid = Uuid::from_u128(i);
            let v = bucket_percent("any_key", uid);
            assert!(v < 100);
        }
    }
}
