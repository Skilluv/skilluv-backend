-- A review that asks for another version, and says which version it read.
--
-- ## Why this is not a new table
--
-- The design backlog asked for `design_iteration_rounds`: one row per round,
-- holding the version submitted and the critique that answered it. Migration
-- 0184 already built that, for every domain, on
-- `slice_validation_decisions` — numbered rounds, a five-round ceiling, and a
-- categorised blocking reason. A second table would have duplicated the
-- ceiling, the numbering trigger and the admin statistics, and the two would
-- have disagreed within a quarter.
--
-- Three things were genuinely missing, and they are what this migration adds.
--
-- ## One: a third verdict
--
-- Code review is binary — the diff is right or it is not. Design review is
-- not, and its most common outcome is neither: "this is going somewhere, come
-- back with another version". Recording that as a rejection would corrupt the
-- history — a designer being coached would read as a designer being refused —
-- and it would corrupt the admin statistics, where `approve_ratio` would
-- punish exactly the behaviour the design charter asks for.
--
-- ## Two: which version was read
--
-- Round four's critique is about round four's file. Without the address of
-- what the reviewer had in front of them, the trail is a list of opinions
-- with nothing to check them against, and the "iteration story" a designer
-- puts on their profile cannot be reconstructed.
--
-- ## Three: the filled grid
--
-- Migration 0230 gives design its criteria. A review that names them and
-- scores them is what turns "it looks off" into something actionable, and
-- what lets two reviewers reach comparable conclusions on the same work.

-- ═══════════════════════════════════════════════════════════════════
-- The third verdict
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE slice_validation_decisions
    DROP CONSTRAINT IF EXISTS slice_validation_decisions_decision_check;

ALTER TABLE slice_validation_decisions
    ADD CONSTRAINT slice_validation_decisions_decision_check
    CHECK (decision IN ('approve', 'reject', 'iterate'));

COMMENT ON COLUMN slice_validation_decisions.decision IS
    'approve: the challenge is a success. reject: refused for good. '
    'iterate: another version is expected, and the challenge stays open.';

-- Design rejections and iterations need reasons the code list cannot express:
-- a direction that does not answer the brief is not "scope_mismatch", and a
-- contrast failure is not "review_comments".
ALTER TABLE slice_validation_decisions
    DROP CONSTRAINT IF EXISTS slice_validation_decisions_blocking_reason_check;

ALTER TABLE slice_validation_decisions
    ADD CONSTRAINT slice_validation_decisions_blocking_reason_check
    CHECK (blocking_reason IS NULL OR blocking_reason IN (
        -- Code and shared (migration 0184)
        'ci_failing',
        'tests_missing',
        'docs_missing',
        'review_comments',
        'scope_mismatch',
        'out_of_depth',
        -- Design
        'brief_unmet',          -- answers a question the brief did not ask
        'direction_mismatch',   -- the direction itself, not its execution
        'craft_gap',            -- the idea holds, the execution does not
        'accessibility',        -- contrast, sizes, motion, non-visual paths
        'system_inconsistent',  -- ignores the design system it belongs to
        'rights_unclear',       -- fonts, images or sounds without a licence
        'derivative'            -- too close to existing work to publish
    ));

-- An approval needs no blocking reason. A rejection and an iteration both do:
-- telling somebody to come back without saying what to change wastes a round.
ALTER TABLE slice_validation_decisions
    DROP CONSTRAINT IF EXISTS slice_validation_reason_matches_decision;

ALTER TABLE slice_validation_decisions
    ADD CONSTRAINT slice_validation_reason_matches_decision
    CHECK (
        (decision = 'approve' AND blocking_reason IS NULL)
        OR (decision IN ('reject', 'iterate') AND blocking_reason IS NOT NULL)
    );

-- ═══════════════════════════════════════════════════════════════════
-- What was reviewed, and against what
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE slice_validation_decisions
    -- The exact version this decision is about. Immutable once written:
    -- replacing it would erase the only thing that makes the critique
    -- checkable afterwards.
    ADD COLUMN reviewed_artifact_url TEXT
        CHECK (reviewed_artifact_url IS NULL
               OR length(reviewed_artifact_url) BETWEEN 4 AND 2048),
    -- What the author said about the version this decision read. Copied from
    -- the slice at review time so each round keeps both halves: the claim and
    -- the finding.
    ADD COLUMN reviewed_artifact_notes_md TEXT,
    -- The filled grid, shaped like migration 0404's criteria:
    --   {"grid": "motion",
    --    "scores": [{"criterion": "Rythme", "score": 4, "comment": "..."}],
    --    "average": 3.8}
    ADD COLUMN grid_scores JSONB
        CHECK (grid_scores IS NULL OR jsonb_typeof(grid_scores) = 'object');

COMMENT ON COLUMN slice_validation_decisions.reviewed_artifact_url IS
    'The version this decision is about. Round four''s critique is about '
    'round four''s file, and without the address the trail cannot be checked.';

COMMENT ON COLUMN slice_validation_decisions.grid_scores IS
    'The review grid of migration 0404, filled in. Named criteria are what '
    'turn a verdict into something the designer can act on.';

-- The craft score (a designer's average across the grids they received) and
-- the public iteration story both read this shape.
CREATE INDEX idx_slice_validation_decisions_graded
    ON slice_validation_decisions (slice_id, round)
    WHERE grid_scores IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- The status a slice sits in between two rounds
-- ═══════════════════════════════════════════════════════════════════
--
-- `claimed` would have done, and would have lost the distinction that
-- matters: a slice nobody has looked at yet and a slice carrying a critique
-- its author owes an answer to are different queues, for the designer and for
-- the reviewer both.

ALTER TABLE project_slices DROP CONSTRAINT IF EXISTS project_slices_status_check;

ALTER TABLE project_slices ADD CONSTRAINT project_slices_status_check
    CHECK (status IN (
        'draft',
        'open',
        'claimed',
        'in_review',            -- legacy bounty/deliverable flow (pre-SKI-77)
        'in_progress',
        'submitted',
        'ci_green',
        'pending_validation',
        'in_iteration',         -- a critique was delivered, a version is owed
        'validated',
        'merged',
        'closed',
        'expired'
    ));

-- Waiting on a new version is a design state. Elsewhere a rejection sends the
-- slice back to `claimed`, and nothing would set this.
ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_iteration_status_is_design;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_iteration_status_is_design
    CHECK (status <> 'in_iteration' OR slice_type = 'design_artifact');
