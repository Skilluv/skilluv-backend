-- Scoring a review against its grid.
--
-- Migration 0180 wrote nine grids: named criteria with a sentence each saying
-- what good looks like. They have been documents ever since — a reviewer
-- could read them and nothing recorded whether they had.
--
-- This is where a review meets its grid. It matters for two reasons that
-- have nothing to do with the number:
--
--   * a reviewer who has to put a figure against "Tests" reads the tests.
--     A free-text verdict lets the whole grid be skipped by writing "LGTM";
--   * the person whose work it is gets told where they stand and on what.
--     "Rejected" teaches nothing; "3 on correctness, 1 on tests" is a
--     week's work with a direction.
--
-- The average is what the craft score reads, and it is counted from 3 rather
-- than from 0: the middle of a five-point grid is the middle, and a formula
-- that pays for it pays everybody for turning up.

CREATE TABLE review_grid_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- One scoring per review. A reviewer who changes their mind edits it;
    -- two rows would mean two opinions from one person on one artefact.
    review_id UUID NOT NULL UNIQUE REFERENCES reviews(id) ON DELETE CASCADE,
    grid_id UUID NOT NULL REFERENCES review_grids(id) ON DELETE RESTRICT,

    -- {"Correction": 4, "Tests": 2, ...}. Keyed by the criterion name from
    -- the grid, so a score can be read next to the sentence that defines it
    -- without a join through a criteria table that does not exist.
    scores JSONB NOT NULL,
    -- Derived from `scores` by the trigger below, so nothing can store an
    -- average that disagrees with the figures it came from.
    average NUMERIC(3,2) NOT NULL,
    -- What the numbers do not say. Optional, unlike the numbers.
    notes TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT scores_is_an_object CHECK (jsonb_typeof(scores) = 'object'),
    CONSTRAINT average_is_on_the_scale CHECK (average >= 1 AND average <= 5)
);

COMMENT ON TABLE review_grid_scores IS
    'A review scored against its grid. The point is not the number: a '
    'reviewer who must put a figure against "Tests" reads the tests, and the '
    'author learns where they stand instead of only that they failed.';

CREATE INDEX idx_review_grid_scores_grid ON review_grid_scores (grid_id);

-- ═══════════════════════════════════════════════════════════════════
-- The scores must be the grid's criteria, on the grid's scale
-- ═══════════════════════════════════════════════════════════════════
--
-- Both halves matter. A score against a criterion the grid does not have is
-- a reviewer inventing one; a missing criterion is a reviewer skipping the
-- part they did not want to look at, which is the exact behaviour this table
-- exists to prevent.

CREATE OR REPLACE FUNCTION review_scores_match_their_grid()
RETURNS TRIGGER AS $$
DECLARE
    expected TEXT[];
    given TEXT[];
    missing TEXT[];
    unknown TEXT[];
    value JSONB;
    total NUMERIC := 0;
    counted INTEGER := 0;
BEGIN
    SELECT array_agg(c ->> 'criterion' ORDER BY c ->> 'criterion')
      INTO expected
      FROM review_grids g, jsonb_array_elements(g.criteria) AS c
     WHERE g.id = NEW.grid_id;

    SELECT array_agg(k ORDER BY k) INTO given
      FROM jsonb_object_keys(NEW.scores) AS k;

    SELECT array_agg(e) INTO missing
      FROM unnest(expected) AS e
     WHERE NOT (e = ANY(COALESCE(given, ARRAY[]::TEXT[])));

    IF missing IS NOT NULL AND cardinality(missing) > 0 THEN
        RAISE EXCEPTION 'the grid asks about % and this review says nothing about it',
            array_to_string(missing, ', ')
            USING HINT = 'every criterion is scored, or the grid is decoration';
    END IF;

    SELECT array_agg(gk) INTO unknown
      FROM unnest(given) AS gk
     WHERE NOT (gk = ANY(COALESCE(expected, ARRAY[]::TEXT[])));

    IF unknown IS NOT NULL AND cardinality(unknown) > 0 THEN
        RAISE EXCEPTION 'this grid has no criterion called %',
            array_to_string(unknown, ', ');
    END IF;

    FOR value IN SELECT v FROM jsonb_each(NEW.scores) AS e(k, v) LOOP
        IF jsonb_typeof(value) <> 'number' THEN
            RAISE EXCEPTION 'a grid score is a number from 1 to 5';
        END IF;
        IF (value #>> '{}')::NUMERIC < 1 OR (value #>> '{}')::NUMERIC > 5 THEN
            RAISE EXCEPTION 'a grid score is a number from 1 to 5, got %', value;
        END IF;
        total := total + (value #>> '{}')::NUMERIC;
        counted := counted + 1;
    END LOOP;

    IF counted = 0 THEN
        RAISE EXCEPTION 'an empty grid scoring is not a scoring';
    END IF;

    -- Derived here rather than taken from the caller: an average that
    -- disagrees with its figures is the one number nobody would ever check.
    NEW.average := ROUND(total / counted, 2);
    NEW.updated_at := NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_review_scores_match_their_grid
    BEFORE INSERT OR UPDATE ON review_grid_scores
    FOR EACH ROW EXECUTE FUNCTION review_scores_match_their_grid();
