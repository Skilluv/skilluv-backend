-- Business model — paid mentoring, consolidated.
-- Migration 0238.
--
-- ## What the audit found
--
-- Ticket 09-01 asks whether the four mentoring modes from migration 0107 are
-- working end to end. They are not. The schema declares `volunteer`,
-- `paid_session`, `paid_monthly` and `hybrid`; `mentor_volunteer_hours` and
-- `mentor_referral_commissions` have tables, indexes and careful comments —
-- and nothing in the codebase reads or writes any of it. Only `paid_session`
-- is wired.
--
-- That is not a bug to file, it is the section. A mode a mentor can choose
-- and that then does nothing is worse than a mode that does not exist: the
-- mentor who picked `paid_monthly` believes they are earning.
--
-- So this migration adds what `paid_monthly` needs to work, and the service
-- layer wires the volunteer hours and the referral commission that were left
-- dormant.
--
-- ## Two products, one table
--
-- A premium cohort (mentees pay per head) and corporate mentoring (a company
-- pays for its own juniors) are the same object with a different payer: a
-- mentor, a group, a monthly or per-head price, a duration, and a commission.
-- The payer is a column, and it is the column the commission rate reads.

-- ═══════════════════════════════════════════════════════════════════
-- The monthly subscription that had nowhere to live
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE mentor_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mentee_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Frozen at subscription. A mentor raising their price must not change
    -- what somebody is already paying without them agreeing to it again.
    monthly_fee_cents BIGINT NOT NULL CHECK (monthly_fee_cents > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    platform_percent NUMERIC(5,2) NOT NULL DEFAULT 20.00
        CHECK (platform_percent >= 0 AND platform_percent <= 30),

    -- How many sessions a month the price includes. Stated, because "monthly
    -- mentoring" without a number is a subscription whose value is whatever
    -- the mentor feels like that month.
    sessions_included SMALLINT NOT NULL DEFAULT 2
        CHECK (sessions_included BETWEEN 1 AND 20),

    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Access is read from this rather than from a status, so a lapsed
    -- subscription cannot be left open by a billing job that failed to run.
    current_period_end TIMESTAMPTZ NOT NULL,
    cancelled_at TIMESTAMPTZ,
    auto_renew BOOLEAN NOT NULL DEFAULT TRUE,

    stripe_subscription_id VARCHAR(80),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_period_runs_forward CHECK (current_period_end > started_at),
    CONSTRAINT nobody_mentors_themselves CHECK (mentor_user_id <> mentee_user_id)
);

COMMENT ON COLUMN mentor_subscriptions.sessions_included IS
    'Stated, because monthly mentoring without a number is a subscription '
    'whose value is whatever the mentor feels like that month.';

-- One live subscription per pair. A second would be a second charge for the
-- same relationship.
CREATE UNIQUE INDEX idx_one_live_mentor_subscription
    ON mentor_subscriptions (mentor_user_id, mentee_user_id)
    WHERE cancelled_at IS NULL;

CREATE INDEX idx_mentor_subscriptions_renewing
    ON mentor_subscriptions (current_period_end)
    WHERE cancelled_at IS NULL AND auto_renew;

-- What was actually delivered against a subscription. Without it, "two
-- sessions a month" is a promise nobody can check and nobody can dispute.
CREATE TABLE mentor_subscription_sessions (
    subscription_id UUID NOT NULL REFERENCES mentor_subscriptions(id) ON DELETE CASCADE,
    session_id UUID NOT NULL REFERENCES mentorship_sessions(id) ON DELETE CASCADE,
    -- The month it counts against, as its first day.
    counts_for_month DATE NOT NULL,

    PRIMARY KEY (subscription_id, session_id),

    CONSTRAINT a_month_is_its_first_day CHECK (EXTRACT(DAY FROM counts_for_month) = 1)
);

CREATE INDEX idx_subscription_sessions_month
    ON mentor_subscription_sessions (subscription_id, counts_for_month);

