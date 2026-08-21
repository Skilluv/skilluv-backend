-- What blocks an AI submission.
--
-- ## Why not six rounds instead of five
--
-- The backlog asked to raise the cap for AI, on the grounds that
-- experimentation needs more attempts. It needs more attempts at the
-- *experiment*, not more round trips through review — a person runs thirty
-- evaluations before submitting once. Migration 0184 set five because past
-- that the problem is no longer the work: it is the scope, the brief, or the
-- match between the two, and a sixth identical rejection says none of that.
-- That argument does not weaken for models. The cap stays.
--
-- ## Why the reasons are AI-specific
--
-- The six existing values describe a repository: tests missing, CI red, docs
-- absent. None of them names the reason an AI submission actually gets sent
-- back — a score measured on the training set, a run nobody can repeat, a
-- dataset whose provenance is unclear. Rejecting those as `review_comments`
-- tells the contributor to go read comments that do not exist.

ALTER TABLE slice_validation_decisions
    DROP CONSTRAINT IF EXISTS slice_validation_decisions_blocking_reason_check;

ALTER TABLE slice_validation_decisions
    ADD CONSTRAINT slice_validation_decisions_blocking_reason_check
    CHECK (blocking_reason IS NULL OR blocking_reason IN (
        -- Migration 0184
        'ci_failing',            -- the automated checks are red
        'tests_missing',         -- no tests, or tests that assert nothing
        'docs_missing',          -- undocumented, which the charter refuses
        'review_comments',       -- naming, structure, small corrections
        'scope_mismatch',        -- the work is not what the slice asked for
        'out_of_depth',          -- beyond where this contributor is today
        -- AI. Each names something a reviewer sees constantly and could not
        -- say before.
        'eval_insufficient',     -- no held-out set, no baseline, or a score
                                 -- measured on what the model was trained on
        'reproducibility_missing', -- seeds, versions or data not pinned; the
                                 -- numbers cannot be obtained again
        'data_provenance_unclear', -- where the data came from, or under which
                                 -- licence, is not stated
        'safety_concern'         -- publishing this as it stands does harm:
                                 -- undisclosed finding, missing guardrail,
                                 -- personal data in a released artefact
    ));

COMMENT ON COLUMN slice_validation_decisions.blocking_reason IS
    'What kind of problem, for a rejection. "Rejected" alone tells a '
    'contributor nothing about what to do next, and an AI submission sent '
    'back as "review comments" sends them looking for comments nobody wrote.';
