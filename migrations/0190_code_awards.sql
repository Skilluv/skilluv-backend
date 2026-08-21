-- The Skilluv Code Awards.
--
-- ## Why this is not a tournament
--
-- A tournament has a start, an end, and people who registered for it. An
-- award has none of those: nobody enters, the work was already done for its
-- own reasons, and the whole year is the window. Forcing it into
-- `tournaments` would mean a registration nobody performs and a score nobody
-- competed for.
--
-- ## Why the vote is weighted, and why the weights are stored
--
-- 70% community, 30% jury. Community alone rewards whoever has the most
-- followers, which is the exact failure Skilluv exists to avoid — the point
-- is that the work is visible, not the person. Jury alone is a small room
-- picking winners, which nobody outside the room believes.
--
-- The weights are a column rather than a constant because the split is a
-- decision somebody will revisit, and a decision that lives in a compiled
-- binary cannot be revisited without a deployment.

CREATE TABLE award_editions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The year the work happened in, not the year the ceremony is held.
    year SMALLINT NOT NULL UNIQUE CHECK (year BETWEEN 2025 AND 2100),
    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN (
        -- Categories exist, nothing is public yet.
        'draft',
        -- Anybody can put work forward.
        'nominations',
        -- The shortlist is fixed and the vote is open.
        'voting',
        -- Counted and published. Nothing moves after this.
        'concluded'
    )),
    community_weight SMALLINT NOT NULL DEFAULT 70 CHECK (community_weight BETWEEN 0 AND 100),
    jury_weight SMALLINT NOT NULL DEFAULT 30 CHECK (jury_weight BETWEEN 0 AND 100),
    nominations_close_at TIMESTAMPTZ,
    voting_closes_at TIMESTAMPTZ,
    -- Per category, in euros. Nullable because an edition may run on
    -- recognition alone, and saying so is better than writing zero.
    prize_amount_eur NUMERIC(10,2) CHECK (prize_amount_eur IS NULL OR prize_amount_eur >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT award_weights_sum_to_a_hundred
        CHECK (community_weight + jury_weight = 100)
);

COMMENT ON TABLE award_editions IS
    'One year of the Code Awards. The weights are stored, not compiled: the '
    '70/30 split is a decision somebody will revisit.';

CREATE TABLE award_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE,
    name VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    -- What a nomination in this category points at. A library is a project,
    -- a contribution is a deliverable, a rookie is a person.
    subject_type VARCHAR(20) NOT NULL CHECK (subject_type IN ('user', 'project', 'deliverable')),
    sort_order SMALLINT NOT NULL DEFAULT 100,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE award_categories IS
    'The categories, as rows. Adding a ninth must not require a deployment.';

CREATE TABLE award_nominees (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    edition_id UUID NOT NULL REFERENCES award_editions(id) ON DELETE CASCADE,
    category_id UUID NOT NULL REFERENCES award_categories(id) ON DELETE CASCADE,
    -- Polymorphic, matching the category's `subject_type`. Enforced by a
    -- trigger below: a foreign key cannot point at three tables, and a
    -- nullable column per table would let a row name two subjects at once.
    subject_type VARCHAR(20) NOT NULL CHECK (subject_type IN ('user', 'project', 'deliverable')),
    subject_id UUID NOT NULL,
    nominated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    -- Why this deserves it. Required: a nomination with no case made for it
    -- is a name, and voters cannot weigh a name.
    citation TEXT NOT NULL CHECK (btrim(citation) <> ''),
    -- Set when the curators fix the shortlist. Only shortlisted nominees are
    -- votable, which is what makes the vote finite.
    shortlisted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- The same work cannot be nominated twice in one category. It can be
    -- nominated in two categories, which is normal: a library that is also a
    -- devtool belongs in both.
    UNIQUE (edition_id, category_id, subject_type, subject_id)
);

