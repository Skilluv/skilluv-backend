-- When the two sides of a paid mission cannot agree.
--
-- ## What the round loop already handles
--
-- A client asks for changes, a designer hands in again. Migration 0254 built
-- that, with a written reason as a CHECK constraint and a mark on any round
-- past what was agreed. Most disagreements end there.
--
-- ## What it cannot handle
--
-- A client who will not accept anything, and a designer who will not hand in
-- again. The mission sits `in_progress` for ever, the money sits in escrow,
-- and neither side can move — which is the case where somebody outside has to
-- decide.
--
-- ## Why a table and not a status
--
-- The outcome of an arbitration is an ordinary outcome: the delivery is
-- accepted, or the mission is cancelled. Both already exist. What does not
-- exist is the record that it was *decided* rather than *agreed* — and that
-- distinction is the whole point. A mission accepted by arbitration and one
-- accepted by a happy client look identical in `missions`, and they must not
-- read the same to anybody who later asks what happened.

CREATE TABLE IF NOT EXISTS mission_arbitrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    mission_id UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,

    -- The round that was in dispute, where there was one. Null when the
    -- disagreement is about the mission rather than about a hand-in — a
    -- designer who vanished has left no round to arbitrate.
    delivery_id UUID REFERENCES mission_deliveries(id) ON DELETE SET NULL,

    arbiter_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    -- What was decided. Both outcomes already exist in the mission's own
    -- vocabulary; this row says who decided and why.
    outcome VARCHAR(20) NOT NULL CHECK (outcome IN (
        'accepted',   -- the delivery stands, the money is released
        'cancelled'   -- the mission ends, the escrow goes back
    )),

    -- Written, and long enough to be an argument rather than a verdict. Both
    -- sides read this, and one of them has just lost — "refusé" teaches
    -- nobody anything and cannot be appealed against.
    reason_md TEXT NOT NULL CHECK (length(reason_md) BETWEEN 80 AND 8000),

    decided_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One arbitration per mission. A second would mean re-opening a decision that
-- has already moved money, and re-opening it is a new mission, not a new row.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_arbitration_per_mission
    ON mission_arbitrations (mission_id);

CREATE INDEX IF NOT EXISTS idx_mission_arbitrations_by_arbiter
    ON mission_arbitrations (arbiter_id, decided_at DESC);

COMMENT ON TABLE mission_arbitrations IS
    'A paid mission that neither side would end, decided by somebody outside '
    'it. The outcome is an ordinary outcome; what this table records is that '
    'it was decided rather than agreed, and by whom.';

-- ═══════════════════════════════════════════════════════════════════
-- Who may decide
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

-- Every value, restated. A CHECK cannot be extended, only replaced, so this
-- list carries everything 0098, 0117, 0120, 0176, 0210, 0229 and 0256 added —
-- dropping one would silently make that capability ungrantable and the guard
-- reading it would start refusing everybody.
ALTER TABLE user_capabilities
    ADD CONSTRAINT user_capabilities_capability_check
    CHECK (capability IN (
        -- P18 base
        'challenger', 'mentor', 'project_steward', 'pr_reviewer',
        'bounty_funder', 'issue_proposer', 'jury_tournament', 'admin',
        'enterprise_recruiter',
        -- P25 community moderation
        'community_moderator', 'forum_moderator',
        'plagiarism_reviewer', 'kyc_reviewer', 'community_curator',
        -- P26 beginner sas (migration 0117)
        'verified_apprentice', 'apprentice_verifier',
        -- P26 v2 per-domain challenge validators (migration 0120)
        'challenge_validator:code',
        'challenge_validator:design',
        'challenge_validator:game',
        'challenge_validator:security',
        'challenge_validator:ops',
        'challenge_validator:ai',
        'challenge_validator:soft_skills',
        -- Code review, by family of trade (migration 0176).
        'code_reviewer:web',
        'code_reviewer:mobile',
        'code_reviewer:systems',
        'code_reviewer:blockchain',
        'code_reviewer:compilers',
        'code_reviewer:data',
        'code_reviewer:scientific',
        'code_reviewer:devtools-media',
        'code_reviewer:all',
        -- AI review, by family of trade (migration 0210).
        'ai_reviewer:data',
        'ai_reviewer:ml',
        'ai_reviewer:llm-nlp',
        'ai_reviewer:cv',
        'ai_reviewer:safety',
        'ai_reviewer:all',
        -- Design review, by family of trade (migration 0229).
        'design_reviewer:product',
        'design_reviewer:web',
        'design_reviewer:mobile',
        'design_reviewer:brand',
        'design_reviewer:motion',
        'design_reviewer:illustration',
        'design_reviewer:game',
        'design_reviewer:immersive',
        'design_reviewer:3d-viz',
        'design_reviewer:dataviz',
        'design_reviewer:service',
        'design_reviewer:ux-writing',
        'design_reviewer:marketing',
        'design_reviewer:all',
        -- Running a domain (migration 0256).
        'domain_curator:code',
        'domain_curator:design',
        'domain_curator:game',
        'domain_curator:security',
        'domain_curator:ops',
        'domain_curator:ai',
        'domain_curator:soft_skills',
        'domain_curator:all',
        -- Deciding a paid mission neither side will end.
        --
        -- Flat, not scoped by domain, and deliberately: the question an
        -- arbiter answers is whether a contract was honoured, which is the
        -- same question about a logotype and about a pull request. Scoping it
        -- would leave a stuck mission nobody is allowed to unstick because
        -- its domain has no arbiter yet.
        'mission_arbiter'
    ));
