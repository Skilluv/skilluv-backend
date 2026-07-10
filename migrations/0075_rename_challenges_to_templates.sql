-- Phase P9.3 — Renommage sémantique `challenges` → `challenge_templates`.
-- Migration 0075.
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md partie C phase 8) :
--   La table `challenges` (héritée du modèle P0) porte en réalité des templates :
--   depuis P2 les instances sont matérialisées par `challenge_submissions` et
--   `deliverables`. Le nom `challenge_templates` reflète la sémantique cible.
--
-- Non-breaking côté FK :
--   Les colonnes `challenge_id` sur les tables enfants (challenge_submissions,
--   challenge_prerequisites, sponsored_challenges, etc.) sont renommées via
--   la RENAME côté parent uniquement — la contrainte FK pointe automatiquement
--   sur la nouvelle table. Aucune migration cascade nécessaire.
--
-- Non-breaking côté API HTTP :
--   Les paths `/api/challenges/*` sont conservés. Le nom de table est un
--   détail d'implémentation ; les clients ne le voient pas.
--
-- Le struct Rust `Challenge` (models/challenge.rs) est conservé — il représente
-- une ligne de challenge_templates et le nom fonctionne toujours dans le
-- contexte code (un « challenge » = un template de challenge).

ALTER TABLE challenges RENAME TO challenge_templates;

-- Renommer aussi les index/constraints qui portaient le nom "challenges_..."
-- (postgres ne renomme pas automatiquement les objets liés).
ALTER INDEX IF EXISTS challenges_pkey RENAME TO challenge_templates_pkey;
ALTER TABLE challenge_templates
    RENAME CONSTRAINT challenges_project_or_training TO challenge_templates_project_or_training;
