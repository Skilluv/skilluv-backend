-- Craft scores, for every domain rather than one.
--
-- ## What was wrong
--
-- Migration 0195 put `craft_score_code` on `users`, while the formula and the
-- tiers it reads (`craft_score_weights`, `craft_score_tiers`) were already
-- keyed by `skill_domain`. So the computation generalised and the storage did
-- not, and the next domain would have meant a twelfth column, a twelfth
-- index, and a search endpoint with a twelve-branch match on which column to
-- read.
--
-- Eleven domains are documented. A row per person per domain, with the tier
-- resolved at write time so a listing can filter on it without joining to the
-- tier table.
--
-- ## Why the tier is stored
--
-- It is derived from the score and the thresholds, so storing it duplicates
-- something. It is stored anyway: the recruiter search filters on "Senior and
-- above", and computing that per row at query time means a join and a range
-- condition on every search. The duplicate is refreshed by the same write
-- that sets the score, in one place.

CREATE TABLE craft_scores (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    skill_domain VARCHAR(30) NOT NULL,
    score INTEGER NOT NULL DEFAULT 0 CHECK (score >= 0 AND score <= 10000),
    -- Resolved from `craft_score_tiers` when the score is written. NULL only
    -- for a domain with no tiers defined, which is an honest answer.
    tier_slug VARCHAR(40),
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, skill_domain)
);

COMMENT ON TABLE craft_scores IS
    'One row per person per domain. The formula was already keyed by domain; '
    'this is the storage catching up, so the twelfth domain is a row rather '
    'than a column and a twelve-branch match in the search.';

COMMENT ON COLUMN craft_scores.tier_slug IS
    'Duplicates what the score and the thresholds imply. Stored so the '
    'recruiter search can filter on "Senior and above" without a join and a '
    'range condition on every row.';

-- What the search reads: everybody in a domain, best first.
CREATE INDEX idx_craft_scores_domain_rank
    ON craft_scores (skill_domain, score DESC)
    WHERE score > 0;

CREATE INDEX idx_craft_scores_tier
    ON craft_scores (skill_domain, tier_slug)
    WHERE tier_slug IS NOT NULL;

-- What the sweep reads: the stalest first.
CREATE INDEX idx_craft_scores_stale ON craft_scores (computed_at);

-- Carry across what 0195 already computed. Only the rows that were actually
-- computed: a zero that was never calculated is not the same as a zero that
-- was, and the sweep will fill the rest.
INSERT INTO craft_scores (user_id, skill_domain, score, computed_at)
SELECT id, 'code', craft_score_code, craft_score_code_computed_at
  FROM users
 WHERE craft_score_code_computed_at IS NOT NULL
ON CONFLICT (user_id, skill_domain) DO NOTHING;

-- Resolve the tier for what was just carried across.
UPDATE craft_scores cs
   SET tier_slug = t.slug
  FROM craft_score_tiers t
 WHERE t.skill_domain = cs.skill_domain
   AND cs.score >= t.min_score
   AND (t.max_score IS NULL OR cs.score <= t.max_score);

-- The old columns go. Keeping them would mean two places to write and one of
-- them eventually going stale, which is worse than either alone.
DROP INDEX IF EXISTS idx_users_craft_score_code;

ALTER TABLE users
    DROP COLUMN craft_score_code,
    DROP COLUMN craft_score_code_computed_at;

-- ═══════════════════════════════════════════════════════════════════
-- The other ten domains get their tiers
-- ═══════════════════════════════════════════════════════════════════
--
-- Same six names and the same thresholds as code. Not because the domains are
-- interchangeable — a Principal designer and a Principal kernel engineer have
-- nothing in common — but because a tier is a position on a scale, the scales
-- are each calibrated by their own weights, and giving each domain its own
-- vocabulary would mean nobody can compare a profile to itself across two
-- domains.
--
-- The weights are what differ, and they are seeded per domain as each one
-- gets a scoring service. A domain with no weights scores zero for everybody,
-- which reads correctly as "not measured here yet".

INSERT INTO craft_score_tiers
    (skill_domain, slug, name, min_score, max_score, description, sort_order)
SELECT d.domain, t.slug, t.name, t.min_score, t.max_score, t.description, t.sort_order
  FROM (VALUES
        ('design'), ('game'), ('security'), ('ai'), ('ops'),
        ('quality'), ('leadership'), ('audio'), ('communication'), ('education')
       ) AS d(domain)
  CROSS JOIN (
        SELECT slug, name, min_score, max_score, description, sort_order
          FROM craft_score_tiers WHERE skill_domain = 'code'
       ) AS t
ON CONFLICT (skill_domain, slug) DO NOTHING;
