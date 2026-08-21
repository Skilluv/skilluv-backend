-- Who is allowed to review design work.
--
-- Same mechanism as migrations 0176 and 0210: the capability is
-- `{primary_domain}_reviewer:{reviewer_group}`, derived rather than invented
-- per domain. Design gets review rights without a second scheme.
--
-- ## Why design needs the family scope more than code does
--
-- A code reviewer has the diff and the CI signal; even outside their comfort
-- zone they can see that the tests pass and the naming is wrong. A design
-- review has neither. The verdict rests entirely on the reviewer's own craft,
-- so a motion designer signing off a typeface is not a stretched judgement,
-- it is no judgement at all. Thirteen groups is where the competence actually
-- stops.
--
-- ## The thirteen
--
-- Someone who can judge a mobile flow can judge a web one and cannot judge a
-- render pipeline. `design-video` sits with motion because the craft is
-- timing either way. `design-sound` sits with XR because both are judged on
-- what they do to a body in a space, and neither has a still image to look
-- at. `design-ops` sits with service design because both are judged on
-- whether a process holds, not on what it looks like.
--
-- A reviewer who disagrees with a placement moves the row in the admin panel;
-- the group lives on the orientation for exactly that reason.

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

-- Every value, restated. A CHECK cannot be extended, only replaced, so this
-- list carries everything 0098, 0117, 0120, 0176 and 0210 added — dropping
-- one would silently make that capability ungrantable and the guard reading
-- it would start refusing everybody.
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
        -- Design review, by family of trade.
        'design_reviewer:product',
        'design_reviewer:web',
        'design_reviewer:mobile',
        'design_reviewer:motion',
        'design_reviewer:brand',
        'design_reviewer:illustration',
        'design_reviewer:dataviz',
        'design_reviewer:ux-writing',
        'design_reviewer:marketing',
        'design_reviewer:game',
        'design_reviewer:3d-viz',
        'design_reviewer:immersive',
        'design_reviewer:service',
        'design_reviewer:all'
    ));

UPDATE orientations SET reviewer_group = g.grp
  FROM (VALUES
    -- Screens, and the systems behind them (3)
    ('design-product',           'product'),
    ('design-system',            'product'),
    ('design-ai-conversational', 'product'),
    -- Pages that are read (2)
    ('design-web',           'web'),
    ('design-editorial-web', 'web'),
    -- The platform in a hand (1)
    ('design-mobile', 'mobile'),
    -- Anything judged on timing (4)
    ('design-motion-ui', 'motion'),
    ('design-motion-2d', 'motion'),
    ('design-motion-3d', 'motion'),
    ('design-video',     'motion'),
    -- What a company is recognised by, seen and heard (3)
    ('design-brand-identity', 'brand'),
    ('design-typography',     'brand'),
    ('design-naming-verbal',  'brand'),
    -- Drawn by hand, at three scales (3)
    ('design-illustration', 'illustration'),
    ('design-iconography',  'illustration'),
    ('design-character',    'illustration'),
    -- Numbers made legible (1)
    ('design-dataviz', 'dataviz'),
    -- Words in an interface (1)
    ('design-ux-writing', 'ux-writing'),
    -- One message across many surfaces (1)
    ('design-marketing', 'marketing'),
    -- Under an engine constraint (2)
    ('design-game-ui',          'game'),
    ('design-game-environment', 'game'),
    -- Buildings that do not exist yet (1)
    ('design-arch-interior-viz', '3d-viz'),
    -- Judged on what it does to a body in a space (2)
    ('design-ar-vr-spatial', 'immersive'),
    ('design-sound',         'immersive'),
    -- Judged on whether a process holds (2)
    ('design-service', 'service'),
    ('design-ops',     'service')
  ) AS g(slug, grp)
 WHERE orientations.slug = g.slug;

-- A trade with no group cannot be reviewed by anybody, which is a state the
-- catalogue should never reach silently.
DO $$
DECLARE
    ungrouped TEXT;
BEGIN
    SELECT string_agg(slug, ', ') INTO ungrouped
      FROM orientations
     WHERE primary_domain = 'design'
       AND is_archived = FALSE
       AND reviewer_group IS NULL;

    IF ungrouped IS NOT NULL THEN
        RAISE EXCEPTION
            'design trades with no reviewer group, so nobody can be granted '
            'review rights for them: %', ungrouped;
    END IF;
END $$;
