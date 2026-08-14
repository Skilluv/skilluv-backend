-- Money coming in, given the same shape as money going out.
--
-- ─── The asymmetry this fixes ─────────────────────────────────────
--
-- Payouts got a trait, a routing table and one adapter per provider, and
-- adding FedaPay cost one file. Collection got none of it. `psp.rs`
-- declares a `PaymentProvider` trait and a `PaymentRegistry` that is never
-- constructed; `psp_africa.rs` implements three African providers and is
-- imported by nothing. Meanwhile `certifications`, `enterprise_credits` and
-- `mentorship` each call Stripe directly.
--
-- The consequence is not stylistic. Stripe cannot collect Mobile Money in
-- Benin, and Mobile Money is how roughly seventy percent of adults in the
-- UEMOA hold money — against about twenty-five percent with a bank account.
-- A Beninese enterprise cannot pay for credits at all, and fixing that
-- meant editing three route files.
--
-- ─── Why a `payments` table ───────────────────────────────────────
--
-- Refunds need the provider's own identifier for the original charge, and
-- nothing stored it. `ledger_transactions.provider_reference` holds
-- `mentorship_session:<uuid>` — our identifier, not Stripe's — so
-- `refund_from_dispute` wrote entries saying the money had left the
-- provider while the provider still had it and the payer was never
-- credited. The books said refunded; the card was not.

-- ─── Where money can come from ────────────────────────────────────

CREATE TABLE collection_routes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- ISO 3166-1 alpha-2 of the *payer*. NULL is the catch-all.
    country CHAR(2) CHECK (country IS NULL OR country = UPPER(country)),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),
    method VARCHAR(20) NOT NULL
        CHECK (method IN ('card', 'mobile_money', 'bank_transfer')),
    -- Must match `CollectionProvider::name()`.
    provider VARCHAR(30) NOT NULL,
    priority SMALLINT NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (country, currency, method, provider)
);

CREATE INDEX idx_collection_routes_lookup
    ON collection_routes (currency, method, priority)
    WHERE enabled = TRUE;

INSERT INTO collection_routes (country, currency, method, provider, priority, notes) VALUES
    -- Cards, anywhere Stripe accepts them. The payer here is usually a
    -- European or North American company.
    (NULL, 'EUR', 'card', 'stripe', 100,
     'Catch-all for card payments in EUR.'),

    -- Mobile Money across the franc zone, which is the point of all this.
    -- One credential reaches MTN, Moov, Togocel, Orange and Wave.
    (NULL, 'XOF', 'mobile_money', 'fedapay', 100,
     'Catch-all for XOF Mobile Money collection: BJ, CI, SN, TG, BF, NE, GN, ML.'),

    -- A card in XOF exists but is rare, and Stripe does not settle it.
    -- Routed at FedaPay rather than left unroutable with no explanation.
    (NULL, 'XOF', 'card', 'fedapay', 100,
     'Cards denominated in XOF. Stripe does not settle this currency.');

CREATE OR REPLACE FUNCTION touch_collection_routes_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_collection_routes_updated_at
    BEFORE UPDATE ON collection_routes
    FOR EACH ROW EXECUTE FUNCTION touch_collection_routes_updated_at();

-- ─── What was collected, and how to give it back ──────────────────

CREATE TABLE payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Who paid. One of the two is set: a person buying a session, or an
    -- organisation buying credits.
    payer_id UUID REFERENCES users(id) ON DELETE SET NULL,
    payer_enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL,

    -- What it was for, so a dispute over a session finds the charge.
    subject_type VARCHAR(40) NOT NULL,
    subject_id UUID NOT NULL,

    provider VARCHAR(30) NOT NULL,
    method VARCHAR(20) NOT NULL
        CHECK (method IN ('card', 'mobile_money', 'bank_transfer')),

    -- The provider's identifier for the charge. This is what a refund
    -- needs, and it is exactly what nothing was storing.
    provider_reference VARCHAR(160),
    -- Their identifier for the checkout, before it becomes a charge.
    provider_session_id VARCHAR(160),

    amount NUMERIC(20, 4) NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),

    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN (
            'pending',    -- checkout created, the payer has not finished
            'succeeded',  -- the provider confirmed the money
            'failed',     -- the payer abandoned it or it was declined
            'refunded'    -- given back, in full
        )),

    -- Non-null once anything has been given back. Partial refunds are a
    -- number rather than a flag because they compose: two partial refunds
    -- must not be able to exceed the charge.
    refunded_amount NUMERIC(20, 4) NOT NULL DEFAULT 0
        CHECK (refunded_amount >= 0),

    idempotency_key TEXT UNIQUE,
    failure_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    succeeded_at TIMESTAMPTZ,

    -- Giving back more than was taken is not a refund, it is a gift funded
    -- by a bug.
    CONSTRAINT payments_refund_within_charge CHECK (refunded_amount <= amount),
    CONSTRAINT payments_succeeded_has_a_time CHECK (
        status <> 'succeeded' OR succeeded_at IS NOT NULL
    )
);

-- The dispute's question: what charge paid for this thing?
CREATE INDEX idx_payments_subject ON payments (subject_type, subject_id);

-- The webhook's question: which payment is the provider talking about?
CREATE UNIQUE INDEX idx_payments_provider_reference
    ON payments (provider, provider_reference)
    WHERE provider_reference IS NOT NULL;
CREATE UNIQUE INDEX idx_payments_provider_session
    ON payments (provider, provider_session_id)
    WHERE provider_session_id IS NOT NULL;

CREATE INDEX idx_payments_payer ON payments (payer_id, created_at DESC);

COMMENT ON TABLE payments IS
    'One row per attempt to collect money. Written by services::collect, '
    'confirmed by a provider webhook, and read by a refund — which needs the '
    'provider reference nothing else was storing.';

CREATE OR REPLACE FUNCTION touch_payments_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_payments_updated_at
    BEFORE UPDATE ON payments
    FOR EACH ROW EXECUTE FUNCTION touch_payments_updated_at();