-- ═══════════════════════════════════════════════════════════════════
-- One-off availability
-- ═══════════════════════════════════════════════════════════════════
--
-- `mentor_availability` from migration 0044 is a recurring weekly pattern:
-- every Tuesday from two to four. A marketplace of one-off sessions needs a
-- mentor to be able to open a single afternoon without committing to it every
-- week for ever.
--
-- A nullable date rather than a second table: it is the same slot, and two
-- tables would mean two booking paths and two places to check a clash.

ALTER TABLE mentor_availability
    ADD COLUMN specific_date DATE,
    -- Set when a booking takes it. A recurring slot is never consumed; a
    -- one-off is, and offering it twice is how two people arrive at once.
    ADD COLUMN consumed_by_session_id UUID
        REFERENCES mentorship_sessions(id) ON DELETE SET NULL;

ALTER TABLE mentor_availability
    ADD CONSTRAINT only_a_one_off_is_consumed CHECK (
        consumed_by_session_id IS NULL OR specific_date IS NOT NULL
    );

COMMENT ON COLUMN mentor_availability.specific_date IS
    'NULL for the recurring weekly pattern from migration 0044. Set for a '
    'one-off slot, which is consumed when booked — offering it twice is how '
    'two people arrive at once.';

CREATE INDEX idx_mentor_availability_one_off
    ON mentor_availability (specific_date, mentor_user_id)
    WHERE specific_date IS NOT NULL AND consumed_by_session_id IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Programmes: premium cohorts and corporate mentoring
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE mentoring_programs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    kind VARCHAR(20) NOT NULL CHECK (kind IN (
        -- A group of mentees who each pay for a structured run.
        'premium_cohort',
        -- A company paying for its own junior staff.
        'corporate'
    )),
    -- Who pays. Derived from the kind and stored anyway, because it is what
    -- the commission rate reads and a rule that has to be recomputed from a
    -- kind is a rule that gets recomputed differently somewhere.
    payer VARCHAR(20) NOT NULL CHECK (payer IN ('mentee', 'enterprise')),
    enterprise_id UUID REFERENCES enterprises(id) ON DELETE CASCADE,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),
    skill_domain VARCHAR(30) NOT NULL,

    duration_months SMALLINT NOT NULL CHECK (duration_months BETWEEN 1 AND 12),
    sessions_per_month SMALLINT NOT NULL DEFAULT 2
        CHECK (sessions_per_month BETWEEN 1 AND 20),

    -- Per head for a cohort, per month for corporate. Which one it means is
    -- decided by the kind, and both cannot be set.
    price_per_mentee NUMERIC(10,2) CHECK (price_per_mentee IS NULL OR price_per_mentee > 0),
    monthly_fee NUMERIC(10,2) CHECK (monthly_fee IS NULL OR monthly_fee > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    -- Higher on corporate, because Skilluv found the client. Stored on the
    -- programme so a rate change never rewrites a run already sold.
    commission_percent NUMERIC(5,2) NOT NULL
        CHECK (commission_percent >= 0 AND commission_percent <= 30),

    max_mentees SMALLINT NOT NULL CHECK (max_mentees BETWEEN 1 AND 50),
    starts_on DATE,
    ends_on DATE,

    status VARCHAR(20) NOT NULL DEFAULT 'recruiting' CHECK (status IN (
        'recruiting', 'running', 'finished', 'cancelled'
    )),
    cancelled_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_programme_runs_forward CHECK (
        ends_on IS NULL OR starts_on IS NULL OR ends_on > starts_on
    ),
    -- Each kind carries its own figure and its own payer, and only its own.
    CONSTRAINT a_cohort_is_priced_per_head CHECK (
        kind <> 'premium_cohort'
        OR (payer = 'mentee' AND price_per_mentee IS NOT NULL AND monthly_fee IS NULL)
    ),
    CONSTRAINT corporate_mentoring_is_priced_monthly_and_billed_to_a_company CHECK (
        kind <> 'corporate'
        OR (payer = 'enterprise' AND monthly_fee IS NOT NULL
            AND price_per_mentee IS NULL AND enterprise_id IS NOT NULL)
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')
    )
);

COMMENT ON TABLE mentoring_programs IS
    'A premium cohort and a corporate engagement are the same object with a '
    'different payer. The payer is a column, and it is the column the '
    'commission rate reads.';

