-- Record whether the mentor was actually paid for a completed session.
--
-- `POST /mentorship/sessions/{id}/complete` attempted a Stripe Connect
-- transfer, logged a warning if it failed, and then wrote
-- `status = 'completed', payout_released_at = NOW()` regardless. A mentor
-- whose transfer failed — or who has no Connect account at all, which is
-- every mentor outside Stripe's coverage — looked paid and was not. Nothing
-- distinguished those rows from the healthy ones, so nothing could replay
-- them.
--
-- `payout_released_at` keeps its meaning (when the money left), and is now
-- only written on success. `payout_status` says why it is NULL.

ALTER TABLE mentorship_sessions
    ADD COLUMN payout_status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (payout_status IN ('pending', 'paid', 'failed', 'no_account')),
    ADD COLUMN payout_error TEXT,
    ADD COLUMN payout_reference VARCHAR(120);

COMMENT ON COLUMN mentorship_sessions.payout_status IS
    'pending: not attempted yet. paid: transfer accepted by the provider. '
    'failed: attempted and refused, see payout_error. '
    'no_account: the mentor has no payout account on a supported provider.';

COMMENT ON COLUMN mentorship_sessions.payout_reference IS
    'Provider-side transfer id, for reconciliation against their statement.';

-- Sessions completed before this migration were stamped as released whether
-- or not the money moved. Mark the ones that at least ran through the old
-- path as paid, and leave the rest pending: guessing would hide exactly the
-- cases this column exists to surface.
UPDATE mentorship_sessions
SET payout_status = 'paid'
WHERE status = 'completed' AND payout_released_at IS NOT NULL;

-- The replay queue: completed work whose money has not moved.
CREATE INDEX idx_mentorship_sessions_unpaid
    ON mentorship_sessions (payout_status, scheduled_at)
    WHERE payout_status IN ('pending', 'failed', 'no_account');
