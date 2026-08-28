//! P18.3 — Helpers de vérification de capability.
//!
//! Chaque handler HTTP qui a besoin d'un droit particulier appelle :
//!
//! ```ignore
//! require_capability(&state.db, auth.user_id, "admin").await?;
//! ```
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
    if !has_capability(db, user_id, capability).await? {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Boolean variant of [`require_capability`]: returns whether the user holds
/// the (non-revoked, non-expired) capability, without turning its absence into
/// an error. Used by endpoints whose gate is a compound condition — e.g. a
/// guild officer *or* an admin may act — where the admin arm is one branch of
/// an `||` rather than the whole check.
pub async fn has_capability(
    db: &PgPool,
    user_id: Uuid,
    capability: &str,
) -> Result<bool, AppError> {
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
    Ok(has)
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

/// P26 v2 (SKI-80): convenience helper for the challenge-validator capability
/// family. Given a domain (`code`, `design`, `game`, `security`, `ops`, `ai`,
/// `soft_skills`), builds the capability string `challenge_validator:{domain}`
/// and delegates to `require_capability`.
///
/// Used by the validation pick-up / approve / reject endpoints (SKI-83/84/85)
/// to ensure only users authorized on this domain can validate PRs in it.
///
/// Returns `AppError::Validation` if the domain is unknown (clearer error
/// surface than a silent capability mismatch), or `AppError::Forbidden` if
/// the user does not hold the capability.
pub async fn require_challenge_validator_for(
    db: &PgPool,
    user_id: Uuid,
    domain: &str,
) -> Result<(), AppError> {
    // Guard against unknown domains rather than delegating a malformed
    // capability string that would never match.
    if !crate::validators::SKILL_DOMAINS.contains(&domain) {
        return Err(AppError::Validation(format!(
            "unknown challenge validator domain: {domain}"
        )));
    }
    let capability = format!("challenge_validator:{domain}");
    require_capability(db, user_id, &capability).await
}

/// Whether this user may review work in a given trade.
///
/// Review rights are granted by family, not by trade: thirty-three code
/// orientations would mean thirty-three capabilities and an operator granting
/// them one at a time, and nobody reviews at that granularity anyway — someone
/// who can judge a React component can judge a Svelte one, and cannot judge a
/// CUDA kernel.
///
/// The family lives on the orientation row rather than in a match here,
/// because orientations are created at runtime through the admin panel and a
/// mapping compiled into the binary would leave every new one unreviewable
/// until somebody deploys.
///
/// `{domain}_reviewer:all` is the wildcard: it covers every family in that
/// domain. Checked second, so the specific grant is the common path.
pub async fn require_reviewer_for_orientation(
    db: &PgPool,
    user_id: Uuid,
    orientation_slug: &str,
) -> Result<(), AppError> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT primary_domain, reviewer_group FROM orientations WHERE slug = $1")
            .bind(orientation_slug)
            .fetch_optional(db)
            .await?;

    let Some((domain, group)) = row else {
        return Err(AppError::Validation(format!(
            "unknown orientation: {orientation_slug}"
        )));
    };

    // No group means nobody has been made responsible for reviewing this
    // trade yet. Refusing is the safe answer, and saying so names the fix.
    let Some(group) = group else {
        return Err(AppError::Validation(format!(
            "orientation '{orientation_slug}' has no reviewer group, so review \
             rights cannot be granted for it"
        )));
    };

    require_any_capability(
        db,
        user_id,
        &[
            &format!("{domain}_reviewer:{group}"),
            &format!("{domain}_reviewer:all"),
        ],
    )
    .await
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
