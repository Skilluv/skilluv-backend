-- Phase P0 — Fondations du modèle cible.
-- Migration 0059 : table M2M `slice_skills` (rattachement slice ↔ skill).
--
-- Rationale :
--   Une slice exerce plusieurs skills atomiques (une PR sur un endpoint peut
--   toucher `rest-design`, `sql-joins`, `axum-route-handlers` en même temps).
--   Le poids `weight` (1-5) exprime l'intensité de l'exercice sur ce skill :
--     - weight=1 : effleuré (ex: quelques lignes touchées)
--     - weight=3 : contribue clairement à ce skill (défaut usuel)
--     - weight=5 : le skill est le cœur de cette slice (ex: refactoring dédié)
--
--   Le weight alimente la formule de proficiency (voir docs/... partie G.2) :
--     weighted_proven_count = SUM(weight) accumulés
--     proficiency_level = min(5, ceil(log2(WPC + 1)))
--
-- Choix de conception :
--   - `is_primary` marque le "skill principal" de la slice, utilisé pour l'affichage
--     et la recommandation. Une slice peut avoir 0 ou 1 skill primary.
--   - Pas de contrainte SQL sur "au moins 1 skill par slice" — laissé à la logique
--     applicative (le steward valide le tag lors du curator_review).

CREATE TABLE slice_skills (
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES skill_nodes(id) ON DELETE CASCADE,
    weight SMALLINT NOT NULL DEFAULT 3 CHECK (weight BETWEEN 1 AND 5),
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (slice_id, skill_id)
);

-- Trouver les slices qui touchent un skill donné
-- ("montre-moi les slices open qui exerceraient mon skill X")
CREATE INDEX idx_slice_skills_skill
    ON slice_skills (skill_id, weight DESC);

-- Assurer qu'il n'y a pas plus d'un skill primary par slice
CREATE UNIQUE INDEX uniq_slice_primary_skill
    ON slice_skills (slice_id)
    WHERE is_primary = TRUE;
