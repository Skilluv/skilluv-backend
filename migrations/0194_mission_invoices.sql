-- What is owed on a mission, and when.
--
-- ## Why a mission needs more than one amount
--
-- `missions.payment_model` names five ways of paying, and four of them are
-- several payments: a retainer is one a month, per_hour is one per batch of
-- approved hours, per_deliverable is one per accepted feature. Only
-- fixed_price is a single figure.
--
-- Without this table the model would be a label on a row and the money would
-- still be one payment for the whole thing — which is the version where
-- somebody works a six-month retainer and gets paid once, at the end, if the
-- client is still there.
--
-- ## Why the commission is copied here too
--
-- It is already frozen on the mission, at selection. It is copied again onto
-- each instalment because a mission can be re-rated between instalments — the
-- talent crossing their tenth delivery mid-retainer is exactly the case — and
-- what somebody was charged on an invoice paid in March must stay readable in
-- November.

CREATE TABLE mission_invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mission_id UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    -- 1, 2, 3… in the order they were issued. What the enterprise sees on
    -- its statement and what somebody quotes when asking about one.
    sequence SMALLINT NOT NULL CHECK (sequence > 0),
    -- "Mars 2026", "Sprint 3", "Livraison finale". Written by whoever issues
    -- it, because only they know what it covers.
    label VARCHAR(200) NOT NULL CHECK (btrim(label) <> ''),

    amount NUMERIC(12,2) NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL DEFAULT 'EUR' CHECK (currency IN ('EUR', 'XOF')),
    commission_percent NUMERIC(5,2) NOT NULL
        CHECK (commission_percent >= 0 AND commission_percent <= 30),

    -- Retainers cover a period; the others do not.
    period_start DATE,
    period_end DATE,
    -- per_hour: what is being billed. Kept next to the amount so the rate can
    -- be checked against the mission rather than taken on trust.
    hours NUMERIC(7,2) CHECK (hours IS NULL OR hours > 0),

    status VARCHAR(20) NOT NULL DEFAULT 'issued' CHECK (status IN (
        'issued',
        -- The enterprise paid; the money is at the provider and the talent's
        -- share sits in their pending balance.
        'paid',
        -- The mission closed: pending became available and it can be
        -- withdrawn.
        'released',
        'cancelled'
    )),
    payment_id UUID REFERENCES payments(id) ON DELETE SET NULL,
    captured_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    cancellation_reason TEXT,

    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (mission_id, sequence),

    CONSTRAINT invoice_period_runs_forward
        CHECK (period_start IS NULL OR period_end IS NULL OR period_end >= period_start),

    CONSTRAINT invoice_cancellation_carries_a_reason CHECK (
        status <> 'cancelled'
        OR (cancellation_reason IS NOT NULL AND btrim(cancellation_reason) <> '')
    ),

    -- Money that moved left a trace of where it moved from.
    CONSTRAINT paid_invoice_names_its_payment CHECK (
        status NOT IN ('paid', 'released') OR payment_id IS NOT NULL
    )
);

COMMENT ON TABLE mission_invoices IS
    'One row per amount owed on a mission. A retainer is one a month; without '
    'this the model would be a label and the money would still be one payment '
    'at the end.';

COMMENT ON COLUMN mission_invoices.commission_percent IS
    'Copied from the mission at issue. A mission can be re-rated between '
    'instalments, and what was charged in March must stay readable in '
    'November.';

CREATE INDEX idx_mission_invoices_mission ON mission_invoices (mission_id, sequence);
CREATE INDEX idx_mission_invoices_unpaid
    ON mission_invoices (issued_at)
    WHERE status = 'issued';

CREATE TRIGGER trg_mission_invoices_updated_at
    BEFORE UPDATE ON mission_invoices
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- An invoice belongs to a mission somebody is actually working on
-- ═══════════════════════════════════════════════════════════════════
--
-- Billing for a mission nobody was selected for means there is no one to pay,
-- and the money would sit in the platform's account with no owner.

CREATE OR REPLACE FUNCTION invoice_requires_somebody_on_the_mission()
RETURNS TRIGGER AS $$
DECLARE
    assignee UUID;
    mission_status TEXT;
BEGIN
    SELECT assigned_user_id, status INTO assignee, mission_status
      FROM missions WHERE id = NEW.mission_id;

    IF assignee IS NULL THEN
        RAISE EXCEPTION 'this mission has nobody on it — there is no one to pay'
            USING HINT = 'select an applicant before issuing an invoice';
    END IF;

    IF mission_status IN ('cancelled', 'closed') THEN
        RAISE EXCEPTION 'this mission is %, invoices are closed', mission_status;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_invoice_requires_somebody_on_the_mission
    BEFORE INSERT ON mission_invoices
    FOR EACH ROW EXECUTE FUNCTION invoice_requires_somebody_on_the_mission();
