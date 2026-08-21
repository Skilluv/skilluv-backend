-- Who is allowed to review work in a given trade.
--
-- ## Why groups rather than one capability per orientation
--
-- Thirty-three code trades would mean thirty-three capabilities, and an
-- operator granting them one at a time. Nobody reviews at that granularity
-- either: someone who can judge a React component can judge a Svelte one,
-- and cannot judge a CUDA kernel. Eight groups plus a wildcard is the shape
-- of the actual competence.
--
-- ## Why the group lives on the row
--
-- Orientations are created and edited at runtime through the admin panel. A
-- mapping compiled into the binary would mean every new orientation is
-- unreviewable until someone deploys — and the person adding it is the one
-- who knows which group it belongs to.
--
-- ## Why the name is built from the domain
--
-- `{domain}_reviewer:{group}`. The ops backlog wants `ops_reviewer` and
-- design will want its own; deriving the capability from `primary_domain`
-- means each domain gets the same mechanism without a second one being
-- invented for it.

-- Long enough for what this introduces, with room left. `code_reviewer:
-- devtools-media` is twenty-eight characters and the column allowed thirty:
-- the next group name would have failed at insert with a truncation error
-- that names nothing useful.
ALTER TABLE user_capabilities
    ALTER COLUMN capability TYPE VARCHAR(48);

ALTER TABLE user_capabilities
    DROP CONSTRAINT IF EXISTS user_capabilities_capability_check;

-- Every value, restated. A CHECK cannot be extended, only replaced, so this
-- list has to carry everything migrations 0098, 0117 and 0120 added — miss
-- one and the capability silently becomes ungrantable, and the guard that
-- reads it starts refusing everyone.
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
        -- Code review, by family of trade.
        'code_reviewer:web',
        'code_reviewer:mobile',
        'code_reviewer:systems',
        'code_reviewer:blockchain',
        'code_reviewer:compilers',
        'code_reviewer:data',
        'code_reviewer:scientific',
        'code_reviewer:devtools-media',
        'code_reviewer:all'
    ));

-- ═══════════════════════════════════════════════════════════════════
-- The group each trade belongs to
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE orientations
    ADD COLUMN reviewer_group VARCHAR(30);

COMMENT ON COLUMN orientations.reviewer_group IS
    'Family of trades reviewed together. The capability that grants review '
    'rights is {primary_domain}_reviewer:{reviewer_group}, or '
    '{primary_domain}_reviewer:all. NULL means nobody can be granted review '
    'rights for this orientation yet — visible rather than silently open.';

CREATE INDEX idx_orientations_reviewer_group
    ON orientations (primary_domain, reviewer_group)
    WHERE reviewer_group IS NOT NULL;

UPDATE orientations SET reviewer_group = g.grp
  FROM (VALUES
    -- Web (5)
    ('web-frontend-developer', 'web'),
    ('web-backend-developer', 'web'),
    ('web-fullstack-developer', 'web'),
    ('web-performance-engineer', 'web'),
    ('web3-frontend-developer', 'web'),
    -- Mobile (3)
    ('mobile-ios-developer', 'mobile'),
    ('mobile-android-developer', 'mobile'),
    ('mobile-cross-platform-developer', 'mobile'),
    -- Systems and everything that runs close to the metal (5)
    ('systems-programmer', 'systems'),
    ('kernel-driver-developer', 'systems'),
    ('firmware-embedded-developer', 'systems'),
    ('robotics-software-developer', 'systems'),
    ('safety-critical-developer', 'systems'),
    -- Blockchain (2)
    ('smart-contract-developer', 'blockchain'),
    ('blockchain-protocol-developer', 'blockchain'),
    -- Compilers and proofs (2)
    ('compiler-language-developer', 'compilers'),
    ('formal-methods-developer', 'compilers'),
    -- Data at rest and in motion (4)
    ('database-engine-developer', 'data'),
    ('search-engine-developer', 'data'),
    ('distributed-systems-developer', 'data'),
    ('stream-processing-developer', 'data'),
    -- Numbers (3)
    ('scientific-computing-developer', 'scientific'),
    ('gpu-compute-developer', 'scientific'),
    ('hft-quant-developer', 'scientific'),
    -- Tools, applications and the wires between them (9)
    ('cli-tools-developer', 'devtools-media'),
    ('ide-extension-developer', 'devtools-media'),
    ('build-system-developer', 'devtools-media'),
    ('media-processing-developer', 'devtools-media'),
    ('platform-app-developer', 'devtools-media'),
    ('network-protocol-developer', 'devtools-media'),
    ('desktop-app-developer', 'devtools-media'),
    ('enterprise-software-developer', 'devtools-media'),
    ('lowcode-platform-developer', 'devtools-media')
  ) AS g(slug, grp)
 WHERE orientations.slug = g.slug;
