-- Phase P5 — Attestations ⭐ LAUNCH FEATURE.
-- Migration 0068 : table `attestations` (killer feature de Skilluv).
--
-- Rationale (voir docs/challenges-target-model-and-roadmap.md sections B.12,
-- G.3, 6.3-6.5) :
--
--   Skilluv N'ÉMET PAS des certifications-examen payantes. Elle émet des
--   attestations gratuites, adossées à des artefacts réels vérifiables.
--
--   Trois types :
--     - gesture : "sait faire un geste précis" (auto, sur proficiency_level=2)
--     - skill   : "maîtrise une compétence" (auto sur level=4 + review sénior)
--     - compagnonnage : "a livré un chef-d'œuvre" (manuel, signé par un steward)
--
--   Chaque attestation :
--     - Est signée cryptographiquement de fait par un `verification_code` unique
--       qui pointe vers une URL publique de vérification (`/attestations/verify/{code}`)
--     - Pointe vers les preuves (linked_deliverable_ids, linked_skill_node_ids,
--       linked_project_ids, linked_reviewer_ids)
--     - Peut être révoquée si un deliverable sous-jacent est révoqué (audit trail
--       préservé via revoked_at + revoke_reason)
--
--   Anti-double-issue : UNIQUE index sur (user_id, attestation_type, linked_skill_node_ids)
--   pour les types gesture et skill qui sont "un par skill".

CREATE TABLE attestations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    attestation_type VARCHAR(20) NOT NULL
        CHECK (attestation_type IN ('gesture','skill','compagnonnage')),

    title VARCHAR(200) NOT NULL,
    description TEXT NOT NULL,
    icon VARCHAR(50),

    -- Preuves (arrays de FK — les FK ne sont pas enforcées dans un array,
    -- mais on documente les invariants applicatifs)
    linked_deliverable_ids UUID[] NOT NULL DEFAULT '{}',
    linked_skill_node_ids UUID[] NOT NULL DEFAULT '{}',
    linked_project_ids UUID[] NOT NULL DEFAULT '{}',
    linked_reviewer_ids UUID[] NOT NULL DEFAULT '{}',

    -- Provenance
    issued_by_type VARCHAR(20) NOT NULL DEFAULT 'skilluv'
        CHECK (issued_by_type IN ('skilluv','partner_org','partner_enterprise')),
    issued_by_org_id UUID,  -- FK optionnelle vers enterprises/partners

    -- Vérification publique (URL /attestations/verify/{code})
    -- 10 chars base32 = 50 bits d'entropie (~10^15 combinaisons)
    verification_code VARCHAR(12) NOT NULL UNIQUE,
    public BOOLEAN NOT NULL DEFAULT TRUE,

    -- Révocation (audit trail préservé)
    revoked_at TIMESTAMPTZ,
    revoked_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    revoke_reason TEXT,

    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- NULL = éternelle (défaut pour skill/compagnonnage) ; NON NULL pour partenaires
    expires_at TIMESTAMPTZ,

    -- Un compagnonnage doit avoir au moins un project référencé.
    -- `cardinality()` retourne 0 pour un array vide (contrairement à `array_length`
    -- qui retourne NULL et laisserait passer la contrainte via SQL nullability).
    CONSTRAINT attestations_compagnonnage_has_project
        CHECK (
            attestation_type != 'compagnonnage'
            OR cardinality(linked_project_ids) >= 1
        ),

    -- Un gesture/skill doit avoir exactement un skill référencé.
    CONSTRAINT attestations_skill_has_one_skill
        CHECK (
            attestation_type NOT IN ('gesture','skill')
            OR cardinality(linked_skill_node_ids) = 1
        )
);

-- Portfolio public d'un user (endpoint principal)
CREATE INDEX idx_attestations_user_public
    ON attestations (user_id, issued_at DESC)
    WHERE public = TRUE AND revoked_at IS NULL;

-- Vérification par code (endpoint public /attestations/verify/{code})
CREATE INDEX idx_attestations_verification_code
    ON attestations (verification_code);

-- Filtrage par type (skill map avec badges)
CREATE INDEX idx_attestations_user_type
    ON attestations (user_id, attestation_type)
    WHERE revoked_at IS NULL;

-- Anti-double-issue pour gesture et skill (un par skill max)
-- Note : compagnonnage peut être multiple (un par projet gradué)
CREATE UNIQUE INDEX uniq_attestations_gesture_skill_per_skill
    ON attestations (user_id, attestation_type, linked_skill_node_ids)
    WHERE attestation_type IN ('gesture', 'skill') AND revoked_at IS NULL;

-- Anti-double compagnonnage sur un même projet (une seule attestation par
-- projet, sinon on multiplie sur les mêmes preuves)
CREATE UNIQUE INDEX uniq_attestations_compagnonnage_per_project
    ON attestations (user_id, attestation_type, linked_project_ids)
    WHERE attestation_type = 'compagnonnage' AND revoked_at IS NULL;
