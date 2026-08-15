-- How long money stays held before the recipient can withdraw it.
--
-- ─── Why this is not a constant ───────────────────────────────────
--
-- The right delay is not the same for every kind of work, and it is not a
-- technical decision:
--
--   * A mentorship session is immaterial. If the student says the mentor
--     never showed up, it is one person's word against another's, and the
--     only protection is time to complain. Seven days.
--
--   * A merged bounty is a public, verifiable artefact. The contribution is
--     in the upstream repository and anyone can look at it. There is nothing
--     to contest, so holding the money would be a delay with no purpose —
--     and paying a contributor the same day is a real argument against every
--     platform that makes them wait a fortnight.
--
--   * Reverse-marketplace hours are immaterial like mentorship. Same window.
--
-- Card networks allow a chargeback for far longer than any of these — up to
-- 120 days. No window closes that risk; it is bounded by proof of service,
-- by reserves, and by accepting a residual loss. What the window buys is the
-- ordinary case: the payer noticing within a few days that something was
-- wrong, before the money has left.
--
-- Living in a table means changing a delay is an UPDATE, not a deployment,
-- and that the rule is inspectable rather than buried in a match arm.

CREATE TABLE release_windows (
    -- Matches `ledger_transactions.subject_type`, so a flow looks up its own
    -- window by the name it already uses.
    subject_type VARCHAR(40) PRIMARY KEY,
    -- Hours, not days: a zero-day window and a six-hour one are different
    -- products, and days cannot express the second.
    hold_hours INTEGER NOT NULL CHECK (hold_hours >= 0),
    -- Whether the payer can shorten it by confirming early. Almost always
    -- yes — someone saying "this was great" should not have to wait.
    payer_can_release_early BOOLEAN NOT NULL DEFAULT TRUE,
    -- Free-text rationale, so the next person to change a number knows what
    -- the current one was protecting against.
    rationale TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO release_windows (subject_type, hold_hours, payer_can_release_early, rationale) VALUES
    ('mentorship_session', 168, TRUE,
     'Seven days. An immaterial service with no artefact: if the student says the session did not happen, time to complain is the only protection. The student can confirm early and pay the mentor immediately.'),
    ('bounty_slice', 0, TRUE,
     'Immediate. The contribution is merged upstream, public and verifiable — there is nothing to contest. Holding it would be a delay with no purpose, and same-day payment is a real advantage over platforms that hold for two weeks.'),
    ('talent_offer_booking', 168, TRUE,
     'Seven days, same reasoning as mentorship: hours of someone''s time, no artefact to point at.'),
    ('certification_purchase', 0, FALSE,
     'Immediate and not early-releasable: nothing is owed to a third party, the platform is the seller.');

COMMENT ON TABLE release_windows IS
    'Per-subject-type hold before funds move from pending to available. '
    'Read by services::ledger::release_due_at. Changing a delay is an UPDATE.';

-- When each held amount becomes withdrawable.
--
-- Kept next to the ledger rather than on each business table: the sweep that
-- releases expired holds has one place to look, whatever the flow. Without
-- it, every new flow would need its own scheduled job, and the one that
-- forgets leaves people unpaid.
CREATE TABLE pending_releases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The transaction that created the hold.
    ledger_transaction_id UUID NOT NULL
        REFERENCES ledger_transactions(id) ON DELETE RESTRICT,
    beneficiary_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    subject_type VARCHAR(40) NOT NULL,
    subject_id UUID NOT NULL,
    amount NUMERIC(20, 4) NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),
    release_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ,
    -- Set when a dispute freezes the hold, so the sweep skips it rather than
    -- releasing money that is being argued over.
    disputed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One hold per subject. A retried capture must not create a second.
    UNIQUE (subject_type, subject_id)
);

-- The sweep's working set: due, not yet released, not disputed.
CREATE INDEX idx_pending_releases_due
    ON pending_releases (release_at)
    WHERE released_at IS NULL AND disputed_at IS NULL;

CREATE INDEX idx_pending_releases_beneficiary
    ON pending_releases (beneficiary_id, released_at);

COMMENT ON TABLE pending_releases IS
    'Amounts held in a user pending account, with the moment they become '
    'available. One sweep releases them all, so a new flow inherits the '
    'behaviour instead of needing its own job.';
