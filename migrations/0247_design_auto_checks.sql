-- What a machine can say about a design version, and what it deliberately
-- cannot.
--
-- ## Why this table is not a verdict
--
-- The design domain has no green CI, and that is the premise the whole
-- workflow is built on: every verdict is a person's. Nothing stored here
-- changes a status, blocks a submission, or feeds a score. A version can have
-- an `error` row and be approved, and it can have a clean run and be rejected
-- — the second is the common case, because no check knows whether a mark is
-- right for a cooperative.
--
-- What these rows do is take the arithmetic off a reviewer's hands. A
-- contrast ratio has one right answer, and a person computing it by hand is a
-- person not looking at the drawing.
--
-- ## Why severity is not a gate
--
-- `error` means "this is almost certainly wrong and worth a sentence in the
-- critique". It does not refuse anything. A check that blocked would have to
-- be right every time, and the first false positive on somebody's deliberate
-- choice teaches a whole community to work around it — after which the panel
-- is noise nobody reads.
--
-- ## Why an unreadable artefact leaves a row
--
-- A Figma or Miro address cannot be read without holding somebody's design
-- account, which the platform does not do. Those runs record an `info` saying
-- so, because silence and success must not look alike: a reviewer opening an
-- empty panel would read it as "everything passed".
--
-- ## One truth per round
--
-- Results are keyed by round and replaced when a round is re-checked, rather
-- than appended. Two contradictory contrast readings side by side is how a
-- reviewer learns to ignore the panel.

CREATE TABLE design_auto_check_results (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id   UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    -- Which version was read. Matches `slice_validation_decisions.round`, and
    -- SMALLINT for the same reason it is there: the ceiling is five.
    round      SMALLINT NOT NULL CHECK (round BETWEEN 1 AND 5),
    -- Which check spoke: `palette_contrast`, `token_lint`, `motion_cost`,
    -- `svg_consistency`, `fetch`. Not a CHECK constraint — the set grows as
    -- checks are written, and a migration per plugin would be friction with
    -- no safety in return, because nothing downstream branches on the value.
    check_type VARCHAR(40) NOT NULL,
    severity   VARCHAR(10) NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    -- One sentence for the reviewer, in French like the review grids.
    message    TEXT NOT NULL CHECK (length(message) BETWEEN 1 AND 2000),
    -- The numbers behind the sentence, for a client that wants to show them.
    details    JSONB,
    ran_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE design_auto_check_results IS
    'What automatic checks found on a design version. Advisory only: nothing '
    'here blocks a submission, changes a status or feeds a score. A version '
    'can pass every check and be rejected, which is a correct outcome.';

COMMENT ON COLUMN design_auto_check_results.severity IS
    'info: a fact worth showing. warning: probably wrong, weigh it. error: '
    'almost certainly wrong and worth a sentence in the critique. None of the '
    'three refuses anything.';

-- The reviewer's panel reads every round of one slice at once.
CREATE INDEX idx_design_auto_checks_slice
    ON design_auto_check_results (slice_id, round);
