-- Phase P14.2 — Row-Level Security (POC).
-- Migration 0083.
--
-- Rationale :
--   Le middleware Rust filtre déjà par tenant_id sur les endpoints listing.
--   Un attaquant qui trouve un ID (deliverable_id, attestation_id) peut
--   toutefois y accéder cross-tenant via `GET /api/deliverables/{id}` si le
--   handler ne re-filtre pas. RLS = filet de sécurité au niveau DB : même
--   si une requête oublie le tenant filter, PG bloque.
--
--   Cette migration crée les policies mais N'ACTIVE PAS RLS par défaut :
--   activer RLS = toutes les queries DOIVENT set `app.tenant_id` avant, sinon
--   elles retournent 0 lignes. Notre code ne le fait pas encore. Le
--   déploiement d'un tenant strict passe par :
--     1. Set `SKILLUV_RLS_ENABLED=1` dans .env.
--     2. Ajouter `SET LOCAL app.tenant_id = '{tenant_id}'` dans chaque
--        requête (via un layer axum middleware, à faire post-POC).
--     3. ALTER TABLE deliverables ENABLE ROW LEVEL SECURITY (via migration
--        ou script ops manuel).
--
--   En P14.2 on livre les policies + la fn helper `set_tenant_context()`
--   + un test qui prouve que quand RLS est ON, les queries sans setting
--   ne voient rien, et avec le bon setting elles voient uniquement leur
--   tenant.

-- ═══════════════════════════════════════════════════════════════════
-- Policies (créées mais RLS reste DISABLE par défaut)
-- ═══════════════════════════════════════════════════════════════════

DROP POLICY IF EXISTS tenant_isolation_deliverables ON deliverables;
CREATE POLICY tenant_isolation_deliverables ON deliverables
    USING (
        tenant_id IS NULL
        OR tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
    );

DROP POLICY IF EXISTS tenant_isolation_attestations ON attestations;
CREATE POLICY tenant_isolation_attestations ON attestations
    USING (
        tenant_id IS NULL
        OR tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
    );

-- Note : on ne cree PAS de policy sur user_skills / challenge_submissions /
-- project_slices en POC pour minimiser la surface — les 2 tables les plus
-- exposees a un scan cross-tenant sont deliverables (portfolios publics) et
-- attestations (verifiables publiquement par code).

-- ═══════════════════════════════════════════════════════════════════
-- Helper : set tenant context pour la session courante
-- ═══════════════════════════════════════════════════════════════════
--
-- Appelé par le middleware Rust au début de chaque request (via
-- `sqlx::query!("SELECT set_config(...)")` OU via SET LOCAL dans une
-- transaction).

CREATE OR REPLACE FUNCTION set_tenant_context(p_tenant_id UUID)
RETURNS void AS $$
BEGIN
    PERFORM set_config('app.tenant_id', p_tenant_id::text, false);
END;
$$ LANGUAGE plpgsql;
