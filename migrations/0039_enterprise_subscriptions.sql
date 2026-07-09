-- Phase 4.6 — recurring subscription plans (Pipeline Starter / Growth / Scale).

CREATE TABLE enterprise_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    plan_slug VARCHAR(30) NOT NULL REFERENCES pricing_packs(slug),
    stripe_customer_id VARCHAR(80),
    stripe_subscription_id VARCHAR(80) UNIQUE,
    status VARCHAR(20) NOT NULL DEFAULT 'active'
        CHECK (status IN ('trialing', 'active', 'past_due', 'canceled', 'unpaid')),
    current_period_start TIMESTAMPTZ,
    current_period_end TIMESTAMPTZ,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT FALSE,
    -- Number of "included" credits granted at each renewal.
    monthly_credit_grant INTEGER NOT NULL DEFAULT 0,
    -- The last period we already granted the monthly credits for (avoids double-grant on webhook retry).
    last_grant_period_start TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_enterprise_subs_active ON enterprise_subscriptions (enterprise_id)
    WHERE status IN ('trialing', 'active', 'past_due');
CREATE INDEX idx_enterprise_subs_stripe ON enterprise_subscriptions (stripe_subscription_id)
    WHERE stripe_subscription_id IS NOT NULL;
