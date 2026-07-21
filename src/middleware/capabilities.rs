//! P18.3 — Helpers de vérification de capability.
//!
//! Chaque handler HTTP qui a besoin d'un droit particulier appelle :
//!
//!     require_capability(&state.db, auth.user_id, "admin").await?;
//!
//! Retourne :
//!   - Ok(()) si l'user a la capability active (revoked_at IS NULL et
//!     expires_at NULL ou > NOW()).
//!   - AppError::Forbidden sinon.
//!
//! Rétro-compat P18 : le backfill 0094 assure que tous les anciens
//! `users.role='admin'/'mentor'/…` ont leurs capabilities équivalentes. Un
//! handler qui utilise `require_capability("admin")` fonctionne pour tous
//! les admins historiques sans intervention.
//!
//! Les vieux `require_admin` inline dans les modules routes sont conservés
//! le temps d'une transition ; en pratique ils vérifient auth.role='admin'
//! qui vient du JWT, donc coexistent sans conflit avec le nouveau système
//! (users.role reste maintenu).

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Retourne Ok(()) si l'user a la capability active à cet instant.
pub async fn require_capability(
    db: &PgPool,
    user_id: Uuid,
    capability: &str,
) -> Result<(), AppError> {
    let has: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM user_capabilities
            WHERE user_id = $1
              AND capability = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
        )
        "#,
    )
    .bind(user_id)
    .bind(capability)
    .fetch_one(db)
    .await?;
    if !has {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// P25.3 — Retourne Ok(()) si l'user a AU MOINS UNE des capabilities listées
/// active. Utile pour les endpoints modération accessibles à plusieurs
/// personas (ex: admin OU plagiarism_reviewer peuvent revoker un deliverable).
pub async fn require_any_capability(
    db: &PgPool,
    user_id: Uuid,
    capabilities: &[&str],
) -> Result<(), AppError> {
    if capabilities.is_empty() {
        return Err(AppError::Forbidden);
    }
    let caps_vec: Vec<String> = capabilities.iter().map(|c| c.to_string()).collect();
    let has: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM user_capabilities
            WHERE user_id = $1
              AND capability = ANY($2)
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
        )
        "#,
    )
    .bind(user_id)
    .bind(&caps_vec)
    .fetch_one(db)
    .await?;
    if !has {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Retourne toutes les capabilities actives d'un user (utile pour /me/capabilities).
pub async fn list_active_capabilities(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, AppError> {
    let rows: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT capability FROM user_capabilities
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND (expires_at IS NULL OR expires_at > NOW())
        ORDER BY capability
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