CREATE INDEX idx_award_nominees_edition ON award_nominees (edition_id, category_id);
CREATE INDEX idx_award_nominees_shortlist
    ON award_nominees (edition_id, category_id)
    WHERE shortlisted_at IS NOT NULL;

CREATE TABLE award_votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    nominee_id UUID NOT NULL REFERENCES award_nominees(id) ON DELETE CASCADE,
    voter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Which half of the split this vote counts in. Stored on the vote rather
    -- than read from the voter's capabilities at counting time: somebody who
    -- joins the jury in December must not retroactively reweight the vote
    -- they cast in March.
    ballot VARCHAR(20) NOT NULL CHECK (ballot IN ('community', 'jury')),
    -- Denormalised so the one-vote-per-category rule can be a unique index
    -- rather than a trigger reading two joins.
    edition_id UUID NOT NULL REFERENCES award_editions(id) ON DELETE CASCADE,
    category_id UUID NOT NULL REFERENCES award_categories(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One vote per person per category per ballot. A juror votes twice —
    -- once as a member of the community, once as a juror — and that is
    -- deliberate: they are a member of the community.
    UNIQUE (edition_id, category_id, voter_id, ballot)
);

CREATE INDEX idx_award_votes_nominee ON award_votes (nominee_id, ballot);

COMMENT ON COLUMN award_votes.ballot IS
    'Which half of the 70/30 split this counts in, fixed when the vote is '
    'cast. Joining the jury later must not reweight what you already voted.';

-- ═══════════════════════════════════════════════════════════════════
-- A nominee must exist, and must be the kind the category asks for
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION award_nominee_subject_is_real()
RETURNS TRIGGER AS $$
DECLARE
    expected TEXT;
    found BOOLEAN;
BEGIN
    SELECT subject_type INTO expected
      FROM award_categories WHERE id = NEW.category_id;

    IF expected IS DISTINCT FROM NEW.subject_type THEN
        RAISE EXCEPTION 'this category nominates a %, not a %', expected, NEW.subject_type;
    END IF;

    CASE NEW.subject_type
        WHEN 'user' THEN
            SELECT EXISTS (SELECT 1 FROM users WHERE id = NEW.subject_id) INTO found;
        WHEN 'project' THEN
            SELECT EXISTS (SELECT 1 FROM projects WHERE id = NEW.subject_id) INTO found;
        WHEN 'deliverable' THEN
            SELECT EXISTS (SELECT 1 FROM deliverables WHERE id = NEW.subject_id) INTO found;
    END CASE;

    IF NOT found THEN
        RAISE EXCEPTION 'nominated % does not exist', NEW.subject_type;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_award_nominee_subject_is_real
    BEFORE INSERT OR UPDATE ON award_nominees
    FOR EACH ROW EXECUTE FUNCTION award_nominee_subject_is_real();

-- ═══════════════════════════════════════════════════════════════════
-- Only a shortlisted nominee can be voted for
-- ═══════════════════════════════════════════════════════════════════
--
-- And the vote must land in the category the nominee is actually in — the
-- denormalised columns exist to make the unique index possible, and nothing
-- else guarantees they agree with the nominee they name.

CREATE OR REPLACE FUNCTION award_vote_is_well_formed()
RETURNS TRIGGER AS $$
DECLARE
    n RECORD;
    edition_status TEXT;
BEGIN
    SELECT edition_id, category_id, shortlisted_at INTO n
      FROM award_nominees WHERE id = NEW.nominee_id;

    IF n.shortlisted_at IS NULL THEN
        RAISE EXCEPTION 'this nominee is not on the shortlist'
            USING HINT = 'voting opens on the shortlist, not on every nomination';
    END IF;

    IF NEW.edition_id <> n.edition_id OR NEW.category_id <> n.category_id THEN
        RAISE EXCEPTION 'vote does not match the nominee''s edition and category';
    END IF;

    SELECT status INTO edition_status FROM award_editions WHERE id = NEW.edition_id;
    IF edition_status <> 'voting' THEN
        RAISE EXCEPTION 'this edition is %, not voting', edition_status;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_award_vote_is_well_formed
    BEFORE INSERT ON award_votes
    FOR EACH ROW EXECUTE FUNCTION award_vote_is_well_formed();

