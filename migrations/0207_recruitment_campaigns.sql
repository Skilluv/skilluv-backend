-- Recruitment that Skilluv runs, rather than a search the client runs.
--
-- ## Four tickets, one table
--
-- The backlog describes a managed sourcing campaign (02-01), a volume
-- programme (02-05), and a private retained pool (02-06) as three tables.
-- They differ in three fields: how many positions, whether there is a monthly
-- fee, and whether the shortlist is refreshed on a cadence. Everything else —
-- the brief, the targeting, the shortlist, the statuses, the fee structure —
-- is identical.
--
-- Three tables would mean three shortlist tables, three status machines and
-- three places to fix the next bug. One table with a `kind` says the same
-- thing and keeps the shortlist singular.
--
-- ## What is genuinely separate
--
-- The success fee (02-02) is its own table because it outlives the campaign:
-- a guarantee runs six months past the hire, and a campaign that closed in
-- March still owes a refund in August.

CREATE TABLE recruitment_campaigns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,

    kind VARCHAR(20) NOT NULL DEFAULT 'managed' CHECK (kind IN (
        -- One brief, one shortlist, paid on success.
        'managed',
        -- Same, several positions of the same profile, discounted.
        'volume',
        -- A standing pool refreshed on a cadence, paid monthly, with no
        -- success fee: the client already pays to keep it warm.
        'private_pool'
    )),

    title VARCHAR(200) NOT NULL,
    -- What they are actually looking for, in their words. Markdown, because a
    -- brief with no structure is a brief nobody reads twice.
    brief_md TEXT NOT NULL CHECK (btrim(brief_md) <> ''),
    target_role VARCHAR(200) NOT NULL,
    target_domain VARCHAR(30) NOT NULL,
    -- Trade slugs. Resolved through `resolve_orientation` at read time, so a
    -- brief written before a rename keeps matching.
    target_orientations TEXT[] NOT NULL DEFAULT '{}',
    target_countries TEXT[] NOT NULL DEFAULT '{}',
    -- `beginner`, `junior`, `mid`, `senior`, `staff` — the onboarding's
    -- vocabulary, so a brief and a profile are comparable.
    seniority_range TEXT[] NOT NULL DEFAULT '{}',
    -- {"min": 1200000, "max": 1800000, "currency": "XOF", "period": "year"}.
    -- A range with no currency is not a range.
    salary_range JSONB,
    remote_ok BOOLEAN NOT NULL DEFAULT TRUE,
    positions_count SMALLINT NOT NULL DEFAULT 1 CHECK (positions_count > 0),

    setup_fee NUMERIC(12,2) CHECK (setup_fee IS NULL OR setup_fee >= 0),
    -- Percentage of annual salary, charged on a confirmed hire.
    success_fee_percent NUMERIC(5,2)
        CHECK (success_fee_percent IS NULL
               OR (success_fee_percent > 0 AND success_fee_percent <= 30)),
    -- The reduction earned by volume, already applied to the percentage
    -- above. Stored so the discount can be explained rather than inferred
    -- from a number that looks arbitrary.
    volume_discount_percent NUMERIC(5,2) NOT NULL DEFAULT 0
        CHECK (volume_discount_percent >= 0 AND volume_discount_percent <= 50),
    -- `private_pool` only.
    monthly_fee NUMERIC(12,2) CHECK (monthly_fee IS NULL OR monthly_fee > 0),
    refresh_cadence_days SMALLINT
        CHECK (refresh_cadence_days IS NULL OR refresh_cadence_days BETWEEN 7 AND 180),
    last_refreshed_at TIMESTAMPTZ,
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    status VARCHAR(30) NOT NULL DEFAULT 'briefing' CHECK (status IN (
        'briefing',
        'sourcing',
        'shortlist_delivered',
        'interviews',
        'hired',
        'closed',
        'cancelled'
    )),
    -- Who at Skilluv is doing the sourcing. A campaign with nobody on it is
    -- a campaign nobody is doing.
    assigned_to UUID REFERENCES users(id) ON DELETE SET NULL,
    deadline TIMESTAMPTZ,
    shortlist_delivered_at TIMESTAMPTZ,
    hired_at TIMESTAMPTZ,
    closed_reason TEXT,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT salary_range_is_an_object
        CHECK (salary_range IS NULL OR jsonb_typeof(salary_range) = 'object'),

    -- A retained pool is paid monthly and refreshed; a campaign is paid on
    -- success. Mixing the two would let somebody be charged twice for one
    -- hire.
    CONSTRAINT a_pool_is_paid_monthly CHECK (
        kind <> 'private_pool'
        OR (monthly_fee IS NOT NULL AND refresh_cadence_days IS NOT NULL
            AND success_fee_percent IS NULL)
    ),
    CONSTRAINT a_campaign_is_paid_on_success CHECK (
        kind = 'private_pool' OR success_fee_percent IS NOT NULL
    ),
    -- Volume pricing exists because there are several positions. One position
    -- at a discount is just a discount, and should be recorded as the
    -- percentage it is.
    CONSTRAINT volume_means_several_positions CHECK (
        kind <> 'volume' OR positions_count >= 5
    ),
    CONSTRAINT cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (closed_reason IS NOT NULL AND btrim(closed_reason) <> '')
    )
);

