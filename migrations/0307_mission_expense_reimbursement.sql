-- Reimbursing what the work cost, without taking a cut of it.
--
-- ## Why this exists, and only really for AI
--
-- A designer needs Figma. A backend developer needs a laptop. Someone
-- fine-tuning a model for a client rents GPUs, and the bill lands on their
-- card before the mission pays anything — a week of an A100 costs more than
-- most people on this platform earn in a month.
--
-- Without a way to reimburse, the choice is between refusing the work and
-- financing the client. Both are ways of saying the marketplace is not for
-- people without savings, which is the opposite of what it is for.
--
-- ## Why a kind on the invoice rather than a table
--
-- Everything an instalment already carries applies: a sequence, a label, a
-- status, a Stripe checkout, a release when the mission closes. A second
-- table would mean a second payment path and two answers to "what does this
-- enterprise still owe".
--
-- What must differ is the commission, and it must differ absolutely.
--
-- ## Nobody takes fifteen percent of a GPU bill
--
-- A reimbursement is money moving through the platform, not money the
-- platform earned. Charging on it would mean somebody pays to be repaid, and
-- the more honest they are about their costs the more it costs them. The
-- constraint below refuses the row rather than trusting a caller to pass zero.
--
-- ## Agreed before, not discovered after
--
-- `expenses_reimbursed` sits on the mission and defaults to false. Somebody
-- who rents a cluster on the assumption that it will be covered, and finds out
-- afterwards that it was not, has lost real money — so the platform will not
-- let a reimbursement be invoiced against a mission that never agreed to one.

ALTER TABLE missions
    ADD COLUMN expenses_reimbursed BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN missions.expenses_reimbursed IS
    'Whether the client agreed to reimburse costs incurred doing the work — '
    'compute, mostly. Stated when the mission is published, because somebody '
    'who rents a cluster on an assumption and is wrong has lost real money.';

ALTER TABLE mission_invoices
    ADD COLUMN kind VARCHAR(24) NOT NULL DEFAULT 'work'
        CHECK (kind IN ('work', 'expense_reimbursement')),
    -- The receipt. Required for a reimbursement, and for the same reason
    -- everything else on this platform carries a link: a figure nobody can
    -- check is a figure somebody has to be believed about.
    ADD COLUMN expense_evidence_url TEXT
        CHECK (expense_evidence_url IS NULL OR expense_evidence_url ~ '^https?://');

COMMENT ON COLUMN mission_invoices.kind IS
    'Work billed, or a cost passed through. The second carries no commission: '
    'a reimbursement is money moving through the platform, not money it '
    'earned.';

ALTER TABLE mission_invoices
    ADD CONSTRAINT reimbursement_carries_no_commission CHECK (
        kind <> 'expense_reimbursement' OR commission_percent = 0
    );

ALTER TABLE mission_invoices
    ADD CONSTRAINT reimbursement_shows_the_receipt CHECK (
        kind <> 'expense_reimbursement' OR expense_evidence_url IS NOT NULL
    );

-- A reimbursement against a mission that never agreed to one.
CREATE OR REPLACE FUNCTION reimbursement_was_agreed()
RETURNS TRIGGER AS $$
DECLARE
    agreed BOOLEAN;
BEGIN
    IF NEW.kind <> 'expense_reimbursement' THEN
        RETURN NEW;
    END IF;

    SELECT expenses_reimbursed INTO agreed FROM missions WHERE id = NEW.mission_id;

    IF NOT COALESCE(agreed, FALSE) THEN
        RAISE EXCEPTION
            'mission % did not agree to reimburse costs', NEW.mission_id
            USING HINT = 'set expenses_reimbursed when the mission is published; '
                         'agreeing after the money is spent is not agreeing';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_reimbursement_was_agreed
    BEFORE INSERT OR UPDATE OF kind ON mission_invoices
    FOR EACH ROW EXECUTE FUNCTION reimbursement_was_agreed();
