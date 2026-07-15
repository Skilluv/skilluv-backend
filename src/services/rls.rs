//! P22.1 — Helpers pour RLS enforcement (Row-Level Security tenant isolation).
//!
//! Les policies RLS ont été livrées en P14.2 (mig 0083) avec `set_tenant_context()`
//! côté PostgreSQL. En dev les policies sont ATTACHÉES mais NON ENFORCED
//! (le rôle `skilluv` est superuser et bypass RLS).
//!
//! Ce module fournit les helpers Rust pour appeler `set_tenant_context` au bon
//! endroit dans les transactions applicatives — préparation à l'activation prod
//! quand un rôle NOSUPERUSER NOBYPASSRLS sera en place.
//!
//! ## Contrat d'usage prod
//!
//! ```ignore
//! let mut tx = state.db.begin().await?;
//! rls::set_tenant_context_on_tx(&mut tx, tenant_id).await?;
//! // ... toutes les queries dans cette tx sont filtrées par tenant.
//! tx.commit().await?;
//! ```
//!
//! Voir `docs/RLS-ENFORCEMENT.md` pour la procédure d'activation prod complète
//! (création du rôle, migration de la config sqlx, tests d'intégration).

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;

/// True si l'enforcement RLS est activé côté application (env
/// `SKILLUV_RLS_ENFORCED=1`). En dev/tests reste false → les helpers
/// suivants sont des no-ops silencieux, ce qui permet d'ajouter les appels
/// dans le code sans casser les tests actuels.
pub fn is_enforced() -> bool {
    std::env::var("SKILLUV_RLS_ENFORCED").as_deref() == Ok("1")
}

/// Set le tenant context sur une transaction. Silencieux si RLS non enforced.
/// À appeler comme première query après `db.begin()` dans les code paths
/// tenant-scoped.
pub async fn set_tenant_context_on_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<(), AppError> {
    if !is_enforced() {
        return Ok(());
    }
    sqlx::query("SELECT set_tenant_context($1)")
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
