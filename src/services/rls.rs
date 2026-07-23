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
//!
//! ## Sites d'appel à patcher lors de l'activation prod
//!
//! Ces call sites font des INSERT/UPDATE sur des tables tenant-scoped
//! (deliverables, attestations, user_badges, hello_wall_entries, etc.). Si
//! RLS est activé (`SKILLUV_RLS_ENFORCED=1`) et le rôle SQL passe en
//! NOSUPERUSER NOBYPASSRLS, chacun de ces sites doit appeler
//! `set_tenant_context_on_tx(&mut tx, tenant_id)` juste après `db.begin()`,
//! avant toute query dans la tx.
//!
//! Snapshot au 2026-07-23 (priorite basse #7 strategy doc §15) :
//!
//! - `services::deliverables::DeliverableService::insert_deliverable_verified`
//!   (4 INSERT sites au total : pr_merged webhook, manual submit, capstone,
//!   admin fixture). Tenant_id derive de user.tenant_id ou
//!   challenge_template.tenant_id.
//! - `routes::onboarding::handle_bonjour_skilluv_pr_event` (INSERT
//!   hello_wall_entries + UPDATE onboarding_bonjour_skilluv). Tenant_id
//!   derive de user.tenant_id.
//! - `services::attestations` (tous les INSERT). Tenant_id derive de user.
//! - `services::badge_engine::recompute_badges_for_user` (INSERT user_badges).
//!
//! Le pattern d'activation :
//! 1. Passer `SKILLUV_RLS_ENFORCED=1` en staging
//! 2. Faire tourner la suite d'integration — tous les tests deliverables/
//!    attestations vont echouer (0 rows visibles ou insert refuse selon les
//!    policies)
//! 3. Patcher chaque site en ajoutant `rls::set_tenant_context_on_tx` en tete
//!    de tx
//! 4. Roll out en prod avec le nouveau role SQL NOSUPERUSER NOBYPASSRLS

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
