# RLS enforcement — activation prod

**Statut au 2026-07-14** : POC livré en P14.2 (migration 0083),
helpers Rust en P22.1 (`src/services/rls.rs`). Enforcement OFF par
défaut.

## Ce qui est déjà en place

- Migration 0083 attache les policies `tenant_isolation_deliverables`
  et `tenant_isolation_attestations` avec `USING (tenant_id IS NULL OR
  tenant_id = current_setting('app.tenant_id'))`.
- Fonction PG `set_tenant_context(uuid)` qui appelle
  `set_config('app.tenant_id', ..., false)`.
- Colonne `tenant_id UUID` avec triggers auto-tag depuis parent
  (migration 0082) sur 5 tables sensibles.
- Helper Rust `services::rls::set_tenant_context_on_tx(tx, tenant_id)`
  no-op sauf si `SKILLUV_RLS_ENFORCED=1`.

## Ce qui manque pour activer en prod

### 1. Créer un rôle PostgreSQL NOSUPERUSER NOBYPASSRLS

Le rôle `skilluv` actuel est superuser et bypass RLS d'office.
Il faut un rôle dédié app :

```sql
CREATE ROLE skilluv_app NOSUPERUSER NOBYPASSRLS LOGIN
    PASSWORD 'change_me_in_prod';
GRANT CONNECT ON DATABASE skilluv TO skilluv_app;
GRANT USAGE ON SCHEMA public TO skilluv_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public
    TO skilluv_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO skilluv_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO skilluv_app;
GRANT EXECUTE ON FUNCTION set_tenant_context(UUID) TO skilluv_app;
```

Migrations continuent de tourner sous `skilluv` (superuser) pour
attacher/detacher policies.

### 2. Configurer sqlx à utiliser le rôle app

`DATABASE_URL` de prod pointe sur `skilluv_app` :
```
postgres://skilluv_app:$PASSWORD@host:5432/skilluv
```

Les migrations gardent l'ancien URL `skilluv:...` — configurable via
`SQLX_MIGRATIONS_URL` séparé.

### 3. Wrapper chaque code path tenant-scoped dans une transaction

Là où les données tenant-scoped sont queried (deliverables,
attestations, user_skills, project_slices, challenge_submissions) :

```rust
let mut tx = state.db.begin().await?;
services::rls::set_tenant_context_on_tx(&mut tx, tenant_id).await?;
// ... queries filtrées automatiquement par RLS ...
tx.commit().await?;
```

**Estimation** : ~30-50 call sites à refactorer. Pas fait en P22.1 par
scope. À déployer progressivement, service par service.

### 4. Activer l'env `SKILLUV_RLS_ENFORCED=1`

Une fois les code paths refactorés, mettre cette env en prod.

### 5. Tests d'intégration

Créer `tests/test_rls_enforcement.rs` :
- User A crée une deliverable dans tenant X → visible par A.
- User B dans tenant Y → NE VOIT PAS la deliverable de A.
- Requête sans tenant_context → deny (0 rows retournées).
- Admin bypass : quand `role != 'skilluv_app'`, RLS bypass normal.

## Trade-offs

**Avantages** de l'enforcement RLS :
- Défense en profondeur : même si une query oublie le filtre tenant_id,
  la DB bloque.
- Isolation stricte pour clients enterprise cloisonnés.
- Base pour futur "self-hosted single-tenant" (chaque tenant = son
  propre schema/DB).

**Coûts** :
- Wrapping obligatoire en transactions → +1 round-trip par request.
- Debug plus dur (0 rows au lieu d'erreur explicite).
- Migrations sqlx doivent être annotées sur les tables tenant-scoped.
- Pooling connections plus tricky (session state via
  `set_config(..., false)` scope à la session, `SET LOCAL` scope à la
  tx — on utilise le premier pour éviter tx obligatoire).

## Reco stratégique

**Activer RLS uniquement quand un client enterprise le demande
explicitement** (contrat de compliance).

Pour du multi-tenant SaaS standard, les triggers auto-tag (P14.1) +
filtres applicatifs sur `tenant_id` suffisent en pratique — le POC RLS
sert de plan B / preuve de conformité SOC2/ISO27001.
