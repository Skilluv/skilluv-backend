-- SKI-83 (P26 v2 D-01) — validator pickup on submitted slices.
--
-- Once CI is green (workflow status `ci_green`), an eligible validator
-- (see migration 0120: challenge_validator:{domain} capabilities) picks
-- the PR up for review. The pickup is exclusive: only one validator can
-- hold a slice at a time. Advances status to `pending_validation`.
--
-- Approve (SKI-84) advances to `validated` (challenge success).
-- Reject (SKI-85) resets to `claimed` with a reason recorded.
--
-- Fields:
--   picked_by_validator_id  → user_id of the current validator holder
--   picked_at               → when the pickup happened
--   validation_reject_reason → last rejection reason (kept for audit /
--                               visibility to the challenger)

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS picked_by_validator_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS picked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS validation_reject_reason TEXT;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_pickup_coherent
    CHECK (
        (picked_by_validator_id IS NULL AND picked_at IS NULL)
        OR (picked_by_validator_id IS NOT NULL AND picked_at IS NOT NULL)
    );

CREATE INDEX IF NOT EXISTS idx_slices_pending_validation
    ON project_slices (picked_at DESC)
    WHERE status = 'pending_validation';
