-- Phase P8.3 — Drop des colonnes deprecated de `challenges`.
-- Migration 0070 :
--   - `ai_allowed BOOLEAN` : remplacée depuis P3 (migration 0061) par `ai_policy VARCHAR`
--     typée. Le code Rust a été nettoyé en P8.3 pour ne plus écrire dans cette colonne.
--   - `prerequisite_fragments INTEGER` : remplacée depuis P3 (migration 0066) par le
--     DAG `challenge_prerequisites`. Le check dans /api/challenges/{id}/start
--     utilise désormais 100% DAG via TracksService::check_eligibility.
--
-- Voir docs/challenges-target-model-and-roadmap.md sections 8.4, 10, Q7.

ALTER TABLE challenges
    DROP COLUMN IF EXISTS ai_allowed,
    DROP COLUMN IF EXISTS prerequisite_fragments;