COMMENT ON COLUMN mentoring_programs.commission_percent IS
    'Higher on corporate, because Skilluv found the client. Stored on the '
    'programme so a rate change never rewrites a run already sold.';

CREATE INDEX idx_mentoring_programs_open
    ON mentoring_programs (kind, created_at DESC)
    WHERE status = 'recruiting';
CREATE INDEX idx_mentoring_programs_mentor
    ON mentoring_programs (mentor_user_id, created_at DESC);

CREATE OR REPLACE FUNCTION touch_mentoring_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_mentoring_programs_updated_at
    BEFORE UPDATE ON mentoring_programs
    FOR EACH ROW EXECUTE FUNCTION touch_mentoring_updated_at();

CREATE TABLE mentoring_program_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    program_id UUID NOT NULL REFERENCES mentoring_programs(id) ON DELETE CASCADE,

    -- A cohort mentee is a Skilluv user. A corporate mentee is one of the
    -- client's employees, who may never have heard of us — so an email is
    -- enough to enrol them, and they can claim the account later.
    mentee_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    mentee_email VARCHAR(255),
    mentee_name VARCHAR(120),

    enrolled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    amount_paid NUMERIC(10,2) CHECK (amount_paid IS NULL OR amount_paid >= 0),
    payment_reference VARCHAR(200),

    graduated_at TIMESTAMPTZ,
    left_at TIMESTAMPTZ,
    left_reason TEXT,

    status VARCHAR(20) NOT NULL DEFAULT 'enrolled' CHECK (status IN (
        'enrolled', 'active', 'graduated', 'left'
    )),

    CONSTRAINT a_member_is_named_once CHECK (
        (mentee_user_id IS NOT NULL)::int + (mentee_email IS NOT NULL)::int = 1
    )
);

COMMENT ON COLUMN mentoring_program_members.mentee_email IS
    'A corporate mentee is the client''s employee, who may never have heard '
    'of Skilluv. An email is enough to enrol them; the account can come '
    'later.';

CREATE UNIQUE INDEX idx_one_enrolment_per_user
    ON mentoring_program_members (program_id, mentee_user_id)
    WHERE mentee_user_id IS NOT NULL;
CREATE UNIQUE INDEX idx_one_enrolment_per_email
    ON mentoring_program_members (program_id, lower(mentee_email))
    WHERE mentee_email IS NOT NULL;

CREATE INDEX idx_program_members_program ON mentoring_program_members (program_id);

-- A programme stops enrolling once it is full, and once it is no longer
-- recruiting. Held in the database because two people taking the last place
-- at the same moment would both pass a check in the service.
CREATE OR REPLACE FUNCTION mentoring_program_has_room()
RETURNS TRIGGER AS $$
DECLARE
    program RECORD;
    taken INTEGER;
BEGIN
    SELECT status, max_mentees INTO program
      FROM mentoring_programs WHERE id = NEW.program_id FOR UPDATE;

    IF program.status <> 'recruiting' THEN
        RAISE EXCEPTION 'this programme is %, and is not enrolling', program.status;
    END IF;

    SELECT count(*) INTO taken
      FROM mentoring_program_members
     WHERE program_id = NEW.program_id AND status <> 'left';

    IF taken >= program.max_mentees THEN
        RAISE EXCEPTION 'this programme already has its % mentees', program.max_mentees;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_mentoring_program_has_room
    BEFORE INSERT ON mentoring_program_members
    FOR EACH ROW EXECUTE FUNCTION mentoring_program_has_room();

-- ═══════════════════════════════════════════════════════════════════
-- The revenue streams these feed
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revenue_streams (slug, pillar, label, description, recurring) VALUES
    ('mentoring_program', 'ecosystem', 'Programme de mentorat',
     'La commission Skilluv sur une cohorte de mentorat ou un engagement de '
     'mentorat en entreprise.',
     TRUE)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO enterprise_product_types
    (slug, label, description, revenue_stream, recurring)
VALUES
    ('corporate_mentoring', 'Mentorat en entreprise',
     'Des mentors Skilluv accompagnant les juniors d''une entreprise.',
     'mentoring_program', TRUE)
ON CONFLICT (slug) DO NOTHING;
