-- SKI-45 (Post-MVP T3-02) — reverse marketplace: talents offer their time.
--
-- The mentor marketplace runs one way: a company or a junior searches, a
-- mentor is found. This inverts it — an Artisan publishes "2h/week of
-- senior Rust pair-programming" and the people who need it come to them.
--
-- Deliberately lighter than `mentor_profiles` + `mentorship_sessions`,
-- which carry a booking calendar, Stripe checkout, payout release and a
-- review cycle. An offer is a standing statement of availability: no
-- calendar, no escrow, no booking state machine. Someone who wants the
-- formal product still has it.
--
-- ## Rank gate
--
-- Publishing requires Artisan or above. That is enforced in the service
-- layer, not here: rank is derived and mutable, so a CHECK against a
-- snapshot would either go stale or need a trigger on every promotion.
-- The listing endpoint re-checks rank at read time, so an offer published
-- by someone whose rank is later overridden downward stops being listed.
--
-- ## Paid offers
--
-- `price_cents_per_hour IS NULL` means free. A priced offer additionally
-- requires a verified Stripe Connect account on the talent's wallet
-- (`talent_wallets.stripe_kyc_status = 'verified'`), checked at write
-- time: advertising a price we could never pay out would be a promise the
-- platform cannot keep.

CREATE TABLE IF NOT EXISTS talent_offers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    offer_type VARCHAR(20) NOT NULL
        CHECK (offer_type IN (
            'pair_programming',
            'code_review',
            'whiteboard',
            'mock_interview',
            'career_advice'
        )),
    -- Optional: `career_advice` and `mock_interview` are frequently not
    -- about one specific skill.
    skill_id UUID REFERENCES skill_nodes(id) ON DELETE CASCADE,
    -- Hours per week on offer. Capped at 20: past that this is a job, and
    -- the platform would be brokering employment without any of the
    -- protections that implies.
    availability_hours SMALLINT NOT NULL DEFAULT 2
        CHECK (availability_hours BETWEEN 1 AND 20),
    -- NULL = free. Non-NULL requires a verified payout account.
    price_cents_per_hour BIGINT
        CHECK (price_cents_per_hour IS NULL OR price_cents_per_hour > 0),
    description TEXT NOT NULL DEFAULT '' CHECK (length(description) <= 2000),
    -- Soft toggle so a talent can pause during a busy month without
    -- losing the wording they wrote.
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One offer per (talent, type, skill). Two identical listings are noise in
-- the browse view, not two opportunities.
--
-- A plain UNIQUE constraint does NOT work here: `skill_id` is nullable and
-- SQL treats NULLs as distinct, so `(user, 'code_review', NULL)` would
-- never collide with itself and a talent could stack unlimited
-- skill-less offers of the same type. COALESCE onto a sentinel UUID makes
-- the null case comparable.
CREATE UNIQUE INDEX IF NOT EXISTS idx_talent_offers_unique_kind
    ON talent_offers (
        user_id,
        offer_type,
        COALESCE(skill_id, '00000000-0000-0000-0000-000000000000'::UUID)
    );

-- Browse: live offers of a kind, newest first.
CREATE INDEX IF NOT EXISTS idx_talent_offers_browse
    ON talent_offers (offer_type, created_at DESC)
    WHERE active = TRUE;

-- Browse by skill.
CREATE INDEX IF NOT EXISTS idx_talent_offers_by_skill
    ON talent_offers (skill_id, created_at DESC)
    WHERE active = TRUE AND skill_id IS NOT NULL;

-- "My offers".
CREATE INDEX IF NOT EXISTS idx_talent_offers_by_user
    ON talent_offers (user_id, created_at DESC);
