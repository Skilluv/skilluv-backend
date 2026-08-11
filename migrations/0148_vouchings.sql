-- SKI-46 (Post-MVP T3-03) — reputation staking.
--
-- The cold-start problem for juniors: with zero proofs, nothing on a
-- profile distinguishes "hasn't started" from "isn't capable". A senior
-- who knows them can say so — and, crucially, put something of their own
-- behind it. A recommendation that costs nothing signals nothing.
--
-- ## What is actually at stake
--
-- If the vouched user is caught in fraud within the vouching window, the
-- voucher takes a temporary rank penalty. `at_stake_kind` names what was
-- wagered so the terms are recorded per row rather than inferred from
-- whatever the policy happens to be when the vouching is broken.
--
-- The penalty is deliberately *temporary* and recorded in
-- `rank_overrides`, not applied by mutating `user_ranks`: ranks are
-- derived from proofs and unidirectional by design (see
-- `services::ranks`), and writing a demotion into that table would be a
-- second, contradicting source of truth for the same field.
--
-- ## Guard rails
--
-- Vouching is restricted to Doyen (the top rank) and capped per voucher in
-- the service layer. Both are policy, not schema: the rank is derived, and
-- a cap enforced by a constraint could not be relaxed without a migration.

CREATE TABLE IF NOT EXISTS vouchings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    voucher_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    vouched_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Vouchings expire. An open-ended one would accumulate risk the
    -- voucher forgot they were carrying.
    active_until TIMESTAMPTZ NOT NULL,
    at_stake_kind VARCHAR(20) NOT NULL DEFAULT 'rank_temporary'
        CHECK (at_stake_kind IN ('rank_temporary', 'reputation_only')),
    -- Public justification. This is the part a recruiter reads.
    statement TEXT NOT NULL DEFAULT '' CHECK (length(statement) <= 1000),
    broken_at TIMESTAMPTZ,
    break_reason TEXT,
    broken_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT vouchings_not_self CHECK (voucher_id <> vouched_id),
    CONSTRAINT vouchings_break_coherent CHECK (
        (broken_at IS NULL AND break_reason IS NULL)
        OR (broken_at IS NOT NULL AND break_reason IS NOT NULL)
    )
);

-- One live vouching per pair. Partial, so the same senior can vouch again
-- after a previous one expired.
CREATE UNIQUE INDEX IF NOT EXISTS idx_vouchings_unique_live
    ON vouchings (voucher_id, vouched_id)
    WHERE broken_at IS NULL;

-- "Who vouched for this user" — read by the profile and by talent search.
CREATE INDEX IF NOT EXISTS idx_vouchings_by_vouched
    ON vouchings (vouched_id)
    WHERE broken_at IS NULL;

-- "What am I currently backing" — read by the voucher's own dashboard.
CREATE INDEX IF NOT EXISTS idx_vouchings_by_voucher
    ON vouchings (voucher_id, created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Temporary rank penalty
-- ═══════════════════════════════════════════════════════════════════
--
-- The obvious implementation — write a lower value into `user_ranks.rank`
-- — does not work here, and it is worth being explicit about why.
--
-- `user_ranks.rank` is DERIVED from proofs, and
-- `ranks::recompute_rank_for_user` is deliberately unidirectional: it only
-- ever moves a user up. A demotion written into that column would be
-- silently undone by the very next recompute, because the deliverables and
-- attestations that earned the rank are all still there. The penalty would
-- last until the next verified deliverable and no longer.
--
-- So the penalty is a separate LAYER over the derived rank:
--
--   * `rank` keeps meaning "what the proofs say" — untouched, still
--     unidirectional, still recomputable at any time;
--   * `penalty_until` marks a window during which the EFFECTIVE rank is
--     one step below the derived one;
--   * the window expires on its own — no job has to remember to lift it,
--     and a crashed worker cannot leave someone penalised forever.
--
-- `ranks::effective_rank` applies the layer. Public surfaces read that;
-- the proof engine keeps reading `rank`. Neither contradicts the other,
-- because they answer different questions.

ALTER TABLE user_ranks
    ADD COLUMN IF NOT EXISTS penalty_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS penalty_source_vouching_id UUID
        REFERENCES vouchings(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_user_ranks_penalised
    ON user_ranks (penalty_until)
    WHERE penalty_until IS NOT NULL;

-- `rank_overrides` (migration 0102) stays the governance journal for every
-- rank adjustment that did not come from proofs. Linking the row to its
-- vouching keeps "why is this person penalised" answerable from one place.

ALTER TABLE rank_overrides
    ADD COLUMN IF NOT EXISTS source_vouching_id UUID
        REFERENCES vouchings(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_rank_overrides_by_vouching
    ON rank_overrides (source_vouching_id)
    WHERE source_vouching_id IS NOT NULL;
