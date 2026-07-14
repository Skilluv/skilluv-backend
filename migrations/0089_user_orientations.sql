-- Phase P16.2 — user_orientations : ce que chaque user déclare comme métier.
-- Migration 0089.
--
-- Rationale :
--   `users.skill_domain` (mig 0002, nullable depuis 0049) est un enum étroit
--   (code/design/game/security…) — un choix mono-valué, sans notion d'évolution
--   ni de mode "learning vs prouvé". P16.1 a introduit `orientations` (30+
--   métiers). P16.2 branche les users dessus.
--
--   Règles produit clés :
--   - Un user peut avoir plusieurs orientations simultanément.
--   - Cap raisonnable de 3 orientations "actives" (learning ou active) à
--     l'inscription — au-delà, il faut avoir prouvé par artefacts (contrainte
--     applicative, pas DB — permet l'exception admin).
--   - Exactement 1 orientation `is_primary=TRUE` par user (celle affichée en
--     grand sur le profil).
--   - `mode='learning'` = "je veux apprendre X" (aspirationnel, ne pollue pas
--     le search recruteur par défaut).
--   - `mode='active'` = "j'ai prouvé X" (au moins 1 artefact sur un skill core
--     de l'orientation). Promotion learning→active peut être automatique.
--   - `ended_at` NOT NULL = orientation historisée (le user a bougé). Garde
--     l'historique visible ("3 ans dev-frontend actif · reconversion pentest
--     depuis 6 mois") — hyper précieux pour les recruteurs qui cherchent des
--     profils en reconversion (wedge Skilluv).

CREATE TABLE user_orientations (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE RESTRICT,
    mode VARCHAR(10) NOT NULL DEFAULT 'learning'
        CHECK (mode IN ('learning', 'active')),
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Historisation : ended_at set = user a quitté cette orientation.
    -- Ne PAS supprimer la ligne — l'historique est un asset produit.
    ended_at TIMESTAMPTZ,
    working_languages TEXT[] NOT NULL DEFAULT ARRAY['fr']::TEXT[],
    -- IANA tz, ex "Africa/Abidjan", "Europe/Paris".
    timezone VARCHAR(60),
    -- Note libre du user ("je veux basculer vers ça d'ici Q4", "3 ans d'XP en
    -- entreprise", etc.) — enrichit le profil public.
    notes TEXT,

    PRIMARY KEY (user_id, orientation_id),

    -- Cohérence : `active` implique que le user a au moins 1 preuve, mais on
    -- ne peut pas cross-checker en CHECK. Contrôle applicatif en P16.3.
    CONSTRAINT user_orientations_end_after_start
        CHECK (ended_at IS NULL OR ended_at >= started_at)
);

-- Un seul primary par user (parmi les orientations non-ended).
CREATE UNIQUE INDEX uniq_user_orientations_primary
    ON user_orientations (user_id)
    WHERE is_primary = TRUE AND ended_at IS NULL;

-- Recherche recruteur : "tous les users dev-frontend en mode active".
CREATE INDEX idx_user_orientations_search
    ON user_orientations (orientation_id, mode)
    WHERE ended_at IS NULL;

-- Profil : "toutes les orientations du user X, actives d'abord".
CREATE INDEX idx_user_orientations_by_user
    ON user_orientations (user_id, ended_at NULLS FIRST, is_primary DESC);

-- ═══════════════════════════════════════════════════════════════════
-- BACKFILL — Convertit `users.skill_domain` (mono-valué) en une première
-- orientation par user, best-effort.
--
-- Mapping domain → orientation par défaut :
--   code         → dev-fullstack (T-shaped default)
--   design       → web-designer
--   game         → game-programmer
--   security     → pentester-web
--   ai           → prompt-engineer
--   ops          → devops-engineer
--   soft_skills  → tech-writer
--
-- Mode : active si le user a >= 1 ligne dans user_skills (donc au moins 1
-- preuve), sinon learning.
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO user_orientations (user_id, orientation_id, mode, is_primary, started_at)
SELECT
    u.id AS user_id,
    o.id AS orientation_id,
    CASE
        WHEN EXISTS (SELECT 1 FROM user_skills us WHERE us.user_id = u.id) THEN 'active'
        ELSE 'learning'
    END AS mode,
    TRUE AS is_primary,
    COALESCE(u.created_at, NOW()) AS started_at
FROM users u
JOIN orientations o
    ON o.slug = CASE u.skill_domain
        WHEN 'code'        THEN 'dev-fullstack'
        WHEN 'design'      THEN 'web-designer'
        WHEN 'game'        THEN 'game-programmer'
        WHEN 'security'    THEN 'pentester-web'
        WHEN 'ai'          THEN 'prompt-engineer'
        WHEN 'ops'         THEN 'devops-engineer'
        WHEN 'soft_skills' THEN 'tech-writer'
        ELSE NULL
    END
WHERE u.skill_domain IS NOT NULL
ON CONFLICT DO NOTHING;
