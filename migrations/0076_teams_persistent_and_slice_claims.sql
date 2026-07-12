-- Phase P10.1 — Teams persistentes + team-claim sur project_slices.
-- Migration 0076.
--
-- Rationale :
--   Avant P10, les teams sont éphémères (une team par challenge, meurt au submit).
--   La décision Q2 du roadmap original ("pas de slice_teams avant P6") est enfin
--   inversée — les teams peuvent maintenant :
--     - Persister au-delà d'un challenge (`is_persistent = TRUE`)
--     - Claimer une slice (via `project_slices.claimed_by_team_id`)
--     - Être ré-utilisées sur plusieurs challenges/slices
--
-- Non-breaking :
--   - `challenge_teams.challenge_id` devient nullable pour supporter les teams
--     persistentes créées hors contexte d'un challenge.
--   - `project_slices.claimed_by_user_id` reste (compat solo claim), et un nouveau
--     `claimed_by_team_id` alternatif s'ajoute — XOR strict (jamais les deux).

-- ═══════════════════════════════════════════════════════════════════
-- 1. challenge_teams : nullable challenge_id + is_persistent
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE challenge_teams
    ALTER COLUMN challenge_id DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS is_persistent BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS disbanded_at TIMESTAMPTZ;

-- Cohérence : une team non-persistente DOIT référencer un challenge.
-- Une team persistente peut vivre sans challenge_id.
ALTER TABLE challenge_teams
    ADD CONSTRAINT challenge_teams_persistence_coherent
    CHECK (
        (is_persistent = FALSE AND challenge_id IS NOT NULL)
        OR (is_persistent = TRUE)
    );

CREATE INDEX IF NOT EXISTS idx_challenge_teams_persistent
    ON challenge_teams (is_persistent, disbanded_at)
    WHERE is_persistent = TRUE AND disbanded_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- 2. project_slices : team-claim en alternative au user-claim
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS claimed_by_team_id UUID
        REFERENCES challenge_teams(id) ON DELETE SET NULL;

-- Drop l'ancienne contrainte user-only et remplace par XOR user/team.
ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_claim_coherent;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_claim_coherent
    CHECK (
        -- Aucun claim : les 3 champs sont NULL
        (claimed_by_user_id IS NULL AND claimed_by_team_id IS NULL AND claimed_at IS NULL)
        -- Claim solo : user set, team NULL, claimed_at set
        OR (claimed_by_user_id IS NOT NULL AND claimed_by_team_id IS NULL AND claimed_at IS NOT NULL)
        -- Claim team : team set, user NULL, claimed_at set
        OR (claimed_by_user_id IS NULL AND claimed_by_team_id IS NOT NULL AND claimed_at IS NOT NULL)
    );

-- Dashboard "mes claims via une team" côté user
CREATE INDEX IF NOT EXISTS idx_slices_claimed_by_team
    ON project_slices (claimed_by_team_id, status)
    WHERE claimed_by_team_id IS NOT NULL;
