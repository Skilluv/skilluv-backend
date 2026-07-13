-- Phase P14.1 — Multi-tenancy full : tenant_id sur les tables sensibles.
-- Migration 0082.
--
-- Rationale :
--   Depuis P0-P8, `tenant_id` est isolé sur `challenges` (renommé
--   `challenge_templates` en P9.3). Le middleware TenantContext filtre le
--   listing des challenges. Mais TOUS les autres artefacts dérivés
--   (deliverables, user_skills, attestations, challenge_submissions,
--   project_slices) restent en scope global — un client entreprise
--   « Skilluv-white-label » verrait potentiellement des attestations d'autres
--   tenants par requête SQL directe.
--
--   Cette migration ajoute `tenant_id` NULLABLE sur ces 5 tables + backfill
--   depuis la source évidente (JOIN challenge_templates via challenge_id ou
--   submission_id). `NULL` = public / root tenant (comportement pré-P14).
--
--   P14.2 (RLS POC) et le filtrage systématique en Rust suivront.

-- ═══════════════════════════════════════════════════════════════════
-- 1. Colonnes tenant_id
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE challenge_submissions
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE deliverables
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE user_skills
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE attestations
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL;

-- ═══════════════════════════════════════════════════════════════════
-- 2. Backfill depuis challenge_templates.tenant_id
-- ═══════════════════════════════════════════════════════════════════

-- Les challenge_submissions héritent du template.
UPDATE challenge_submissions cs
SET tenant_id = ct.tenant_id
FROM challenge_templates ct
WHERE cs.challenge_id = ct.id
  AND cs.tenant_id IS NULL
  AND ct.tenant_id IS NOT NULL;

-- Les deliverables issus d'un challenge_submission héritent du même template.
UPDATE deliverables d
SET tenant_id = ct.tenant_id
FROM challenge_templates ct
WHERE d.challenge_id = ct.id
  AND d.tenant_id IS NULL
  AND ct.tenant_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- 3. Indexes pour filtrage tenant
-- ═══════════════════════════════════════════════════════════════════

CREATE INDEX IF NOT EXISTS idx_challenge_submissions_tenant
    ON challenge_submissions (tenant_id)
    WHERE tenant_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_deliverables_tenant
    ON deliverables (tenant_id)
    WHERE tenant_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_user_skills_tenant
    ON user_skills (tenant_id)
    WHERE tenant_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_attestations_tenant
    ON attestations (tenant_id)
    WHERE tenant_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_project_slices_tenant
    ON project_slices (tenant_id)
    WHERE tenant_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- 4. Triggers d'auto-tag pour les nouvelles rows
-- ═══════════════════════════════════════════════════════════════════
--
-- Pragmatique : plutôt que de propager tenant_id dans chaque INSERT côté
-- Rust (audit surface énorme), on utilise des triggers BEFORE INSERT qui
-- dérivent tenant_id depuis la ressource parente évidente. Le code Rust
-- peut aussi set tenant_id explicitement (ex: depuis TenantContext), auquel
-- cas le trigger respecte la valeur fournie.

-- challenge_submissions : hérite de challenge_templates via challenge_id.
CREATE OR REPLACE FUNCTION set_challenge_submission_tenant()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.tenant_id IS NULL AND NEW.challenge_id IS NOT NULL THEN
        SELECT tenant_id INTO NEW.tenant_id
        FROM challenge_templates
        WHERE id = NEW.challenge_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_challenge_submissions_tenant ON challenge_submissions;
CREATE TRIGGER trg_challenge_submissions_tenant
    BEFORE INSERT ON challenge_submissions
    FOR EACH ROW
    EXECUTE FUNCTION set_challenge_submission_tenant();

-- deliverables : hérite de challenge_id (si set) sinon de slice_id → projet
-- (les projets n'ont pas encore de tenant_id, mais peuvent en avoir en P14.2+).
CREATE OR REPLACE FUNCTION set_deliverable_tenant()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.tenant_id IS NULL AND NEW.challenge_id IS NOT NULL THEN
        SELECT tenant_id INTO NEW.tenant_id
        FROM challenge_templates
        WHERE id = NEW.challenge_id;
    END IF;
    IF NEW.tenant_id IS NULL AND NEW.slice_id IS NOT NULL THEN
        SELECT tenant_id INTO NEW.tenant_id
        FROM project_slices
        WHERE id = NEW.slice_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_deliverables_tenant ON deliverables;
CREATE TRIGGER trg_deliverables_tenant
    BEFORE INSERT ON deliverables
    FOR EACH ROW
    EXECUTE FUNCTION set_deliverable_tenant();

-- attestations : dérivé du user (users.primary_tenant_id).
CREATE OR REPLACE FUNCTION set_attestation_tenant()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
        SELECT primary_tenant_id INTO NEW.tenant_id
        FROM users
        WHERE id = NEW.user_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_attestations_tenant ON attestations;
CREATE TRIGGER trg_attestations_tenant
    BEFORE INSERT ON attestations
    FOR EACH ROW
    EXECUTE FUNCTION set_attestation_tenant();

-- user_skills : dérivé du user.
CREATE OR REPLACE FUNCTION set_user_skill_tenant()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
        SELECT primary_tenant_id INTO NEW.tenant_id
        FROM users
        WHERE id = NEW.user_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_user_skills_tenant ON user_skills;
CREATE TRIGGER trg_user_skills_tenant
    BEFORE INSERT ON user_skills
    FOR EACH ROW
    EXECUTE FUNCTION set_user_skill_tenant();

-- project_slices : dérivé du created_by_user_id ou du funded_by_user_id (bounty B2B).
CREATE OR REPLACE FUNCTION set_project_slice_tenant()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
        SELECT primary_tenant_id INTO NEW.tenant_id
        FROM users
        WHERE id = COALESCE(NEW.funded_by_user_id, NEW.created_by_user_id);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_project_slices_tenant ON project_slices;
CREATE TRIGGER trg_project_slices_tenant
    BEFORE INSERT ON project_slices
    FOR EACH ROW
    EXECUTE FUNCTION set_project_slice_tenant();

