-- SKI-77 (P26 v2 C-03) — Extend project_slices.status with new workflow statuses.
--
-- New sequence (see docs/challenges-target-model-and-roadmap.md part G.1):
--   draft -> open -> claimed -> in_progress -> submitted -> ci_green ->
--   pending_validation -> validated -> merged
--                                ^                          ^
--                                |                          |
--                        (rejected: -> claimed)     (bonus, not required for success)
--
-- Kept statuses: draft, open, claimed, merged, closed, expired.
-- Removed status: 'in_review' (superseded by the more precise
-- submitted/ci_green/pending_validation triplet). No production data uses
-- 'in_review' (this workflow was never wired), so drop is safe.
--
-- Key design decision (2026-08-06): the success of a challenge is `validated`
-- (fragments distributed, attestation generated). `merged` is a bonus that
-- adds extra fragments + tags the attestation. This decouples Skilluv success
-- from the upstream maintainer's merge decision (which can take months on
-- external repos).

ALTER TABLE project_slices DROP CONSTRAINT IF EXISTS project_slices_status_check;

ALTER TABLE project_slices ADD CONSTRAINT project_slices_status_check
    CHECK (status IN (
        'draft',                -- ingested but not yet published (curator_review mode)
        'open',                 -- visible, claim-able by any eligible user
        'claimed',              -- reserved by a user, working on it
        'in_progress',          -- user has pushed activity on their fork
        'submitted',            -- PR opened upstream, waiting for CI
        'ci_green',             -- CI passed, pickable by a validator
        'pending_validation',   -- a validator has picked it up
        'validated',            -- SUCCESS: fragments + attestation issued
        'merged',               -- BONUS: upstream maintainer has merged the PR
        'closed',               -- closed without success (obsolete, duplicate)
        'expired'               -- claim expired, returned to pool as 'open' via job
    ));

-- Validation tracking columns.
ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS validated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS validated_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;

-- Index for admin/analytics queries filtering on validated_at (e.g. weekly
-- rollups of successful challenges).
CREATE INDEX IF NOT EXISTS idx_slices_validated_at
    ON project_slices (validated_at DESC)
    WHERE validated_at IS NOT NULL;

-- Coherence constraint: if validated_at is set, validated_by_user_id must be
-- set too (a slice cannot be marked validated without a responsible party).
ALTER TABLE project_slices DROP CONSTRAINT IF EXISTS project_slices_validation_coherent;
ALTER TABLE project_slices ADD CONSTRAINT project_slices_validation_coherent
    CHECK (
        (validated_at IS NULL AND validated_by_user_id IS NULL)
        OR (validated_at IS NOT NULL AND validated_by_user_id IS NOT NULL)
    );
