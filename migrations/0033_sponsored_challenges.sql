-- Phase 3.12 — sponsored challenges.

ALTER TABLE challenges ADD COLUMN sponsor_enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL;
ALTER TABLE challenges ADD COLUMN sponsor_logo_url VARCHAR(500);
ALTER TABLE challenges ADD COLUMN sponsor_blurb TEXT;
ALTER TABLE challenges ADD COLUMN sponsor_visible_from TIMESTAMPTZ;
ALTER TABLE challenges ADD COLUMN sponsor_visible_until TIMESTAMPTZ;

CREATE INDEX idx_challenges_sponsor ON challenges (sponsor_enterprise_id, sponsor_visible_until)
    WHERE sponsor_enterprise_id IS NOT NULL;

-- Sponsor → privileged access to submissions on their sponsored challenges.
-- The view is built dynamically ; this table only tracks who has been granted access.
CREATE TABLE sponsor_challenge_access (
    challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    free_contact_until TIMESTAMPTZ NOT NULL,  -- typically end_of_sponsorship + 30 days
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (challenge_id, enterprise_id)
);

CREATE INDEX idx_sponsor_access_enterprise ON sponsor_challenge_access (enterprise_id, free_contact_until);

-- Sponsorship requests (workflow draft → admin review → live)
CREATE TABLE sponsored_challenge_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    requested_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    proposed_title VARCHAR(200) NOT NULL,
    brief TEXT NOT NULL,
    skill_domain VARCHAR(20) NOT NULL CHECK (skill_domain IN ('code', 'design', 'game', 'security')),
    difficulty INT2 NOT NULL CHECK (difficulty BETWEEN 1 AND 5),
    duration_days INTEGER NOT NULL CHECK (duration_days BETWEEN 1 AND 90),
    budget_eur_cents BIGINT NOT NULL CHECK (budget_eur_cents >= 0),
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'negotiating', 'approved', 'live', 'rejected', 'completed', 'cancelled')),
    challenge_id UUID REFERENCES challenges(id) ON DELETE SET NULL,
    admin_notes TEXT,
    decided_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sponsored_requests_enterprise ON sponsored_challenge_requests (enterprise_id, created_at DESC);
CREATE INDEX idx_sponsored_requests_status ON sponsored_challenge_requests (status, created_at DESC);
