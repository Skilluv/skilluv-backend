-- Phase P26.2 — Sas compagnonnage débutant : marquage challenges + questions
-- de review + trace des vérifications asynchrones.
-- Migration 0118.
--
-- Rationale :
--   Trois pièces DDL portent le workflow discuté :
--
--   (A) challenge_templates.beginner_stage
--       Marque quels challenges appartiennent au sas (review humaine
--       obligatoire) vs mode libre (post-sas, sans review) vs hors-sas
--       (challenges non-beginner : intermediate, advanced, tournaments…
--       aucun contrôle sas ne s'applique). Enum textuel plutôt que PG type
--       pour pouvoir ajouter des stades futurs sans migration structurelle
--       (ex : `mentor_only`, `sponsored_intro`).
--
--   (B) challenge_verification_questions
--       Pool de questions préenregistrées PAR challenge, alimenté par
--       l'auteur du challenge. Au moment du submit, le serveur tire N
--       questions aléatoires (default 2) dans le pool actif. L'apprenti
--       enregistre une vidéo/audio par question. Uniformise la review et
--       empêche l'anticipation par un tricheur.
--
--   (C) apprentice_verifications
--       Log d'événement : une ligne par soumission au sas. Contient les
--       réponses (mapping question_id → media_url en JSONB), le verdict
--       du compagnon reviewer, et les notes. Sert de source à la promotion
--       auto vers `verified_apprentice` (P26.6 : hook sur count(approved
--       distincts par challenge)).
--
--   Le workflow ne recrée AUCUN concept déjà porté par les tables existantes :
--     - identité apprenti : users.id
--     - identité compagnon : users.id + user_capabilities.capability='apprentice_verifier'
--     - stockage media : bucket privé MinIO existant (services/storage.rs)
--     - engine de grant : capability_engine (P18.2)
--     - hook d'orchestration : proof_hooks (P19.1)

-- ═══════════════════════════════════════════════════════════════════
-- (A) beginner_stage sur challenge_templates
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE challenge_templates
    ADD COLUMN beginner_stage VARCHAR(16)
        CHECK (beginner_stage IS NULL OR beginner_stage IN ('sas', 'free'));

COMMENT ON COLUMN challenge_templates.beginner_stage IS
    'P26 : NULL = challenge non-beginner (aucun contrôle sas). '
    'sas = review humaine obligatoire (compagnon verdict). '
    'free = mode libre, réservé aux verified_apprentice.';

-- Query "quels challenges du sas sont disponibles ?" et
-- "quels challenges libres sont soumissibles par un verified_apprentice ?"
CREATE INDEX idx_challenge_templates_beginner_stage
    ON challenge_templates (beginner_stage)
    WHERE beginner_stage IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- (B) Pool de questions par challenge
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE challenge_verification_questions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES challenge_templates(id) ON DELETE CASCADE,
    -- Question posée à l'apprenti. Sera lue à l'écran ou vocalisée par le
    -- client au moment de l'enregistrement. Texte i18n libre — pas de clé
    -- rust-i18n ici parce que ces prompts sont éditoriaux, gérés par les
    -- auteurs de challenge, pas par le pipeline de traduction plateforme.
    prompt_text TEXT NOT NULL CHECK (length(prompt_text) BETWEEN 10 AND 500),
    -- Sert au tri par défaut côté auteur / admin ; sans effet sur le
    -- tirage aléatoire côté submit (uniform sample).
    order_hint INTEGER NOT NULL DEFAULT 0,
    -- Désactive sans supprimer, préserve les historiques dans
    -- apprentice_verifications.answers -> question_id.
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Query "pool actif du challenge X pour tirage aléatoire au submit"
CREATE INDEX idx_challenge_verification_questions_active
    ON challenge_verification_questions (template_id)
    WHERE active = TRUE;

-- Éviter les doublons de prompt exact sur un même template (typo, copie).
CREATE UNIQUE INDEX uniq_challenge_verification_questions_prompt
    ON challenge_verification_questions (template_id, prompt_text);

-- ═══════════════════════════════════════════════════════════════════
-- (C) Log des vérifications sas
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE apprentice_verifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- L'apprenti dont on vérifie la contribution.
    apprentice_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Le challenge concerné. Le beginner_stage doit être 'sas' au moment
    -- du submit (contrainte vérifiée applicativement, pas en trigger, pour
    -- laisser la flexibilité d'un rollback stage).
    template_id UUID NOT NULL REFERENCES challenge_templates(id) ON DELETE CASCADE,
    -- La soumission de code / deliverable associée. NULL possible si l'user
    -- a supprimé sa soumission entretemps (ON DELETE SET NULL préserve le
    -- log de la review).
    submission_id UUID REFERENCES challenge_submissions(id) ON DELETE SET NULL,
    -- Compagnon qui a rendu le verdict. NULL tant que pending.
    reviewer_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    -- Mapping question_id (UUID) → media_url (String) déposé par l'apprenti,
    -- une entrée par question tirée au submit. JSONB pour éviter une table
    -- de jointure lourde ; les questions sont read-only depuis le submit.
    -- Exemple :
    --   {"a1b2...": "s3://private/verifs/2026/08/abc.webm",
    --    "c3d4...": "s3://private/verifs/2026/08/def.webm"}
    answers JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(answers) = 'object'),
    -- Verdict compagnon. Reste 'pending' tant que non reviewé. 'abstain'
    -- permet à un compagnon de passer la main sans rejeter (ex : cas
    -- ambigu, doit être vu par un plus expérimenté).
    verdict VARCHAR(16) NOT NULL DEFAULT 'pending'
        CHECK (verdict IN ('pending', 'approved', 'rejected', 'abstain')),
    -- Note libre du compagnon, visible par l'apprenti si rejected/abstain.
    reviewer_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Renseigné au moment du verdict (approved/rejected/abstain).
    reviewed_at TIMESTAMPTZ,
    CONSTRAINT reviewer_and_verdict_coherent
        CHECK (
            (verdict = 'pending' AND reviewer_user_id IS NULL AND reviewed_at IS NULL)
            OR
            (verdict <> 'pending' AND reviewer_user_id IS NOT NULL AND reviewed_at IS NOT NULL)
        )
);

-- Un apprenti ne peut avoir qu'UNE vérif pending par challenge à la fois
-- (re-submit après rejet permis, on garde l'historique).
CREATE UNIQUE INDEX uniq_apprentice_verifications_pending
    ON apprentice_verifications (apprentice_user_id, template_id)
    WHERE verdict = 'pending';

-- File compagnon "qu'est-ce qui attend d'être reviewé, en FIFO ?"
CREATE INDEX idx_apprentice_verifications_pending_queue
    ON apprentice_verifications (created_at ASC)
    WHERE verdict = 'pending';

-- Comptage rapide "combien d'approbations distinctes cet apprenti a-t-il ?"
-- pour le hook d'auto-grant P26.6.
CREATE INDEX idx_apprentice_verifications_approved_by_user
    ON apprentice_verifications (apprentice_user_id, template_id)
    WHERE verdict = 'approved';

-- Historique "qui a été reviewé par quel compagnon" pour l'audit compagnon.
CREATE INDEX idx_apprentice_verifications_by_reviewer
    ON apprentice_verifications (reviewer_user_id, reviewed_at DESC)
    WHERE reviewer_user_id IS NOT NULL;
