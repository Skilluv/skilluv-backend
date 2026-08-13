-- Which payout provider serves which destination.
--
-- No provider reaches everyone. Stripe does not pay out to Benin — it is not
-- in its country list, extended network included. Mobile Money does not pay a
-- mentor in France. Which rail reaches whom depends on the recipient's
-- country and the currency, and it changes whenever a provider opens a market
-- or raises its prices.
--
-- That is knowledge with a short half-life, so it lives in a table rather
-- than in a `match`. Opening a country is an INSERT; switching provider is an
-- UPDATE. Neither is a deployment.
--
-- `src/services/psp.rs` holds the earlier attempt at this: a const array,
-- compiled in, covering only collection, pointing at adapters that were never
-- registered anywhere. It is superseded for payouts.

CREATE TABLE payout_routes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- ISO 3166-1 alpha-2 of the *recipient*. NULL is the catch-all, used
    -- when no country-specific rule matches.
    country CHAR(2)
        CHECK (country IS NULL OR country = UPPER(country)),
    currency CHAR(3) NOT NULL CHECK (currency IN ('EUR', 'XOF')),
    rail VARCHAR(20) NOT NULL CHECK (rail IN ('bank_account', 'mobile_money')),
    -- Must match `PayoutProvider::name()`. Also the ledger account segment,
    -- so renaming one orphans its balance history.
    provider VARCHAR(30) NOT NULL,
    -- Lower wins. Leaves room to prefer a cheaper rail while keeping the
    -- previous one as a fallback instead of deleting it.
    priority SMALLINT NOT NULL DEFAULT 100,
    -- Disable rather than delete: an outage at one provider should be a
    -- one-column change that is trivially reversible, and the history of
    -- what was routed where stays readable.
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One rule per destination and provider. Two rows for the same provider
    -- on the same route would only ever be a mistake.
    UNIQUE (country, currency, rail, provider)
);

CREATE INDEX idx_payout_routes_lookup
    ON payout_routes (currency, rail, priority)
    WHERE enabled = TRUE;

COMMENT ON TABLE payout_routes IS
    'Recipient country + currency + rail -> payout provider. Read by '
    'services::payout::routes. A provider named here that the deployment has '
    'no credentials for is skipped, not fatal.';

-- Seed reflecting coverage as of this migration. Deliberately sparse: a
-- route that has never been exercised is a promise the platform cannot keep,
-- and a payout failing at the provider is worse than one refused up front
-- with a clear message.

-- SEPA and the rest of Stripe's payout coverage.
INSERT INTO payout_routes (country, currency, rail, provider, priority, notes) VALUES
    (NULL, 'EUR', 'bank_account', 'stripe', 100,
     'Catch-all for EUR bank payouts. Stripe covers the EU, UK, US, CA, AU and, through Paystack, CI, NG, SN.');

-- West Africa: Mobile Money is the primary rail, not a fallback. Bank
-- account penetration across the UEMOA is around 25% of adults; counting
-- mobile money it is roughly 70%. Building card-first here would exclude
-- most of the intended audience.
INSERT INTO payout_routes (country, currency, rail, provider, priority, notes) VALUES
    ('BJ', 'XOF', 'mobile_money', 'mtn',    100, 'Benin — MTN. Stripe does not reach Benin at all.'),
    ('BJ', 'XOF', 'mobile_money', 'orange', 110, 'Benin — Moov/Orange as the second choice.'),
    ('CI', 'XOF', 'mobile_money', 'orange', 100, 'Cote d''Ivoire — Orange Money is dominant.'),
    ('CI', 'XOF', 'mobile_money', 'wave',   110, 'Cote d''Ivoire — Wave, cheaper, narrower coverage.'),
    ('SN', 'XOF', 'mobile_money', 'wave',   100, 'Senegal — Wave leads on price.'),
    ('SN', 'XOF', 'mobile_money', 'orange', 110, 'Senegal — Orange Money as the fallback.'),
    ('TG', 'XOF', 'mobile_money', 'mtn',    100, 'Togo.'),
    ('BF', 'XOF', 'mobile_money', 'orange', 100, 'Burkina Faso.'),
    ('ML', 'XOF', 'mobile_money', 'orange', 100, 'Mali.'),
    (NULL, 'XOF', 'mobile_money', 'orange', 200,
     'Catch-all for the XOF zone. Last resort: a route that has not been exercised in a given country is a promise we may not keep.');

CREATE OR REPLACE FUNCTION touch_payout_routes_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_payout_routes_updated_at
    BEFORE UPDATE ON payout_routes
    FOR EACH ROW EXECUTE FUNCTION touch_payout_routes_updated_at();
