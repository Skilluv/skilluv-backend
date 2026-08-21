-- Contests that belong together.
--
-- ## The two features this replaces
--
-- The design backlog asked for two things that read as separate formats:
--
--   * **Skilluv Design Awards** — an annual edition with thirteen categories,
--     one per family of trades.
--   * **Design sprints** — a weekend with an imposed theme, ending in a
--     showcase.
--
-- Neither is a new format. An awards edition is thirteen contests judged in
-- parallel and read as one event; a sprint is a contest with a very short
-- window, run again every few weeks. What both actually need — and what
-- neither could express — is a way to say *these contests are one thing*.
--
-- Built as two bespoke features, they would have shipped two tables, two sets
-- of routes and two definitions of "who won overall", and the third format
-- somebody thinks of next year would have been a third.
--
-- ## What a series is not
--
-- It is not a season. `seasons` already exists and means something else: a
-- period the whole platform is in, which every tournament belongs to whether
-- it wants to or not. A series is opt-in and narrow — thirteen contests out
-- of the fifty running that month.
--
-- It is also not a bracket. Contests in a series do not feed each other;
-- they are read together, not played through. A format where the winner of
-- one enters the next is a different thing and is not this.
--
-- ## Why the kind matters
--
-- `awards_edition` and `sprint` are read differently. An edition's page lists
-- thirteen podiums; a sprint's lists one, plus the last six sprints. Storing
-- the kind lets one set of routes serve both without a client guessing from
-- the number of contests.

CREATE TABLE tournament_series (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug        VARCHAR(80) NOT NULL UNIQUE,
    name        VARCHAR(160) NOT NULL CHECK (length(name) BETWEEN 3 AND 160),
    description TEXT,

    kind        VARCHAR(30) NOT NULL CHECK (kind IN (
        -- Many contests judged in parallel and read as one event.
        'awards_edition',
        -- One short contest, repeated. The series is the run of them.
        'sprint',
        -- Anything grouped for editorial reasons: a partner's season, a
        -- themed month. Deliberately vague, because the alternative is a
        -- migration every time somebody has an idea.
        'programme'
    )),

    -- NULL means the series crosses domains, like a tournament's own
    -- `skill_domain`: an awards edition that spans code and design is a real
    -- thing and should not need a second series.
    skill_domain VARCHAR(30),

    -- When the whole thing runs. Not derived from the contests inside it: an
    -- edition is announced before its categories exist, and a page that could
    -- not say "opens in March" until the first contest was created would be
    -- useless for the month that matters most.
    starts_at   TIMESTAMPTZ NOT NULL,
    ends_at     TIMESTAMPTZ NOT NULL,

    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT tournament_series_ends_after_it_starts CHECK (ends_at > starts_at)
);

COMMENT ON TABLE tournament_series IS
    'Contests read as one event: an awards edition, a run of sprints, a '
    'themed programme. Opt-in and narrow — not a season, which every '
    'tournament belongs to whether it wants to or not.';

COMMENT ON COLUMN tournament_series.starts_at IS
    'When the series runs, stated rather than derived. An edition is '
    'announced before its categories exist.';

ALTER TABLE tournaments
    ADD COLUMN series_id UUID REFERENCES tournament_series(id) ON DELETE SET NULL,
    -- Which category of the series this contest is. For an awards edition,
    -- the family: `brand`, `motion`, … Free text rather than a foreign key to
    -- `orientations.reviewer_group`, because a series may be organised along
    -- an axis that is not the review families — "best first work", "best
    -- collaboration" — and forcing it into the family vocabulary would make
    -- those unrepresentable.
    ADD COLUMN series_category VARCHAR(60);

COMMENT ON COLUMN tournaments.series_category IS
    'Which category of its series this contest is — a family for an awards '
    'edition, or an editorial axis. NULL for a contest that is the whole of '
    'its series, which is what a sprint is.';

-- A category appears once per series. Two "best motion" categories in one
-- edition is a mistake nobody notices until the results page shows two
-- winners of the same thing.
CREATE UNIQUE INDEX idx_tournaments_one_contest_per_category
    ON tournaments (series_id, series_category)
    WHERE series_id IS NOT NULL AND series_category IS NOT NULL;

-- The edition page: every contest of a series, in category order.
CREATE INDEX idx_tournaments_by_series ON tournaments (series_id, series_category)
    WHERE series_id IS NOT NULL;

-- A category on no series is a category of nothing.
ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_category_belongs_to_a_series
    CHECK (series_category IS NULL OR series_id IS NOT NULL);
