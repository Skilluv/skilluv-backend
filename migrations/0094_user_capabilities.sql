-- Phase P18.1 — user_capabilities cumulables (personas plateforme).
-- Migration 0094.
--
-- Rationale :
--   `users.role` (mig 0002/0011) est un enum étroit à 5 valeurs mutuellement
--   exclusives : {user, mentor, admin, enterprise, recruiter}. C'est
--   insuffisant : un même user peut être *simultanément* challenger + mentor
--   + pr_reviewer + issue_proposer + bounty_funder. Un projet steward peut
--   aussi être un jury de tournoi. Le monde réel est cumulable, pas mutuellement
--   exclusif.
--
--   `user_capabilities` = 3ᵉ axe orthogonal (skills / orientations /
--   capabilities). Discussion produit : voir memory
--   `project_p17_completion.md` section "Prochaine phase possible".
--
--   Règles produit clés :
--   - Cumulable : un user peut avoir N capabilities actives simultanément.
--   - Gagnées, pas déclarées : chaque capability a un critère mesurable
--     (challenger = inscription, mentor = 5 attestations données, etc.).
--     Le service capabilities_engine (P18.2) applique les seuils.
--   - Révocables + expirables : un mentor inactif 6 mois peut perdre la
--     capability. `expires_at` supporte les nominations temporaires
--     (jury_tournament pour l'édition Y).
--   - Découplées de la permission : la capability dit "peut faire X".
--     Le middleware require_capability(cap) vérifie (P18.3).

CREATE TABLE user_capabilities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    capability VARCHAR(30) NOT NULL
        CHECK (capability IN (
            'challenger',             -- défaut, tout user inscrit
            'mentor',                  -- 5 attestations données OU nommé
            'project_steward',         -- owner d'un project OSS Skilluv
            'pr_reviewer',             -- 10 PR reviewed approuvées
            'bounty_funder',           -- a financé >= 1 bounty
            'issue_proposer',          -- 3 issues acceptées comme challenges
            'jury_tournament',         -- nommé par admin pour un tournoi
            'admin',                   -- droit plateforme complet
            'enterprise_recruiter'     -- accès talent search + shortlists
        )),
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Explication humaine ("auto:threshold_met", "admin:jane", "backfill_from_users_role").
    granted_reason TEXT NOT NULL DEFAULT 'unspecified',
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    -- Expiration : NULL = illimité, sinon date après laquelle la capability
    -- n'est plus active (utile jury_tournament, bounty_funder saisonnier).
    expires_at TIMESTAMPTZ,
    -- Révocation soft : preserve l'historique pour l'audit ("Jean a été
    -- mentor de 2024 à 2026, révoqué pour inactivité").
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT
);

-- Un user ne peut détenir la même capability qu'UNE FOIS active à la fois.
-- La partial UNIQUE laisse les rows revoked historisées sans bloquer.
CREATE UNIQUE INDEX uniq_user_capabilities_active
    ON user_capabilities (user_id, capability)
    WHERE revoked_at IS NULL;

-- Query "toutes les capabilities actives du user X"
CREATE INDEX idx_user_capabilities_by_user
    ON user_capabilities (user_id, capability)
    WHERE revoked_at IS NULL;

-- Recherche "qui sont les mentors ?" pour la modération / assignation.
CREATE INDEX idx_user_capabilities_by_cap
    ON user_capabilities (capability, granted_at DESC)
    WHERE revoked_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- BACKFILL depuis users.role (enum historique).
-- Mapping :
--   role='user'/'mentor'/'admin'/'enterprise'/'recruiter' → challenger
--   role='mentor'                                          → challenger + mentor
--   role='admin'                                           → challenger + admin
--   role='enterprise'/'recruiter'                          → challenger + enterprise_recruiter
-- ═══════════════════════════════════════════════════════════════════

-- 1. Tout le monde = challenger (défaut universel).
INSERT INTO user_capabilities (user_id, capability, granted_reason)
SELECT id, 'challenger', 'backfill:default_all_users' FROM users
ON CONFLICT DO NOTHING;

-- 2. mentor
INSERT INTO user_capabilities (user_id, capability, granted_reason)
SELECT id, 'mentor', 'backfill:from_users_role'
FROM users WHERE role = 'mentor'
ON CONFLICT DO NOTHING;

-- 3. admin
INSERT INTO user_capabilities (user_id, capability, granted_reason)
SELECT id, 'admin', 'backfill:from_users_role'
FROM users WHERE role = 'admin'
ON CONFLICT DO NOTHING;

-- 4. enterprise_recruiter (rassemble enterprise + recruiter historiques)
INSERT INTO user_capabilities (user_id, capability, granted_reason)
SELECT id, 'enterprise_recruiter', 'backfill:from_users_role'
FROM users WHERE role IN ('enterprise', 'recruiter')
ON CONFLICT DO NOTHING;
