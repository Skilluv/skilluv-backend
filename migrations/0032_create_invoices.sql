-- Phase 3.10 — invoices (sequential SKL-YYYY-NNNNN numbering, 10-year retention).

CREATE TABLE invoice_counters (
    year INTEGER PRIMARY KEY,
    next_number INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_number VARCHAR(20) NOT NULL UNIQUE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE RESTRICT,
    amount_ht_cents BIGINT NOT NULL CHECK (amount_ht_cents >= 0),
    amount_tva_cents BIGINT NOT NULL DEFAULT 0 CHECK (amount_tva_cents >= 0),
    amount_ttc_cents BIGINT NOT NULL CHECK (amount_ttc_cents >= 0),
    tva_rate NUMERIC(5,2) NOT NULL DEFAULT 0,
    currency CHAR(3) NOT NULL DEFAULT 'EUR',
    billing_country VARCHAR(2),
    billing_company_name TEXT,
    billing_address TEXT,
    billing_vat_number TEXT,
    description TEXT,
    stripe_payment_intent_id VARCHAR(120),
    stripe_session_id VARCHAR(120),
    related_transaction_id UUID REFERENCES credit_transactions(id) ON DELETE SET NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invoices_enterprise ON invoices (enterprise_id, issued_at DESC);
CREATE INDEX idx_invoices_stripe_pi ON invoices (stripe_payment_intent_id) WHERE stripe_payment_intent_id IS NOT NULL;
-- Yearly indexing is implicit via the existing (enterprise_id, issued_at DESC) index.

-- Helper: claim the next invoice number for the given year, atomically.
CREATE OR REPLACE FUNCTION claim_invoice_number(p_year INT) RETURNS INT AS $$
DECLARE n INT;
BEGIN
    INSERT INTO invoice_counters (year, next_number) VALUES (p_year, 1)
    ON CONFLICT (year) DO UPDATE SET next_number = invoice_counters.next_number
    RETURNING next_number INTO n;
    UPDATE invoice_counters SET next_number = next_number + 1 WHERE year = p_year;
    RETURN n;
END;
$$ LANGUAGE plpgsql;
