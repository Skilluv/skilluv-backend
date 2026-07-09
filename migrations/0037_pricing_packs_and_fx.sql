-- Phase 4.4 — Dynamic pricing packs + FX cache table.

CREATE TABLE pricing_packs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(30) NOT NULL UNIQUE,
    credit_count INTEGER NOT NULL CHECK (credit_count >= 0),
    price_eur_cents BIGINT NOT NULL CHECK (price_eur_cents >= 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    kind VARCHAR(20) NOT NULL DEFAULT 'credits' CHECK (kind IN ('credits', 'subscription')),
    position INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO pricing_packs (slug, credit_count, price_eur_cents, kind, position) VALUES
    ('pack_1',   1,   3900,   'credits', 10),
    ('pack_5',   5,   16900,  'credits', 20),
    ('pack_20',  20,  59900,  'credits', 30),
    ('pack_100', 100, 249900, 'credits', 40),
    ('pipeline_starter', 5,   9900,   'subscription', 100),
    ('pipeline_growth',  20,  29900,  'subscription', 110),
    ('pipeline_scale',   100, 99900,  'subscription', 120)
ON CONFLICT (slug) DO NOTHING;

-- FX rate cache. Populated by a background task pulling the ECB reference rates.
CREATE TABLE fx_rates (
    base_currency CHAR(3) NOT NULL,
    quote_currency CHAR(3) NOT NULL,
    rate NUMERIC(18,8) NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (base_currency, quote_currency)
);

-- Seed the currencies that are pegged to EUR (XOF/XAF).
INSERT INTO fx_rates (base_currency, quote_currency, rate) VALUES
    ('EUR', 'XOF', 655.957),
    ('EUR', 'XAF', 655.957)
ON CONFLICT (base_currency, quote_currency) DO NOTHING;
