-- Phase 3 — enterprise credits system (3.6 + 3.7 + 3.11).
--
-- Decisions (cf. docs/monetization-strategy.md sections 2-3):
--   - Solde au niveau enterprise, pas user. Plusieurs recruteurs partagent le pot.
--   - Balance en NUMERIC(10,2) pour autoriser les refunds 0.5 crédit.
--   - Pas de balance négative possible (CHECK + atomic decrement).
--   - Toute transaction (purchase, spend, refund, grant) tracée en credit_transactions.
--   - Promo codes : table dédiée avec usage tracking, applicables une fois par enterprise.

CREATE TABLE enterprise_credits (
    enterprise_id UUID PRIMARY KEY REFERENCES enterprises(id) ON DELETE CASCADE,
    balance NUMERIC(10,2) NOT NULL DEFAULT 0 CHECK (balance >= 0),
    total_purchased INTEGER NOT NULL DEFAULT 0,
    total_used NUMERIC(10,2) NOT NULL DEFAULT 0,
    total_refunded NUMERIC(10,2) NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE credit_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    delta NUMERIC(10,2) NOT NULL,
    balance_after NUMERIC(10,2) NOT NULL CHECK (balance_after >= 0),
    reason VARCHAR(40) NOT NULL CHECK (reason IN (
        'purchase',
        'spend_interest_request',
        'refund_refused',
        'refund_timeout',
        'refund_admin',
        'admin_grant',
        'signup_bonus',
        'promo_code',
        'subscription_grant'
    )),
    related_interest_request_id UUID REFERENCES interest_requests(id) ON DELETE SET NULL,
    related_payment_id UUID,
    related_promo_code_id UUID,
    notes TEXT,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_credit_transactions_enterprise ON credit_transactions (enterprise_id, created_at DESC);
CREATE INDEX idx_credit_transactions_payment ON credit_transactions (related_payment_id) WHERE related_payment_id IS NOT NULL;
CREATE INDEX idx_credit_transactions_interest ON credit_transactions (related_interest_request_id) WHERE related_interest_request_id IS NOT NULL;

-- Promo codes
CREATE TABLE promo_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code VARCHAR(50) NOT NULL UNIQUE,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('bonus_credits', 'percent_off')),
    value NUMERIC(10,2) NOT NULL CHECK (value > 0),
    max_uses INTEGER,  -- NULL = unlimited
    uses_count INTEGER NOT NULL DEFAULT 0,
    applicable_to VARCHAR(20) NOT NULL DEFAULT 'purchase' CHECK (applicable_to IN ('purchase', 'subscription')),
    valid_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_until TIMESTAMPTZ,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_promo_codes_code ON promo_codes (code);

CREATE TABLE promo_code_redemptions (
    promo_code_id UUID NOT NULL REFERENCES promo_codes(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    redeemed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (promo_code_id, enterprise_id)
);

-- Track Stripe webhook events for idempotency (Phase 3.8).
CREATE TABLE stripe_webhook_events (
    event_id VARCHAR(100) PRIMARY KEY,  -- Stripe event ID (evt_*)
    event_type VARCHAR(80) NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);
