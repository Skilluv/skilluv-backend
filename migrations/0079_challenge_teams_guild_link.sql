-- Phase P10.5 — Bridge Guild ↔ Team.
-- Migration 0079.
--
-- Rationale :
--   Une team persistente peut être marquée « team officielle de la guilde X ».
--   Quand cette team submit un team challenge avec succès :
--   - Chaque membre reçoit son 10% GP habituel (via services/guild::award_gp_for_fragments).
--   - En plus, la guilde reçoit un bonus collectif de 10% du total (matérialise
--     le fait que la victoire team = coup de force pour l'ensemble de la guilde).
--
-- Non-breaking : `guild_id` nullable — une team ne DOIT PAS appartenir à une guilde.

ALTER TABLE challenge_teams
    ADD COLUMN IF NOT EXISTS guild_id UUID
        REFERENCES guilds(id) ON DELETE SET NULL;

-- Dashboard guilde : "teams officielles de ma guilde"
CREATE INDEX IF NOT EXISTS idx_challenge_teams_guild
    ON challenge_teams (guild_id, is_persistent, disbanded_at)
    WHERE guild_id IS NOT NULL;
