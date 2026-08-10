-- SKI-114 (M-08) — Journal of validator decisions on slices.
--
-- Prior to this migration, reject counts were inferred from the current
-- state of a slice: "was picked_by_validator_id but slice is now claimed
-- with validation_reject_reason IS NOT NULL". This double-counts when a
-- slice is rejected → re-picked → rejected again, inflating the
-- denominator of approve_ratio and understating a validator's real
-- approval rate.
--
-- The right shape is an append-only journal — one row per decision.
-- Same-slice re-pickups are counted correctly, and we gain the historic
-- decision trail for free ("this slice was rejected twice before being
-- validated").

CREATE TABLE slice_validation_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    validator_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    decision TEXT NOT NULL CHECK (decision IN ('approve', 'reject')),
    -- Only carried on rejects (validators are not asked to justify approvals).
    reason TEXT,
    -- Snapshot of picked_at at decision time. Lets us compute an exact
    -- pickup->decision latency per decision without relying on the mutable
    -- picked_at column on the slice (cleared on reject).
    picked_at TIMESTAMPTZ,
    decided_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Aggregate queries hit these two shapes most often.
CREATE INDEX idx_svd_validator_decided ON slice_validation_decisions (validator_id, decided_at DESC);
CREATE INDEX idx_svd_slice_decided ON slice_validation_decisions (slice_id, decided_at DESC);
