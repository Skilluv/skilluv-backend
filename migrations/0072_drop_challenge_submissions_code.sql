-- Phase P9.1 — Drop des colonnes legacy `challenge_submissions.code|stdout|stderr`.
--
-- Rationale :
--   Depuis P8.5a, chaque submission réussie crée un `deliverable` avec
--   `artifact_hash` SHA-256 du code + `artifact_metadata` JSONB pointant sur
--   la submission. Les colonnes `code|stdout|stderr` sur `challenge_submissions`
--   dupliquent une info désormais portée par le modèle cible.
--
--   Pour préserver l'immuabilité des preuves (règle produit A.4), on backfille
--   les deliverables existants avec le contenu du code + stdout + stderr des
--   submissions liées avant de dropper les colonnes.
--
-- Étape 1 : backfill deliverables.artifact_metadata
UPDATE deliverables d
SET artifact_metadata = COALESCE(d.artifact_metadata, '{}'::jsonb)
    || jsonb_strip_nulls(jsonb_build_object(
        'code_content', cs.code,
        'stdout', cs.stdout,
        'stderr', cs.stderr,
        'language', cs.language
    ))
FROM challenge_submissions cs
WHERE d.artifact_metadata->>'submission_id' = cs.id::text
  AND (cs.code IS NOT NULL OR cs.stdout IS NOT NULL OR cs.stderr IS NOT NULL);

-- Étape 2 : DROP des colonnes legacy.
-- On conserve `challenge_submissions.language` (utile pour analytics)
-- et `.fragments_earned`/`.status`/`.attempt_number` (historique de progression).
ALTER TABLE challenge_submissions
    DROP COLUMN IF EXISTS code,
    DROP COLUMN IF EXISTS stdout,
    DROP COLUMN IF EXISTS stderr;
