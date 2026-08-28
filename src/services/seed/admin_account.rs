//! The administrator every other seed step hangs off.
//!
//! ## Why the password is required and never generated
//!
//! An auto-generated password would have to be printed, logged or stored, and
//! all three are worse than asking. `SEED_ADMIN_PASSWORD` is mandatory and at
//! least twelve characters; without it this step declines rather than invents
//! a credential, and the deployment continues with the catalogue unseeded and
//! says so.
//!
//! Declining rather than failing is the point. A first deployment that has not
//! been told who the administrator is should still come up — the operator sets
//! the variable and restarts, and the seed catches up on the next boot because
//! the ledger has no row for any of it.
//!
//! ## Why it upserts
//!
//! Re-running rotates the password and re-asserts `role = 'admin'` and
//! `email_verified`. An operator who has lost the password recovers by setting
//! the variable and restarting, which is the shortest recovery that does not
//! involve psql.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::{AuthService, capabilities_engine};

/// Short passwords are the ones that get reused.
pub const MIN_PASSWORD_LEN: usize = 12;

pub const DEFAULT_EMAIL: &str = "admin@skill-uv.com";

fn from_env(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// Provision or refresh the administrator, from the environment.
///
/// Returns what happened, for the ledger. An absent or too-short password is
/// reported in the returned string rather than as an error: it is a
/// configuration this deployment has not been given yet, not a fault.
pub async fn run(db: &PgPool) -> Result<String, AppError> {
    let email = from_env("SEED_ADMIN_EMAIL", DEFAULT_EMAIL).to_lowercase();
    let username = from_env("SEED_ADMIN_USERNAME", "admin").to_lowercase();
    let first_name = from_env("SEED_ADMIN_FIRST_NAME", "Admin");
    let last_name = from_env("SEED_ADMIN_LAST_NAME", "Skilluv");

    let Some(password) = std::env::var("SEED_ADMIN_PASSWORD")
        .ok()
        .filter(|p| !p.trim().is_empty())
    else {
        // An administrator may already exist from an earlier deployment, or
        // have been made by hand. Saying "none" when there is one would send
        // an operator looking for a problem that is not there.
        let existing: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE role = 'admin')")
                .fetch_one(db)
                .await?;
        return Ok(if existing {
            "SEED_ADMIN_PASSWORD not set; an administrator already exists, left alone".into()
        } else {
            "SEED_ADMIN_PASSWORD not set; no administrator created".into()
        });
    };

    if password.chars().count() < MIN_PASSWORD_LEN {
        return Ok(format!(
            "SEED_ADMIN_PASSWORD is {} characters; {MIN_PASSWORD_LEN} are required, \
             so no administrator was created",
            password.chars().count()
        ));
    }

    let display_name = format!("{} {}", first_name.trim(), last_name.trim());
    let password_hash = AuthService::hash_password(&password)
        .map_err(|e| AppError::Internal(format!("hash_password failed: {e}")))?;

    let (user_id, inserted): (Uuid, bool) = sqlx::query_as(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             role, email_verified, terms_accepted_at, password_changed_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'admin', TRUE, NOW(), NOW())
        ON CONFLICT (email) DO UPDATE SET
            password_hash = EXCLUDED.password_hash,
            role = 'admin',
            email_verified = TRUE,
            first_name = EXCLUDED.first_name,
            last_name = EXCLUDED.last_name,
            display_name = EXCLUDED.display_name,
            password_changed_at = NOW(),
            updated_at = NOW()
        RETURNING id, (xmax = 0) AS inserted
        "#,
    )
    .bind(&email)
    .bind(&username)
    .bind(&password_hash)
    .bind(first_name.trim())
    .bind(last_name.trim())
    .bind(&display_name)
    .fetch_one(db)
    .await?;

    // Best effort. `role = 'admin'` is what the admin gate reads, so the panel
    // is reachable whatever the capabilities engine does; this refreshes the
    // rank- and activity-derived grants on a re-run.
    if let Err(e) = capabilities_engine::recompute_capabilities_for_user(db, user_id).await {
        tracing::warn!(%user_id, error = %e, "capabilities not recomputed for the seeded admin");
    }

    tracing::info!(%user_id, %email, created = inserted, "admin account seeded");
    Ok(format!(
        "{email} {}",
        if inserted { "created" } else { "updated" }
    ))
}
