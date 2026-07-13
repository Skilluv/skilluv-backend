-- Phase P12.2 — Marque d'intérêt d'un user pour un projet.
-- Migration 0080.
--
-- Rationale :
--   Après validation email, l'onboarding présente les 3-5 projets recommandés
--   par `projects::recommend_for_user`. Le user coche ceux qui l'intéressent.
--   Cet intérêt scope :
--   - Le feed personnalisé /api/feed/for-you (P12.3) : les slices open des
--     projets favoris sont mises en tête.
--   - Les notifications ciblées (P15) : nouvelle slice sur un projet favori
--     → push mobile.
--   - Les statistiques admin : quels projets attirent quelles skills ?
--
-- Design :
--   - `interest_score` (0-100) : pour graduation future (0 = décoché, 100 =
--     favori très fort). En P12.2 on écrit 50 par défaut sur un "je marque
--     intérêt", 0 sur un "je retire". La logique de gradation est simple pour
--     l'instant, extensible plus tard.
--   - PRIMARY KEY (user_id, project_id) : un seul intérêt par pair.

CREATE TABLE user_project_interests (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    interest_score SMALLINT NOT NULL DEFAULT 50
        CHECK (interest_score BETWEEN 0 AND 100),
    decided_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, project_id)
);

-- Dashboard "mes projets d'intérêt" côté user + feed for-you.
CREATE INDEX idx_user_project_interests_user
    ON user_project_interests (user_id, interest_score DESC)
    WHERE interest_score > 0;

-- Stats admin : "combien d'utilisateurs ont marqué ce projet ?"
CREATE INDEX idx_user_project_interests_project
    ON user_project_interests (project_id)
    WHERE interest_score > 0;
