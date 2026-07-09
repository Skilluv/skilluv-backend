-- Phase P6 — Seasons + stewards.
-- Migration 0069 : étendre `seasons` (créée en 0030 pour tournaments), créer
-- `project_seasons` et `project_stewards`.
--
-- Rationale : la table `seasons` existe déjà depuis la migration 0030 pour
-- rythmer les tournaments. On la réutilise pour le rythme narratif produit
-- (voir docs/challenges-target-model-and-roadmap.md sections B.13, 9.4, 9.5).
-- Une saison porte à la fois les tournois ET le contexte narratif projets.

-- ═══════════════════════════════════════════════════════════════════
-- Extensions de seasons (déjà existante depuis 0030)
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE seasons
    ADD COLUMN IF NOT EXISTS theme TEXT,
    ADD COLUMN IF NOT EXISTS retrospective_report_url TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Backfill theme pour les seasons existantes (utilisation `description` comme fallback)
UPDATE seasons SET theme = COALESCE(description, name) WHERE theme IS NULL;

-- Après backfill, on peut rendre theme NOT NULL
ALTER TABLE seasons ALTER COLUMN theme SET NOT NULL;

-- Étendre le CHECK sur status pour supporter 'completed' et 'archived' en plus
-- de 'ended' (garde-t-on 'ended' pour compat backward).
ALTER TABLE seasons DROP CONSTRAINT IF EXISTS seasons_status_check;
ALTER TABLE seasons ADD CONSTRAINT seasons_status_check
    CHECK (status IN ('upcoming','active','ended','completed','archived'));

CREATE INDEX IF NOT EXISTS idx_seasons_active_or_upcoming
    ON seasons (status)
    WHERE status IN ('upcoming','active');

CREATE OR REPLACE FUNCTION touch_seasons_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS seasons_updated_at ON seasons;
CREATE TRIGGER seasons_updated_at
    BEFORE UPDATE ON seasons
    FOR EACH ROW
    EXECUTE FUNCTION touch_seasons_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- project_seasons (M2M projet ↔ saison)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE project_seasons (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    focus_type VARCHAR(20) NOT NULL DEFAULT 'primary'
        CHECK (focus_type IN ('primary','featured','sponsor')),

    PRIMARY KEY (project_id, season_id)
);

CREATE INDEX idx_project_seasons_season
    ON project_seasons (season_id, focus_type);

-- ═══════════════════════════════════════════════════════════════════
-- project_stewards
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE project_stewards (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(30) NOT NULL
        CHECK (role IN (
            'lead_steward',
            'co_steward',
            'domain_lead_code',
            'domain_lead_design',
            'domain_lead_sec',
            'domain_lead_game',
            'mediator'
        )),
    appointed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    appointed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    PRIMARY KEY (project_id, user_id, role)
);

CREATE INDEX idx_project_stewards_project_active
    ON project_stewards (project_id, role)
    WHERE ended_at IS NULL;

CREATE INDEX idx_project_stewards_user_active
    ON project_stewards (user_id, project_id)
    WHERE ended_at IS NULL;
