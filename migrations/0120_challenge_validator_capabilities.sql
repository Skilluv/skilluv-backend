-- SKI-80 (P26 v2 D-01) — Extend user_capabilities with challenge_validator:{domain}.
--
-- The Skilluv challenge workflow (see docs/challenges-target-model-and-roadmap.md
-- part G.1 updated by P26 v2) requires a validation step by community peers
-- BEFORE the challenge is marked successful (validated status introduced in
-- migration 0119). A user must hold the capability challenge_validator:{domain}
-- to be eligible to pick up and approve/reject PRs in that domain.
--
-- Nomenclature: challenge_validator:{domain} where {domain} is one of the
-- 7 skill domains already enforced by project_slices.primary_domain:
--
--   - challenge_validator:code
--   - challenge_validator:design
--   - challenge_validator:game
--   - challenge_validator:security
--   - challenge_validator:ops
--   - challenge_validator:ai
--   - challenge_validator:soft_skills
--
-- A single user may hold multiple validator capabilities (e.g. code + ai).
-- Cumulability follows the P18 base rule.
--
-- Attribution:
--   - Via candidacy (see SKI-81 / D-02): user must meet stats thresholds
--     (rank >= artisan, 10 PRs merged in domain, 3 repos covered, 3 months
--     tenure) then admin approves.
--   - Via admin invitation (see SKI-82 / D-03): admin bypass stats for cases
--     like invited professors; user must explicitly accept.

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

ALTER TABLE user_capabilities
    ADD CONSTRAINT user_capabilities_capability_check
    CHECK (capability IN (
        -- P18 base
        'challenger',
        'mentor',
        'project_steward',
        'pr_reviewer',
        'bounty_funder',
        'issue_proposer',
        'jury_tournament',
        'admin',
        'enterprise_recruiter',
        -- P25: community moderation (user front, NOT admin panel)
        'community_moderator',
        'forum_moderator',
        'plagiarism_reviewer',
        'kyc_reviewer',
        'community_curator',
        -- P26: beginner sas compagnonnage
        'verified_apprentice',
        'apprentice_verifier',
        -- P26 v2 (SKI-80): per-domain challenge validators for the workflow
        -- introduced in migration 0119. See docs/CAPABILITIES.md.
        'challenge_validator:code',
        'challenge_validator:design',
        'challenge_validator:game',
        'challenge_validator:security',
        'challenge_validator:ops',
        'challenge_validator:ai',
        'challenge_validator:soft_skills'
    ));
