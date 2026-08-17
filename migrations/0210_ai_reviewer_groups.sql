-- Who is allowed to review AI work.
--
-- Same mechanism as migration 0176, which is the point: the capability is
-- `{primary_domain}_reviewer:{reviewer_group}`, derived rather than invented
-- per domain, so `ai` gets review rights without a second scheme being
-- written for it.
--
-- ## The five families
--
-- Somebody who can judge a dbt model can judge an Airflow DAG, and cannot
-- judge an alignment experiment. Five groups is the shape of the competence,
-- not of the org chart.
--
-- `generative-ai-artist` sits with vision because both are image models and
-- the failure modes rhyme — but the part of that trade that matters most, a
-- direction held across a series, is closer to a design critique. The group
-- lives on the orientation row, so an operator who finds this wrong moves it
-- in the admin panel instead of waiting for a deployment.

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

-- Every value, restated. A CHECK cannot be extended, only replaced, so this
-- list carries everything 0098, 0117, 0120 and 0176 added — dropping one
-- would silently make that capability ungrantable and the guard reading it
-- would start refusing everybody.
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
        -- AI review, by family of trade.
        'ai_reviewer:data',
        'ai_reviewer:ml',
        'ai_reviewer:llm-nlp',
        'ai_reviewer:cv',
        'ai_reviewer:safety',
        'ai_reviewer:all'
    ));

UPDATE orientations SET reviewer_group = g.grp
  FROM (VALUES
    -- Data at rest and the questions asked of it (2)
    ('data-engineer', 'data'),
    ('data-analyst',  'data'),
    -- Models trained and models kept alive (2)
    ('ml-engineer',    'ml'),
    ('mlops-engineer', 'ml'),
    -- Language, whether prompted or parsed (3)
    ('llm-engineer',    'llm-nlp'),
    ('prompt-engineer', 'llm-nlp'),
    ('nlp-engineer',    'llm-nlp'),
    -- Images, discriminative and generative (2)
    ('computer-vision-engineer', 'cv'),
    ('generative-ai-artist',     'cv'),
    -- Breaking things on purpose (1)
    ('ai-safety-researcher', 'safety')
  ) AS g(slug, grp)
 WHERE orientations.slug = g.slug;