COMMENT ON TABLE recruitment_campaigns IS
    'Sourcing Skilluv runs for a client. Three kinds in one table: they '
    'differ in three fields and share the brief, the shortlist, the statuses '
    'and the fee structure.';

COMMENT ON COLUMN recruitment_campaigns.volume_discount_percent IS
    'Already applied to success_fee_percent. Stored so the discount can be '
    'explained rather than inferred from a number that looks arbitrary.';

CREATE INDEX idx_recruitment_campaigns_enterprise
    ON recruitment_campaigns (enterprise_id, status, created_at DESC);
CREATE INDEX idx_recruitment_campaigns_open
    ON recruitment_campaigns (status, deadline)
    WHERE status IN ('briefing', 'sourcing', 'shortlist_delivered', 'interviews');
CREATE INDEX idx_recruitment_campaigns_assigned
    ON recruitment_campaigns (assigned_to, status)
    WHERE assigned_to IS NOT NULL;
-- Pools due a refresh. The list the curation job reads.
CREATE INDEX idx_recruitment_pools_stale
    ON recruitment_campaigns (last_refreshed_at NULLS FIRST)
    WHERE kind = 'private_pool' AND status NOT IN ('closed', 'cancelled');

CREATE TRIGGER trg_recruitment_campaigns_updated_at
    BEFORE UPDATE ON recruitment_campaigns
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The shortlist
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE recruitment_shortlist (
    campaign_id UUID NOT NULL REFERENCES recruitment_campaigns(id) ON DELETE CASCADE,
    talent_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Why this person, in the recruiter's words, with links. Required: a
    -- shortlist of names with no argument is a search result, and the client
    -- is paying precisely not to have to do that reading themselves.
    match_reason_md TEXT NOT NULL CHECK (btrim(match_reason_md) <> ''),

    status VARCHAR(20) NOT NULL DEFAULT 'proposed' CHECK (status IN (
        'proposed',
        -- The talent was asked and said yes. Nobody reaches a client without
        -- this: presenting somebody who has not agreed is how a platform
        -- burns the trust of the people it depends on.
        'interested',
        'declined',
        'interviewed',
        'hired',
        -- The client looked and passed.
        'passed'
    )),
    talent_notified_at TIMESTAMPTZ,
    talent_responded_at TIMESTAMPTZ,
    decided_at TIMESTAMPTZ,
    decision_note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (campaign_id, talent_user_id)
);

COMMENT ON TABLE recruitment_shortlist IS
    'Who was put forward and why. The reason is required: a list of names '
    'with no argument is a search result, and the client is paying not to do '
    'that reading themselves.';

