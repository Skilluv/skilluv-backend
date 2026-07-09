-- Phase 5.11 — Mentorship features.
--
-- Un utilisateur avec rôle 'mentor' peut publier son profil (bio, expertise,
-- tarif horaire), les autres talents réservent des sessions 1-on-1 payantes.
-- Payment split : 80% mentor / 20% Skilluv (via Stripe Connect Express, à
-- brancher côté ops — pour l'instant on trace le split, la libération est manuelle).

CREATE TABLE mentor_profiles (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    headline VARCHAR(200) NOT NULL,
    bio TEXT NOT NULL,
    expertise_areas TEXT[] NOT NULL DEFAULT '{}',
    languages_spoken TEXT[] NOT NULL DEFAULT '{}',
    hourly_rate_eur_cents BIGINT NOT NULL CHECK (hourly_rate_eur_cents >= 0),
    min_session_minutes INTEGER NOT NULL DEFAULT 30,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    stripe_connect_account_id VARCHAR(80),
    total_sessions INTEGER NOT NULL DEFAULT 0,
    avg_rating NUMERIC(3,2),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_mentor_profiles_expertise ON mentor_profiles USING gin (expertise_areas)
    WHERE active = TRUE;

-- Créneaux de disponibilité (récurrent hebdomadaire, MVP)
CREATE TABLE mentor_availability (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    weekday INTEGER NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    timezone VARCHAR(64) NOT NULL DEFAULT 'UTC',
    CHECK (end_time > start_time)
);

CREATE INDEX idx_mentor_availability_mentor ON mentor_availability (mentor_user_id, weekday);

-- Sessions réservées
CREATE TABLE mentorship_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mentee_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scheduled_at TIMESTAMPTZ NOT NULL,
    duration_minutes INTEGER NOT NULL CHECK (duration_minutes > 0),
    price_total_cents BIGINT NOT NULL,
    price_mentor_cents BIGINT NOT NULL,
    price_platform_cents BIGINT NOT NULL,
    currency CHAR(3) NOT NULL DEFAULT 'EUR',
    stripe_session_id VARCHAR(80),
    stripe_payment_intent_id VARCHAR(80),
    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN (
            'pending', 'paid', 'confirmed', 'completed',
            'cancelled_by_mentee', 'cancelled_by_mentor', 'no_show', 'refunded'
        )),
    meeting_url TEXT,
    mentee_notes TEXT,
    mentor_notes TEXT,
    payout_released_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (mentor_user_id <> mentee_user_id)
);

CREATE INDEX idx_mentorship_sessions_mentor ON mentorship_sessions (mentor_user_id, scheduled_at);
CREATE INDEX idx_mentorship_sessions_mentee ON mentorship_sessions (mentee_user_id, scheduled_at);
CREATE INDEX idx_mentorship_sessions_upcoming ON mentorship_sessions (scheduled_at)
    WHERE status IN ('paid', 'confirmed');

-- Reviews (uniquement après session complétée, une par session)
CREATE TABLE mentorship_reviews (
    session_id UUID PRIMARY KEY REFERENCES mentorship_sessions(id) ON DELETE CASCADE,
    reviewer_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
