-- Phase P0 — Fondations du modèle cible.
-- Migration 0060 : tables `deliverables` (artefact opposable) + `user_skills`
-- (skills prouvés par user). Ces deux tables sont introduites ensemble parce
-- qu'elles sont couplées par le workflow de vérification (docs/... partie G.1/G.2).
--
-- Rationale :
--   1. `deliverables` remplace sémantiquement `challenge_submissions.code` :
--      un artefact réel avec URL vérifiable, hash immutable, politique IA déclarée.
--      Immuable une fois vérifié sauf `revoked_at`, `featured`, `public`
--      (décision Q3 session 2026-07-09). Les corrections légitimes passent par
--      un nouveau deliverable qui supersede via `parent_deliverable_id`.
--   2. `user_skills` remplace sémantiquement `skill_fragments` :
--      chaque skill prouvé par un user avec proficiency_level (1-5) calculé
--      via la formule log2 de G.2. Cumule via `weighted_proven_count`.
--
-- Workflow (résumé, détail dans le doc) :
--   PR mergée → deliverable inséré (verification_status='verified')
--             → pour chaque slice_skill : update user_skills WPC + proficiency
--             → si proficiency franchit un seuil, éligibilité attestation (Phase P5)
--
-- Contraintes clés (décidées en G.6) :
--   - UNIQUE(user_id, artifact_hash) : idempotence webhook GitHub
--   - CHECK "at least one parent" : chaque deliverable est rattaché à
--     un slice OU un challenge (impossible d'être orphelin)

-- ═══════════════════════════════════════════════════════════════════
-- Table : deliverables
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE deliverables (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Rattachement : au moins un des deux non-null (contrainte plus bas)
    slice_id UUID REFERENCES project_slices(id) ON DELETE CASCADE,
    challenge_id UUID REFERENCES challenges(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    team_id UUID REFERENCES challenge_teams(id) ON DELETE SET NULL,

    -- Chain de supersede (décision Q3 : corrections via nouveau deliverable)
    parent_deliverable_id UUID REFERENCES deliverables(id) ON DELETE SET NULL,

    -- L'artefact lui-même
    artifact_type VARCHAR(40) NOT NULL
        CHECK (artifact_type IN (
            'pr_merged',              -- PR mergée dans un repo tiers
            'pr_open',                -- PR ouverte, pas encore mergée
            'commit',                 -- commit direct (rare, cas maintainer)
            'figma_frame',            -- frame Figma livrée
            'design_tokens_export',   -- export JSON W3C CG tokens
            'playable_build',         -- build jouable (HTML5, itch.io)
            'game_asset',             -- asset livré (.blend, .png, .fbx)
            'game_scene',             -- scène Godot livrée
            'cve_report',             -- rapport de vuln avec CVE assigné
            'pentest_writeup',        -- writeup pentest (Markdown standard)
            'disclosure',             -- disclosure responsable non-CVE
            'code_review',            -- review humaine formalisée (artefact)
            'documentation',          -- doc mergée / publiée
            'test_suite',             -- suite de tests ajoutée
            'blender_asset',          -- asset Blender exportable
            'other'
        )),
    artifact_url TEXT NOT NULL,
    artifact_hash TEXT,       -- SHA commit, Figma version ID, itch.io build ID
    artifact_metadata JSONB,  -- taille, langue, tools, stats diff, etc.

    -- Vérification (workflow G.1)
    verifiable_by VARCHAR(30) NOT NULL
        CHECK (verifiable_by IN (
            'github_webhook',     -- auto via webhook (le référent)
            'human_review',       -- via file d'attente review humaine (Phase P2)
            'automated_diff',     -- diff automatique (pipe rétrocompat challenges/submit)
            'third_party_api',    -- appel API externe (itch.io, HackerOne, CVE.org)
            'ci_status',          -- statut CI vert (utilisé conjointement)
            'multi'               -- combinaison des ci-dessus
        )),
    verification_status VARCHAR(30) NOT NULL DEFAULT 'pending'
        CHECK (verification_status IN (
            'pending',                    -- attend review
            'pending_manual_review',      -- auto-verify échoué, attend steward (G.1 étape 3)
            'pending_admin_escalation',   -- SLA 72h dépassé (W4)
            'verified',                   -- vérifié
            'rejected',                   -- rejeté par review
            'revoked'                     -- révoqué a posteriori (plagiarism, revert, etc.)
        )),
    verified_at TIMESTAMPTZ,
    verified_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    verification_signal JSONB,   -- payload webhook GitHub, ID review, etc.
    verification_notes TEXT,

    -- Récompenses figées au moment de la vérification (immuables)
    fragments_awarded INTEGER NOT NULL DEFAULT 0 CHECK (fragments_awarded >= 0),
    credits_awarded NUMERIC(10,2) NOT NULL DEFAULT 0 CHECK (credits_awarded >= 0),

    -- Politique IA (aligné section 10 vision doc)
    ai_assistance_level VARCHAR(30)
        CHECK (ai_assistance_level IS NULL OR ai_assistance_level IN (
            'none',
            'autocomplete',
            'pair_programming',
            'generated_then_refactored',
            'generated_as_is'
        )),
    ai_tools_used TEXT[] NOT NULL DEFAULT '{}',
    ai_disclosure_notes TEXT,
    -- Fenêtre de 7j après verified pour déclarer (workflow G.1 étape 12)
    ai_disclosure_prompted_at TIMESTAMPTZ,
    ai_disclosure_deadline_at TIMESTAMPTZ,

    -- Visibilité (mutables même après vérification)
    public BOOLEAN NOT NULL DEFAULT TRUE,
    featured BOOLEAN NOT NULL DEFAULT FALSE,

    -- Révocation (audit trail préservé)
    revoked_at TIMESTAMPTZ,
    revoked_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    revocation_reason TEXT,

    -- Timestamps
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Un deliverable doit être rattaché à quelque chose
    CONSTRAINT deliverables_at_least_one_parent
        CHECK (slice_id IS NOT NULL OR challenge_id IS NOT NULL),

    -- Empêche le cas "vérifié sans preuve de vérification"
    CONSTRAINT deliverables_verified_has_signal
        CHECK (
            verification_status != 'verified'
            OR (artifact_url IS NOT NULL AND verifiable_by IS NOT NULL)
        )
);

-- Idempotence webhook GitHub : même merge SHA ne crée qu'un seul deliverable
-- pour un user donné. Le hash peut être NULL pour les artefacts sans notion de SHA
-- (design, game asset) donc l'unique s'applique seulement quand non-NULL.
CREATE UNIQUE INDEX uniq_deliverables_user_artifact_hash
    ON deliverables (user_id, artifact_hash)
    WHERE artifact_hash IS NOT NULL;

-- Profil public : ce que le user peut montrer aux recruteurs
CREATE INDEX idx_deliverables_user_public
    ON deliverables (user_id, submitted_at DESC)
    WHERE public = TRUE AND revoked_at IS NULL AND verification_status = 'verified';

-- Retrouver les deliverables d'une slice (typiquement 1, mais chain via parent possible)
CREATE INDEX idx_deliverables_slice
    ON deliverables (slice_id)
    WHERE slice_id IS NOT NULL;

-- Retrouver les deliverables d'un challenge (training/capstone)
CREATE INDEX idx_deliverables_challenge
    ON deliverables (challenge_id)
    WHERE challenge_id IS NOT NULL;

-- File d'attente de vérification humaine (Phase P2)
CREATE INDEX idx_deliverables_pending
    ON deliverables (verification_status, submitted_at)
    WHERE verification_status IN ('pending', 'pending_manual_review', 'pending_admin_escalation');

-- Featured (mis en avant sur la landing / dashboard impact)
CREATE INDEX idx_deliverables_featured
    ON deliverables (submitted_at DESC)
    WHERE featured = TRUE AND public = TRUE AND revoked_at IS NULL;

-- Chain supersede (retrouver les descendants d'un deliverable révoqué)
CREATE INDEX idx_deliverables_parent
    ON deliverables (parent_deliverable_id)
    WHERE parent_deliverable_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Table : user_skills
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE user_skills (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    skill_id UUID NOT NULL REFERENCES skill_nodes(id) ON DELETE CASCADE,

    -- Compteurs alimentant la formule proficiency (G.2)
    proven_count INTEGER NOT NULL DEFAULT 0 CHECK (proven_count >= 0),
    -- Weighted proven count = SUM(slice_skills.weight) sur toutes les slices
    -- vérifiées ayant touché ce skill. Alimente la formule
    -- proficiency_level = min(5, ceil(log2(WPC + 1)))
    weighted_proven_count INTEGER NOT NULL DEFAULT 0 CHECK (weighted_proven_count >= 0),
    proficiency_level SMALLINT NOT NULL DEFAULT 1
        CHECK (proficiency_level BETWEEN 1 AND 5),

    -- Top 5 preuves (deliverable IDs triés par fragments_awarded DESC)
    -- Affichées sur le profil public sous le skill
    top_proof_deliverable_ids UUID[] NOT NULL DEFAULT '{}',

    first_proven_at TIMESTAMPTZ,
    last_proven_at TIMESTAMPTZ,

    PRIMARY KEY (user_id, skill_id)
);

-- Vue "mes skills" sur le profil, triée par niveau puis récence
CREATE INDEX idx_user_skills_user
    ON user_skills (user_id, proficiency_level DESC, last_proven_at DESC);

-- Recherche recruteur : "qui sait X à niveau ≥ 3 ?"
CREATE INDEX idx_user_skills_skill_level
    ON user_skills (skill_id, proficiency_level DESC);

-- Note : la formule proficiency n'est PAS enforced par CHECK en base
-- (elle change potentiellement dans le temps). La cohérence est maintenue
-- par le service applicatif à chaque update de weighted_proven_count.
