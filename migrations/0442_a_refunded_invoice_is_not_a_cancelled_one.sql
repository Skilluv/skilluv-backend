-- An invoice whose money went back is not an invoice that was never paid.
--
-- `mission_invoices.status` had `cancelled`, which migration 0194 gave the
-- meaning "issued and then withdrawn before anybody paid it" -- the constraint
-- `paid_invoice_names_its_payment` lets a cancelled invoice have no payment at
-- all. Reusing it for a refund would say that a mission where the client paid
-- two thousand euros and got them back is indistinguishable from one where the
-- invoice was deleted the day it was written. An accountant reading the table
-- could not tell the two apart, and neither could the reconciliation.
--
-- So: a status of its own, and a timestamp of its own, next to `released_at`.
--
-- The whole list is restated because a CHECK cannot be extended, only
-- replaced. This is the seventh time that has cost this repository a
-- vocabulary; the list below is the one from 0194 plus one value, and nothing
-- has touched it in between.

ALTER TABLE mission_invoices
    DROP CONSTRAINT IF EXISTS mission_invoices_status_check;

ALTER TABLE mission_invoices
    ADD CONSTRAINT mission_invoices_status_check
    CHECK (status IN (
        'issued',
        -- The enterprise paid; the money is at the provider and the talent's
        -- share sits in their pending balance.
        'paid',
        -- The mission closed: pending became available and it can be
        -- withdrawn.
        'released',
        -- Withdrawn before anybody paid it. No money ever moved.
        'cancelled',
        -- Paid, then given back: the mission was cancelled or arbitrated
        -- against the delivery while the share was still pending. Money moved
        -- twice and the books say so both times.
        'refunded'
    ));

ALTER TABLE mission_invoices
    ADD COLUMN IF NOT EXISTS refunded_at TIMESTAMPTZ;

COMMENT ON COLUMN mission_invoices.refunded_at IS
    'When the captured amount went back to the payer. Distinct from '
    'cancelled, which is an invoice nobody ever paid.';

-- A refund names its payment for the same reason a capture does: money that
-- moved left a trace of where it moved from, and a refund is the second
-- movement of the same money.
ALTER TABLE mission_invoices
    DROP CONSTRAINT IF EXISTS paid_invoice_names_its_payment;

ALTER TABLE mission_invoices
    ADD CONSTRAINT paid_invoice_names_its_payment CHECK (
        status NOT IN ('paid', 'released', 'refunded') OR payment_id IS NOT NULL
    );

-- And it carries a reason, because somebody will ask why. Reusing the
-- cancellation column rather than adding a second one: both are "this invoice
-- stopped, here is why", and two columns would leave a reader checking the
-- wrong one half the time.
ALTER TABLE mission_invoices
    DROP CONSTRAINT IF EXISTS invoice_cancellation_carries_a_reason;

ALTER TABLE mission_invoices
    ADD CONSTRAINT invoice_cancellation_carries_a_reason CHECK (
        status NOT IN ('cancelled', 'refunded')
        OR (cancellation_reason IS NOT NULL AND btrim(cancellation_reason) <> '')
    );
