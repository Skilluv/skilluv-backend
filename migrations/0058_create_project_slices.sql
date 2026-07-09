-- Phase P0 — Fondations du modèle cible.
-- Migration 0058 : création de la table `project_slices` (unité de travail réelle).
--
-- Rationale :
--   `project_slices` généralise le pattern éprouvé de `oss_bounties`/`oss_bounty_claims`
--   (voir migration 0042). Une slice représente un scope de travail claim-able
--   exclusivement par un user sur un projet réel :
--     - une issue GitHub à résoudre
--     - un composant Figma à livrer
--     - un niveau de jeu à concevoir
--     - une cible pentest à investiguer
--     - une tâche CLI (ex: rebase interactive, refactoring)
--
--   Le workflow (documenté dans docs/challenges-target-model-and-roadmap.md
--   partie G.1) : ingestion → open → claimed (par un user, exclusif, 7j) →
--   in_review → merged (auto via webhook GitHub, ou manual via review humaine).
--
-- Choix de conception :
--   - `slice_type` détermine comment l'artefact est produit et vérifié.
--   - `external_ref` pointe vers la source externe (issue URL, Figma node ID, etc.).
--   - `credits_reward` NUMERIC(10,2) pour absorber le cas bounty enterprise
--     (dans les autres cas, reste à 0). En Phase P8, `oss_bounties` sera drop
--     et fusionné ici avec `credits_reward > 0`.
--   - `claim_expires_at` : implémentation du soft-lock, mêmes 7j que les bounties.
--   - `ingested_from` pour tracer l'origine (webhook, manuel, IA, import partenaire).
--   - Aucune FK slice → challenge template ici (décision Q1 session 2026-07-09 : YAGNI).
--   - Aucune notion de team ici (décision Q2 : slice_teams reporté à Phase P6).

CREATE TABLE project_slices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    -- Type de slice + référence externe
    slice_type VARCHAR(30) NOT NULL
        CHECK (slice_type IN (
            'github_issue',       -- résolue via PR mergée + webhook (workflow G.1)
            'figma_frame',        -- livrée via URL Figma frame + review humaine
            'game_level',         -- livrée via build jouable + review humaine
            'game_asset',         -- livrée via fichier (.blend, .png, .fbx) + review
            'sec_target',         -- rapport de vuln, disclosure, ou fix mergé
            'cli_task',           -- geste CLI (git, docker) vérifié via sandbox (Phase P3+)
            'design_token',       -- token JSON W3C CG format + review
            'documentation',      -- doc mergée dans le repo du projet
            'other'               -- fallback, requires human_review
        )),
    external_ref TEXT,
    external_metadata JSONB,

    -- Contenu de la tâche
    title VARCHAR(300) NOT NULL,
    description TEXT NOT NULL,
    acceptance_criteria TEXT,

    -- Classification pour le matching skill graph + affichage
    primary_domain VARCHAR(30) NOT NULL
        CHECK (primary_domain IN ('code','design','game','security','soft_skills','ai','ops')),
    difficulty SMALLINT NOT NULL CHECK (difficulty BETWEEN 1 AND 5),
    estimated_hours INTEGER CHECK (estimated_hours IS NULL OR estimated_hours > 0),
    fragments_reward INTEGER NOT NULL DEFAULT 50 CHECK (fragments_reward >= 0),
    credits_reward NUMERIC(10,2) NOT NULL DEFAULT 0 CHECK (credits_reward >= 0),

    -- Claim exclusif (voir workflow G.1)
    status VARCHAR(20) NOT NULL DEFAULT 'draft'
        CHECK (status IN (
            'draft',        -- ingested but not yet published (curator_review mode)
            'open',         -- visible, claim-able by any eligible user
            'claimed',      -- reserved by a user, working on it
            'in_review',    -- artefact soumis, en attente de vérification
            'merged',       -- terminée avec succès
            'closed',       -- fermée sans succès (ex: obsolete, duplicate)
            'expired'       -- claim expiré, retournera au pool comme 'open' via job
        )),
    claimed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,

    -- Provenance (utile pour debug, curation, et attributions futures)
    created_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ingested_from VARCHAR(30) NOT NULL DEFAULT 'manual'
        CHECK (ingested_from IN (
            'manual',             -- créée à la main par un steward via UI admin
            'github_webhook',     -- ingested auto depuis un webhook GitHub issue
            'ai_ingested',        -- créée par un pipeline IA (Phase P5+)
            'partner_import',     -- imported depuis un partenaire externe
            'legacy_bounty'       -- migrée depuis oss_bounties en P1
        )),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMPTZ
);

-- Cohérence claim : si claimed_by est set, claimed_at doit l'être aussi
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_claim_coherent
    CHECK (
        (claimed_by_user_id IS NULL AND claimed_at IS NULL)
        OR (claimed_by_user_id IS NOT NULL AND claimed_at IS NOT NULL)
    );

-- Recherche des slices ouvertes par projet (dashboard steward)
CREATE INDEX idx_slices_project_status
    ON project_slices (project_id, status);

-- Vue "backlog global" — les slices disponibles pour claim, tri par date
CREATE INDEX idx_slices_open_backlog
    ON project_slices (status, created_at DESC)
    WHERE status = 'open';

-- Trouver les slices d'un user (pour "mes tâches en cours")
CREATE INDEX idx_slices_claimed_by
    ON project_slices (claimed_by_user_id, status)
    WHERE claimed_by_user_id IS NOT NULL;

-- Trouver les slices proches d'expirer (job de nettoyage + notif J+5, décision W1)
CREATE INDEX idx_slices_expiring
    ON project_slices (claim_expires_at)
    WHERE status = 'claimed' AND claim_expires_at IS NOT NULL;

-- Filtrage par domaine + difficulté (page "opportunities" côté frontend)
CREATE INDEX idx_slices_domain_difficulty
    ON project_slices (primary_domain, difficulty)
    WHERE status = 'open';

-- Empêcher deux slices pour la même issue GitHub sur un même projet
CREATE UNIQUE INDEX uniq_slices_github_issue_per_project
    ON project_slices (project_id, external_ref)
    WHERE slice_type = 'github_issue' AND external_ref IS NOT NULL;

-- Trigger updated_at
CREATE OR REPLACE FUNCTION touch_project_slices_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER project_slices_updated_at
    BEFORE UPDATE ON project_slices
    FOR EACH ROW
    EXECUTE FUNCTION touch_project_slices_updated_at();
