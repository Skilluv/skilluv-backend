-- Business model — the internal sales pipeline.
-- Migration 0242.
--
-- ## A CRM for a team that does not exist yet
--
-- Ticket 14-08 asks for a CRM for the Skilluv sales team. There is no sales
-- team: three volunteers, no users, no revenue. Building a Salesforce would
-- be building for a company we are not.
--
-- What is worth building now is the part that does not depend on headcount:
-- a record of who we are talking to, what was said, and what is due to renew.
-- Three tables, no lead scoring, no forecast, no territory. The rest can be
-- added when there is somebody whose job it is to want it.
--
-- ## Renewals are derived, not tracked
--
-- Every recurring product already knows when it lapses — subscriptions,
-- entitlements, certifications, annual contracts. A `renewal_date` column
-- here would be a copy of eight other columns, wrong the first time one of
-- them moved. The renewal view reads them.

CREATE TABLE sales_opportunities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The company, if they have an account. An opportunity often starts
    -- before one exists, which is why the name is a column too.
    enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL,
    org_name VARCHAR(200) NOT NULL CHECK (btrim(org_name) <> ''),
    contact_name VARCHAR(120),
    contact_email VARCHAR(255),

    -- What we think they would buy. A foreign key rather than free text, so
    -- the pipeline can be read against the product catalogue rather than
    -- against somebody's spelling.
    product_type VARCHAR(60) REFERENCES enterprise_product_types(slug) ON DELETE SET NULL,
    estimated_value NUMERIC(12,2) CHECK (estimated_value IS NULL OR estimated_value >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF', 'USD')),

    stage VARCHAR(20) NOT NULL DEFAULT 'lead' CHECK (stage IN (
        'lead',
        'qualified',
        'proposal',
        'negotiation',
        'won',
        'lost'
    )),
    -- Why it was lost. The only field in this table anybody will thank us
    -- for in a year: a pipeline that records wins and shrugs at losses
    -- teaches nothing.
    lost_reason TEXT,

    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    expected_close_on DATE,
    closed_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_loss_says_why CHECK (
        stage <> 'lost' OR (lost_reason IS NOT NULL AND btrim(lost_reason) <> '')
    ),
    CONSTRAINT a_closed_opportunity_is_dated CHECK (
        stage NOT IN ('won', 'lost') OR closed_at IS NOT NULL
    )
);

COMMENT ON COLUMN sales_opportunities.lost_reason IS
    'The only field here anybody will thank us for in a year. A pipeline '
    'that records wins and shrugs at losses teaches nothing.';

CREATE INDEX idx_opportunities_open
    ON sales_opportunities (stage, expected_close_on)
    WHERE stage NOT IN ('won', 'lost');
CREATE INDEX idx_opportunities_enterprise
    ON sales_opportunities (enterprise_id)
    WHERE enterprise_id IS NOT NULL;

CREATE OR REPLACE FUNCTION touch_sales_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_opportunities_updated_at
    BEFORE UPDATE ON sales_opportunities
    FOR EACH ROW EXECUTE FUNCTION touch_sales_updated_at();

-- What was actually said, and when.
CREATE TABLE sales_activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    opportunity_id UUID NOT NULL REFERENCES sales_opportunities(id) ON DELETE CASCADE,

    kind VARCHAR(20) NOT NULL CHECK (kind IN (
        'call', 'email', 'meeting', 'demo', 'proposal_sent', 'note'
    )),
    summary_md TEXT NOT NULL CHECK (btrim(summary_md) <> ''),
    happened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- What we said we would do. A CRM without this is a diary.
    next_step TEXT,
    next_step_due_on DATE,

    author_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON COLUMN sales_activities.next_step IS
    'What we said we would do. A CRM without this is a diary.';

CREATE INDEX idx_activities_opportunity
    ON sales_activities (opportunity_id, happened_at DESC);
CREATE INDEX idx_activities_due
    ON sales_activities (next_step_due_on)
    WHERE next_step_due_on IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What is due to renew
-- ═══════════════════════════════════════════════════════════════════
--
-- Read from the products themselves. Every recurring thing already knows
-- when it lapses, and a second copy would be wrong the first time one of
-- them moved.

CREATE OR REPLACE VIEW upcoming_renewals AS
    SELECT
        'corporate_learning'::TEXT AS product,
        s.enterprise_id,
        s.id AS source_id,
        s.current_period_end AS renews_at,
        (s.monthly_fee_per_seat * s.seats) AS value,
        s.currency
      FROM corporate_learning_subscriptions s
     WHERE s.cancelled_at IS NULL AND s.auto_renew

    UNION ALL

    SELECT
        'data_room', d.enterprise_id, d.id, d.expires_at, d.monthly_fee, d.currency
      FROM data_room_subscriptions d
     WHERE d.cancelled_at IS NULL AND d.auto_renew

    UNION ALL

    SELECT
        'annual_sponsorship', a.enterprise_id, a.id,
        make_timestamptz(a.year + 1, 1, 1, 0, 0, 0), a.total_fee, a.currency
      FROM annual_sponsorship_contracts a
     WHERE a.signed_at IS NOT NULL

    UNION ALL

    SELECT
        'certification', c.subject_enterprise_id, c.id, c.expires_at, c.fee, c.currency
      FROM certifications c
     WHERE c.status = 'issued' AND c.subject_enterprise_id IS NOT NULL

    UNION ALL

    SELECT
        'white_label', NULL::UUID, w.id,
        (w.contract_ends_on::TIMESTAMPTZ), COALESCE(w.annual_fee, w.monthly_fee * 12),
        w.currency
      FROM white_label_deployments w
     WHERE w.status = 'live' AND w.contract_ends_on IS NOT NULL

    UNION ALL

    SELECT
        'ambassador_program', p.enterprise_id, p.id, p.ends_at,
        p.management_monthly_fee, p.currency
      FROM ambassador_programs p
     WHERE p.status = 'running' AND p.ends_at IS NOT NULL;

COMMENT ON VIEW upcoming_renewals IS
    'Read from the products themselves. Every recurring thing already knows '
    'when it lapses, and a renewal_date column would be a copy of six others '
    'that is wrong the first time one of them moves.';
