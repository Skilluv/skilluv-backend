-- Content strategy foundation — mentorship economic model (2026-07-21 decision).
--
-- Rationale: the content strategy session determined that mentors can choose
-- one of 4 modes: volunteer, paid session, paid monthly, hybrid. Talents can
-- pay for value-added services (soft skills, HR, interview prep) but the
-- platform base remains free for talents. A recruitment commission (10% of
-- placement fee) rewards volunteer mentors whose mentees get hired.
--
-- Anti double-dipping rule: a mentor cannot cumulate "paid per session" +
-- "recruitment commission" on the same mentor-mentee relationship. The
-- commission only applies to volunteer hours cumulated ≥ 5h threshold.
--
-- References:
--   - Migration 0044 (Phase 5.11): base mentor_profiles + mentorship_sessions
--     with existing 80/20 split (kept as-is, this migration extends it).
--   - Content strategy doc §14 "modèle mentorship éco" — tranché 2026-07-21.

-- ═══════════════════════════════════════════════════════════════════
-- 1. Extend mentor_profiles with mode + optional monthly subscription
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE mentor_profiles
    ADD COLUMN mode VARCHAR(20) NOT NULL DEFAULT 'volunteer'
        CHECK (mode IN ('volunteer', 'paid_session', 'paid_monthly', 'hybrid'));

ALTER TABLE mentor_profiles
    ADD COLUMN monthly_subscription_eur_cents BIGINT
        CHECK (monthly_subscription_eur_cents IS NULL OR monthly_subscription_eur_cents >= 0);

COMMENT ON COLUMN mentor_profiles.mode IS
    'Mentor economic mode. volunteer = free, gets recruitment commission on mentee placement. paid_session = per-session pricing (hourly_rate_eur_cents). paid_monthly = subscription (monthly_subscription_eur_cents). hybrid = volunteer + paid sessions coexist, commission only on volunteer hours.';

COMMENT ON COLUMN mentor_profiles.monthly_subscription_eur_cents IS
    'Only relevant when mode IN (paid_monthly, hybrid). NULL for volunteer/paid_session only.';

-- ═══════════════════════════════════════════════════════════════════
-- 2. Track volunteer hours per mentor-mentee pair
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE mentor_volunteer_hours (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mentee_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id UUID REFERENCES mentorship_sessions(id) ON DELETE SET NULL,
    hours_spent NUMERIC(4,2) NOT NULL CHECK (hours_spent > 0),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CHECK (mentor_user_id <> mentee_user_id)
);

-- For eligibility checks on recruitment commission (aggregate hours per pair)
CREATE INDEX idx_mentor_volunteer_hours_pair
    ON mentor_volunteer_hours (mentor_user_id, mentee_user_id);

-- For mentor dashboard (their contribution history)
CREATE INDEX idx_mentor_volunteer_hours_mentor_recorded
    ON mentor_volunteer_hours (mentor_user_id, recorded_at DESC);

COMMENT ON TABLE mentor_volunteer_hours IS
    'Tracks volunteer (unpaid) hours cumulated per mentor-mentee pair. Sole basis for eligibility to recruitment commission (see mentor_referral_commissions). Only sessions where the mentor was in volunteer or hybrid-volunteer mode count here.';

-- ═══════════════════════════════════════════════════════════════════
-- 3. Track recruitment commissions when mentees get placed
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE mentor_referral_commissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    mentee_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE RESTRICT,

    -- Commission calculation
    placement_amount_cents BIGINT NOT NULL CHECK (placement_amount_cents > 0),
    mentor_share_cents BIGINT NOT NULL CHECK (mentor_share_cents > 0),
    commission_rate_bps INTEGER NOT NULL DEFAULT 1000
        CHECK (commission_rate_bps > 0 AND commission_rate_bps <= 10000),
    -- 1000 bps = 10% (basis points). Kept configurable in case rate changes.

    -- Eligibility justification (snapshot at time of commission)
    hours_mentored_volunteer NUMERIC(6,2) NOT NULL
        CHECK (hours_mentored_volunteer >= 5.0),
    eligibility_threshold_hours NUMERIC(4,2) NOT NULL DEFAULT 5.0,

    -- Payout tracking (via Stripe Connect Express, mentor's account)
    stripe_transfer_id VARCHAR(80),
    payout_status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (payout_status IN ('pending', 'released', 'failed', 'refunded')),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    released_at TIMESTAMPTZ,

    CHECK (mentor_user_id <> mentee_user_id),
    CHECK (released_at IS NULL OR payout_status IN ('released', 'refunded'))
);

CREATE INDEX idx_mentor_referral_commissions_mentor
    ON mentor_referral_commissions (mentor_user_id, created_at DESC);

CREATE INDEX idx_mentor_referral_commissions_pending_payout
    ON mentor_referral_commissions (payout_status)
    WHERE payout_status = 'pending';

-- Unique constraint: one commission per (mentor, mentee, enterprise) triple.
-- If the same person is hired twice by the same enterprise via the same mentor,
-- this is a strong signal to investigate manually (dedup handled by application).
CREATE UNIQUE INDEX uniq_mentor_referral_commissions_triple
    ON mentor_referral_commissions (mentor_user_id, mentee_user_id, enterprise_id);

COMMENT ON TABLE mentor_referral_commissions IS
    'Recruitment commissions paid to mentors when their mentee is hired by an enterprise via Skilluv Talent Search. Eligibility: ≥5h volunteer hours cumulated (see mentor_volunteer_hours). Anti double-dipping: enforced application-side by checking mode=volunteer or hybrid at the time of the volunteer_hours records.';
