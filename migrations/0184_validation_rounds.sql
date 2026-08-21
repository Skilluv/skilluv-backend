-- Rejection as a round, not as a verdict.
--
-- ## What was missing
--
-- A slice could be rejected, reworked and resubmitted any number of times.
-- Each decision was a row and nothing tied them together, so neither side
-- could see where they were: the contributor got a rejection with no sense
-- of progress, and nobody could tell a slice on its second pass from one on
-- its seventh.
--
-- ## Why five
--
-- Not because the sixth attempt would be worthless — because by then the
-- problem is not the code. A slice that has been round the loop five times
-- is mis-scoped, mis-briefed, or beyond the person who claimed it, and the
-- honest response is a human looking at it rather than a sixth identical
-- rejection.
--
-- ## Why the reason is categorised
--
-- "Rejected" tells a contributor nothing about what to do next. CI failing
-- is a different action from a naming comment, which is different again from
-- a slice that was never the right size. Naming which one lets the platform
-- say something useful, and lets an operator see which projects reject for
-- which reasons.

ALTER TABLE slice_validation_decisions
    ADD COLUMN round SMALLINT NOT NULL DEFAULT 1
        CHECK (round BETWEEN 1 AND 5),
    ADD COLUMN blocking_reason VARCHAR(30)
        CHECK (blocking_reason IS NULL OR blocking_reason IN (
            'ci_failing',        -- the automated checks are red
            'tests_missing',     -- no tests, or tests that assert nothing
            'docs_missing',      -- undocumented, which the charter refuses
            'review_comments',   -- naming, structure, small corrections
            'scope_mismatch',    -- the work is not what the slice asked for
            'out_of_depth'       -- beyond where this contributor is today
        ));

COMMENT ON COLUMN slice_validation_decisions.round IS
    'Which pass this is, from one to five. Set by trigger from the decisions '
    'already recorded, so a caller cannot get it wrong or restart the count.';

COMMENT ON COLUMN slice_validation_decisions.blocking_reason IS
    'What kind of problem, for a rejection. "Rejected" alone tells a '
    'contributor nothing about what to do next.';

-- An approval needs no blocking reason, and a rejection is not useful
-- without one.
ALTER TABLE slice_validation_decisions
    ADD CONSTRAINT slice_validation_reason_matches_decision
    CHECK (
        (decision = 'approve' AND blocking_reason IS NULL)
        OR (decision = 'reject' AND blocking_reason IS NOT NULL)
    );

-- ═══════════════════════════════════════════════════════════════════
-- The round number is derived, never supplied
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION slice_validation_next_round()
RETURNS TRIGGER AS $$
DECLARE
    already SMALLINT;
BEGIN
    SELECT count(*) INTO already
      FROM slice_validation_decisions
     WHERE slice_id = NEW.slice_id;

    IF already >= 5 THEN
        RAISE EXCEPTION
            'slice % has already been through five validation rounds; it needs a '
            'human decision on scope or assignment, not a sixth rejection',
            NEW.slice_id
            USING ERRCODE = 'check_violation';
    END IF;

    NEW.round := already + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_slice_validation_round
    BEFORE INSERT ON slice_validation_decisions
    FOR EACH ROW
    EXECUTE FUNCTION slice_validation_next_round();

-- Existing rows predate the column and are all first rounds by definition:
-- nothing was counting before, so nothing was a second pass on purpose.
UPDATE slice_validation_decisions SET round = 1 WHERE round IS DISTINCT FROM 1;
