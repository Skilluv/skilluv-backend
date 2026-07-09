-- Phase P0 — Fondations du modèle cible.
-- Migration 0061 : extensions non-breaking de la table `challenges` pour
-- s'aligner avec le modèle cible (voir docs/challenges-target-model-and-roadmap.md
-- section B.14).
--
-- Rationale :
--   1. `is_training` — Distingue les challenges d'onboarding/training (hors règle
--      dure #1 qui exige un project_id) des challenges rattachés à un projet réel.
--      Les seeds actuels (Hello World, premier design, etc.) auront is_training=TRUE.
--   2. `project_id` — Lien optionnel vers un vrai projet Skilluv. Applique la
--      règle dure #1 : "aucun challenge publié sans project_id sauf training".
--   3. `ai_policy` — Remplace le pauvre `ai_allowed BOOLEAN` (métadonnée décorative
--      jamais vérifiée, voir section 10.1 du doc vision) par une politique typée
--      qui alimente le workflow d'évaluation (voir section 10.6 du doc vision).
--
-- Non-breaking : ajoute des colonnes optionnelles avec defaults sensés, migre
-- `ai_allowed → ai_policy` de manière conservative.
-- `ai_allowed` reste en place pour compatibilité, sera drop en Phase P3.

ALTER TABLE challenges
    ADD COLUMN is_training BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    ADD COLUMN ai_policy VARCHAR(30) NOT NULL DEFAULT 'disclosure_required'
        CHECK (ai_policy IN (
            'unrestricted',         -- contribution réelle, review humaine décide
            'disclosure_required',  -- IA autorisée mais à déclarer
            'human_verified',       -- IA autorisée mais compréhension vérifiée (live/quiz)
            'no_ai_declared',       -- honor system, sanction morale si violation évidente
            'ai_native'             -- AI-first challenge (prompting, refactoring, etc.)
        ));

-- Marquer les challenges seeded d'onboarding comme training (règle dure #1)
UPDATE challenges
SET is_training = TRUE
WHERE is_onboarding = TRUE;

-- Migration progressive de `ai_allowed` vers `ai_policy` :
UPDATE challenges SET ai_policy = 'unrestricted'    WHERE ai_allowed = TRUE;
UPDATE challenges SET ai_policy = 'no_ai_declared'  WHERE ai_allowed = FALSE;

-- Règle dure #1 : "aucun challenge status='published' sans project_id sauf training".
-- Cette contrainte protège l'invariant produit central (pas de kata sans lien à un
-- vrai projet). Les challenges existants publiés en dev sont tous is_onboarding=TRUE
-- (donc désormais is_training=TRUE), l'invariant est respecté.
ALTER TABLE challenges
    ADD CONSTRAINT challenges_project_or_training
    CHECK (
        status != 'published'
        OR is_training = TRUE
        OR project_id IS NOT NULL
    );

CREATE INDEX idx_challenges_project
    ON challenges (project_id)
    WHERE project_id IS NOT NULL;

CREATE INDEX idx_challenges_training
    ON challenges (is_training)
    WHERE is_training = TRUE;