CREATE INDEX idx_recruitment_shortlist_talent
    ON recruitment_shortlist (talent_user_id, created_at DESC);

-- Nobody is presented to a client without having agreed.
--
-- A trigger rather than a service check: the shortlist is written by an admin
-- endpoint, a curation job and eventually an import, and the rule has to hold
-- for all three.
CREATE OR REPLACE FUNCTION shortlist_requires_consent()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status IN ('interviewed', 'hired')
       AND OLD.status IS DISTINCT FROM NEW.status
       AND NEW.talent_responded_at IS NULL THEN
        RAISE EXCEPTION 'this person has not agreed to be put forward'
            USING HINT = 'ask them first — presenting somebody who has not '
                         'agreed is how a platform burns the trust it runs on';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_shortlist_requires_consent
    BEFORE UPDATE ON recruitment_shortlist
    FOR EACH ROW EXECUTE FUNCTION shortlist_requires_consent();

-- ═══════════════════════════════════════════════════════════════════
-- The success fee, and the guarantee that outlives the campaign
-- ═══════════════════════════════════════════════════════════════════
--
-- Its own table because it outlives what produced it: a guarantee runs six
-- months past the hire, and a campaign closed in March still owes a refund in
-- August.

CREATE TABLE recruitment_success_fees (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id UUID REFERENCES recruitment_campaigns(id) ON DELETE SET NULL,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    talent_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    hired_at TIMESTAMPTZ NOT NULL,
    -- Declared by the client. Not verified, and named as declared so nobody
    -- later mistakes it for something Skilluv checked.
    annual_salary_declared NUMERIC(14,2) NOT NULL CHECK (annual_salary_declared > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),
    success_fee_percent NUMERIC(5,2) NOT NULL
        CHECK (success_fee_percent > 0 AND success_fee_percent <= 30),
    success_fee_amount NUMERIC(14,2) NOT NULL CHECK (success_fee_amount > 0),

    invoiced_at TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,

    -- Six months from the hire. Stored rather than computed, because the
    -- guarantee period is negotiable and a contract signed at nine months
    -- should not be silently shortened by a constant in the code.
    guarantee_ends_at TIMESTAMPTZ NOT NULL,
    -- Set when the person leaves inside the window.
    left_at TIMESTAMPTZ,
    refund_amount NUMERIC(14,2) CHECK (refund_amount IS NULL OR refund_amount >= 0),
    refund_reason TEXT,
    refunded_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT guarantee_runs_forward CHECK (guarantee_ends_at > hired_at),
    CONSTRAINT a_refund_says_why CHECK (
        refund_amount IS NULL
        OR (refund_reason IS NOT NULL AND btrim(refund_reason) <> '')
    ),
    -- A refund can never exceed what was charged.
    CONSTRAINT a_refund_fits_the_fee CHECK (
        refund_amount IS NULL OR refund_amount <= success_fee_amount
    )
);

COMMENT ON TABLE recruitment_success_fees IS
    'Charged on a confirmed hire, refundable if the person leaves inside the '
    'guarantee. Separate from the campaign because it outlives it.';

COMMENT ON COLUMN recruitment_success_fees.annual_salary_declared IS
    'Declared by the client, not verified. Named as declared so nobody later '
    'mistakes it for something Skilluv checked.';

CREATE INDEX idx_success_fees_enterprise
    ON recruitment_success_fees (enterprise_id, hired_at DESC);
-- What the monthly guarantee sweep reads: still under guarantee, not yet
-- refunded.
CREATE INDEX idx_success_fees_under_guarantee
    ON recruitment_success_fees (guarantee_ends_at)
    WHERE refunded_at IS NULL;
CREATE INDEX idx_success_fees_unpaid
    ON recruitment_success_fees (invoiced_at)
    WHERE paid_at IS NULL;

CREATE TRIGGER trg_success_fees_updated_at
    BEFORE UPDATE ON recruitment_success_fees
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();
