-- Somebody who runs a domain without running the platform.
--
-- ## The gap
--
-- Curating a domain — publishing its challenges, opening its contests,
-- scheduling its featurings — required `admin`, which also grants the ability
-- to ban people, move money and read the financial dashboard. There was no way
-- to hand somebody the design calendar without handing them everything.
--
-- ## Why scoped, and not `design_curator`
--
-- The ticket asked for `design_curator` and `design_moderator`. A flat
-- `design_curator` would be the eighth capability spelled one way while its
-- seven siblings are spelled another: `challenge_validator:design`,
-- `design_reviewer:brand`, `code_reviewer:web`. The scoped form is the
-- platform's convention, and it means the same guard reads a design curator
-- and a security one.
--
-- No `domain_moderator`. Moderation is already granular and already
-- domain-blind: `plagiarism_reviewer` reads flagged deliverables of every
-- kind, `forum_moderator` reads posts, `community_moderator` is the umbrella.
-- A design moderator would be a seventh way of saying one of those, and the
-- decision it makes — this work is plagiarised — is the same decision
-- whichever domain the work came from. Splitting it per domain would mean a
-- flagged deliverable nobody is allowed to look at because its domain has no
-- moderator yet.

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

-- Every value, restated. A CHECK cannot be extended, only replaced, so this
-- list carries everything 0098, 0117, 0120, 0176, 0210 and 0229 added —
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
        -- Running a domain: its challenges, its contests, its featurings.
        -- Not its people, not its money.
        'domain_curator:code',
        'domain_curator:design',
        'domain_curator:game',
        'domain_curator:security',
        'domain_curator:ops',
        'domain_curator:ai',
        'domain_curator:soft_skills',
        'domain_curator:all'
    ));

COMMENT ON COLUMN user_capabilities.capability IS
    'What somebody is allowed to do. Cumulative, never exclusive: a person '
    'holds every capability they have earned or been given, and no capability '
    'implies another. Scoped values carry their scope after the colon — the '
    'same slug as `orientations.reviewer_group` for reviewers, and '
    '`users.skill_domain` for curators.';