-- ═══════════════════════════════════════════════════════════════════
-- Counting
-- ═══════════════════════════════════════════════════════════════════
--
-- Each ballot is normalised to a share of its own electorate before the
-- weights are applied. Otherwise 30% jury weight applied to eight jurors
-- against four thousand community votes is 30% of nothing.

CREATE OR REPLACE VIEW award_results AS
WITH tallies AS (
    SELECT n.id AS nominee_id,
           n.edition_id,
           n.category_id,
           count(*) FILTER (WHERE v.ballot = 'community') AS community_votes,
           count(*) FILTER (WHERE v.ballot = 'jury') AS jury_votes
      FROM award_nominees n
      LEFT JOIN award_votes v ON v.nominee_id = n.id
     WHERE n.shortlisted_at IS NOT NULL
     GROUP BY n.id, n.edition_id, n.category_id
),
totals AS (
    SELECT edition_id, category_id,
           sum(community_votes) AS community_total,
           sum(jury_votes) AS jury_total
      FROM tallies
     GROUP BY edition_id, category_id
)
SELECT t.nominee_id,
       t.edition_id,
       t.category_id,
       t.community_votes,
       t.jury_votes,
       ROUND(
           e.community_weight
               * COALESCE(t.community_votes::NUMERIC / NULLIF(o.community_total, 0), 0)
         + e.jury_weight
               * COALESCE(t.jury_votes::NUMERIC / NULLIF(o.jury_total, 0), 0),
           4
       ) AS weighted_score
  FROM tallies t
  JOIN totals o ON o.edition_id = t.edition_id AND o.category_id = t.category_id
  JOIN award_editions e ON e.id = t.edition_id;

COMMENT ON VIEW award_results IS
    'Weighted standings. Each ballot is a share of its own electorate first, '
    'so eight jurors carry the jury weight rather than being drowned by four '
    'thousand community votes.';

-- ═══════════════════════════════════════════════════════════════════
-- The eight categories
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO award_categories (slug, name, description, subject_type, sort_order)
VALUES
    ('best-oss-contribution',
     'Best OSS Contribution of the Year',
     'One merged contribution upstream that changed something for the people using the project. Judged on what it made possible, not on its size.',
     'deliverable', 10),

    ('best-library-published',
     'Best Library Published',
     'A package other people depend on. Published to a registry, documented well enough to use without reading the source.',
     'project', 20),

    ('best-devtool-created',
     'Best Devtool Created',
     'A CLI, extension or plugin that other developers adopted. Adoption is the criterion — a tool with one user is a script.',
     'project', 30),

    ('best-web-project',
     'Best Web Project Shipped',
     'Shipped, reachable, and used by somebody who did not build it.',
     'project', 40),

    ('best-mobile-app',
     'Best Mobile App Shipped',
     'Published to a store or distributed as an installable build, with real users.',
     'project', 50),

    ('best-systems-project',
     'Best Systems Project',
     'Kernel, embedded, drivers, runtimes. The work that is hardest to show and easiest to overlook.',
     'project', 60),

    ('best-blockchain-project',
     'Best Blockchain Project',
     'Deployed and audited, or deployed and honest about not being audited.',
     'project', 70),

    ('rookie-coder',
     'Rookie Coder of the Year',
     'The strongest first year. Measured from the first verified artifact, so somebody who started late is not penalised for it.',
     'user', 80);

CREATE OR REPLACE FUNCTION touch_award_editions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_award_editions_updated_at
    BEFORE UPDATE ON award_editions
    FOR EACH ROW EXECUTE FUNCTION touch_award_editions_updated_at();
