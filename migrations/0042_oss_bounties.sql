-- Phase 5.6 — Bounties OSS.

-- Étendre les reasons autorisés dans credit_transactions pour couvrir les
-- flux bounties + certifications + mentorship (Phase 5).
ALTER TABLE credit_transactions DROP CONSTRAINT credit_transactions_reason_check;
ALTER TABLE credit_transactions ADD CONSTRAINT credit_transactions_reason_check
    CHECK (reason IN (
        'purchase',
        'spend_interest_request',
        'refund_refused',
        'refund_timeout',
        'refund_admin',
        'admin_grant',
        'signup_bonus',
        'promo_code',
        'subscription_grant',
        -- Phase 5 additions
        'spend_bounty_escrow',
        'spend_bounty_payout',
        'refund_bounty_cancelled',
        'spend_certification',
        'spend_mentorship_session',
        'refund_mentorship_cancelled'
    ));

--
-- Une entreprise poste une bounty sur une issue GitHub (URL) avec un pool en
-- crédits Skilluv. Les talents "claim" l'issue, soumettent une PR ; le webhook
-- GitHub `pull_request.closed` avec `merged=true` déclenche le payout auto.

CREATE TABLE oss_bounties (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    posted_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE SET NULL,

    -- Issue GitHub cible
    repo_owner VARCHAR(120) NOT NULL,
    repo_name VARCHAR(200) NOT NULL,
    issue_number INTEGER NOT NULL,
    issue_url TEXT NOT NULL,
    title VARCHAR(300) NOT NULL,
    description TEXT NOT NULL,

    -- Récompense
    reward_credits NUMERIC(10,2) NOT NULL CHECK (reward_credits > 0),
    fragments_bonus INTEGER NOT NULL DEFAULT 100 CHECK (fragments_bonus >= 0),

    -- Filtres et méta
    required_skills TEXT[] NOT NULL DEFAULT '{}',
    difficulty INTEGER NOT NULL DEFAULT 3 CHECK (difficulty BETWEEN 1 AND 5),
    tags TEXT[] NOT NULL DEFAULT '{}',

    -- État
    status VARCHAR(20) NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'claimed', 'in_review', 'paid', 'cancelled', 'expired')),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (repo_owner, repo_name, issue_number)
);

CREATE INDEX idx_oss_bounties_status ON oss_bounties (status, created_at DESC);
CREATE INDEX idx_oss_bounties_enterprise ON oss_bounties (enterprise_id, status);
CREATE INDEX idx_oss_bounties_tags ON oss_bounties USING gin (tags);
CREATE INDEX idx_oss_bounties_skills ON oss_bounties USING gin (required_skills);

-- Claims : un talent revendique la bounty (soft-lock, expiration 7j sans PR)
CREATE TABLE oss_bounty_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bounty_id UUID NOT NULL REFERENCES oss_bounties(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pull_request_url TEXT,
    pull_request_number INTEGER,
    status VARCHAR(20) NOT NULL DEFAULT 'claimed'
        CHECK (status IN ('claimed', 'pr_submitted', 'merged', 'rejected', 'expired')),
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    pr_submitted_at TIMESTAMPTZ,
    merged_at TIMESTAMPTZ,
    payout_credit_tx_id UUID,
    UNIQUE (bounty_id, user_id)
);

CREATE INDEX idx_bounty_claims_bounty ON oss_bounty_claims (bounty_id, status);
CREATE INDEX idx_bounty_claims_user ON oss_bounty_claims (user_id, status);

-- Événements webhook GitHub reçus (idempotence par delivery_id)
CREATE TABLE github_webhook_events (
    delivery_id VARCHAR(80) PRIMARY KEY,
    event_type VARCHAR(40) NOT NULL,
    payload JSONB NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
