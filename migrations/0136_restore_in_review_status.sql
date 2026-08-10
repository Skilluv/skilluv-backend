-- Restore 'in_review' as a valid project_slices.status value.
--
-- SKI-77 (migration 0119) dropped 'in_review' from the CHECK constraint
-- assuming it was never wired. It IS wired — the legacy bounty flow
-- (routes/bounties.rs), deliverable review queue (services/deliverables.rs,
-- services/reviews.rs) and existing tests all read and write 'in_review'.
--
-- Restoring the status keeps the legacy code path working while the new
-- workflow's submitted -> ci_green -> pending_validation triplet remains
-- the canonical progression for P26 v2 challenges. A follow-up migration
-- can retire 'in_review' once every legacy call-site has been rewritten
-- to emit 'pending_validation' explicitly.

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
        'validated',
        'merged',
        'closed',
        'expired'
    ));
