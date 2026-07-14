-- Phase P15.2 — Support `llm_evaluation` comme mode de vérification.
-- Migration 0087.
--
-- Rationale :
--   Les challenges `ai_policy='ai_native'` (P8.1) autorisent l'IA côté user
--   mais n'ont pas de vérificateur dédié. Cette migration :
--   - Ajoute `'llm_evaluation'` au CHECK verifiable_by de deliverables.
--   - Ajoute `challenge_templates.evaluation_rubric JSONB` pour permettre
--     à un admin de spécifier des critères d'évaluation LLM déclaratifs :
--     ex `{ "criteria": ["clarity", "correctness"], "min_score": 0.7 }`.
--
--   L'implémentation Rust côté backend délègue au service Python
--   `skilluv-ia` via l'appel gRPC `AiClient::review_code` (existant depuis
--   Phase 4). Aucun nouveau modèle IA n'est développé ici — on branche.

ALTER TABLE deliverables
    DROP CONSTRAINT IF EXISTS deliverables_verifiable_by_check;

ALTER TABLE deliverables
    ADD CONSTRAINT deliverables_verifiable_by_check
    CHECK (verifiable_by IN (
        'github_webhook',
        'human_review',
        'automated_diff',
        'third_party_api',
        'ci_status',
        'multi',
        'llm_evaluation'      -- P15.2 : LLM juge selon evaluation_rubric du template
    ));

ALTER TABLE challenge_templates
    ADD COLUMN IF NOT EXISTS evaluation_rubric JSONB;

-- Index GIN pour recherche : "quels challenges ai_native ont une rubrique ?"
CREATE INDEX IF NOT EXISTS idx_challenge_templates_evaluation_rubric
    ON challenge_templates USING gin (evaluation_rubric)
    WHERE evaluation_rubric IS NOT NULL;
