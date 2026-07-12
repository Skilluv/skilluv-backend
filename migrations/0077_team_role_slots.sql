-- Phase P10.2 — Rôles multidisciplinaires sur teams.
-- Migration 0077.
--
-- Rationale :
--   Les teams actuelles sont composées de N users interchangeables. Pour un
--   challenge game-dev réaliste il faut modéliser la composition attendue :
--   1 musicien + 1 animateur_3d + 2 coders + 1 designer, etc.
--
-- Design :
--   - `role_slug` est libre-forme (VARCHAR) — pas d'énum contraignant, la
--     communauté définit ses vocabulaires (musician, illustrator, animator_3d,
--     coder, designer, sound_designer, game_writer, level_designer, etc.).
--   - `required_skill_id` optionnel : si set, le user qui join le slot doit
--     avoir `user_skills.proven_count > 0` sur ce skill (validation best-effort
--     côté service, pas de CHECK cross-table).
--   - `filled_by_user_id` NULL = slot ouvert. NOT NULL + `filled_at` set = slot
--     rempli.
--   - Cohérence : (filled_by NULL XOR filled_at NULL) → CHECK.
--   - Un user peut occuper au plus 1 slot par team (contrainte UNIQUE partielle).

CREATE TABLE team_role_slots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES challenge_teams(id) ON DELETE CASCADE,
    role_slug VARCHAR(60) NOT NULL
        CHECK (length(role_slug) BETWEEN 2 AND 60),
    role_display_name VARCHAR(100),
    required_skill_id UUID REFERENCES skill_nodes(id) ON DELETE SET NULL,
    min_proficiency_level SMALLINT NOT NULL DEFAULT 1
        CHECK (min_proficiency_level BETWEEN 1 AND 5),
    filled_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    filled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT team_role_slots_fill_coherent
        CHECK (
            (filled_by_user_id IS NULL AND filled_at IS NULL)
            OR (filled_by_user_id IS NOT NULL AND filled_at IS NOT NULL)
        )
);

-- Dashboard team : "quels slots reste-t-il à remplir ?"
CREATE INDEX idx_team_role_slots_team
    ON team_role_slots (team_id, filled_by_user_id NULLS FIRST);

-- Un user occupe au plus 1 slot par team (règle métier : pas de double-rôle
-- dans la même équipe pour éviter la sur-attribution de fragments).
CREATE UNIQUE INDEX uniq_team_role_slots_user
    ON team_role_slots (team_id, filled_by_user_id)
    WHERE filled_by_user_id IS NOT NULL;

-- Recherche "slots ouverts pour rôle X" (marketplace teams cherchent musicien)
CREATE INDEX idx_team_role_slots_open_by_role
    ON team_role_slots (role_slug, filled_by_user_id)
    WHERE filled_by_user_id IS NULL;
