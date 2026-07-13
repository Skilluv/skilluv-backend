-- Phase P10.3 — Team composition template sur challenge_templates.
-- Migration 0078.
--
-- Rationale :
--   Un challenge game-dev peut prescrire "il faut 1 musicien + 1 animateur_3d
--   + 2 coders + 1 designer pour claim ce challenge en team". Ce template est
--   optionnel : sans lui, les teams s'auto-organisent librement (comportement
--   pré-P10.3).
--
-- Format JSONB attendu :
--   [
--     { "role_slug": "musician", "role_display_name": "Musicien",
--       "required_skill_slug": null, "min_proficiency_level": 1, "count": 1 },
--     { "role_slug": "coder", "role_display_name": "Coder Godot",
--       "required_skill_slug": "rust", "min_proficiency_level": 2, "count": 2 }
--   ]
--
-- Non-breaking : NULL = pas de contrainte de composition (fallback historique).

ALTER TABLE challenge_templates
    ADD COLUMN IF NOT EXISTS team_composition JSONB;

-- Index GIN pour rechercher "quels challenges cherchent des musiciens ?"
CREATE INDEX IF NOT EXISTS idx_challenge_templates_team_composition
    ON challenge_templates USING gin (team_composition)
    WHERE team_composition IS NOT NULL;
