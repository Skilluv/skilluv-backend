-- Phase 5.10 — Certifications payantes.
--
-- Une certification = une suite de N challenges d'un domaine à passer dans un
-- temps limité. Prix en EUR (Stripe direct, pas de crédits enterprise).
-- Réussite → diplôme PDF avec URL de vérification publique.

CREATE TABLE certifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE,
    title VARCHAR(200) NOT NULL,
    description TEXT NOT NULL,
    skill_domain VARCHAR(30) NOT NULL,
    level VARCHAR(20) NOT NULL DEFAULT 'intermediate'
        CHECK (level IN ('foundation', 'intermediate', 'advanced', 'expert')),
    price_eur_cents BIGINT NOT NULL CHECK (price_eur_cents >= 0),
    duration_minutes INTEGER NOT NULL CHECK (duration_minutes > 0),
    passing_score INTEGER NOT NULL DEFAULT 70 CHECK (passing_score BETWEEN 0 AND 100),
    challenge_ids UUID[] NOT NULL DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    validity_months INTEGER NOT NULL DEFAULT 24,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_certifications_domain ON certifications (skill_domain, active);

-- Tentative de certification (une par utilisateur × certif, réessayable après 7j)
CREATE TABLE certification_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    certification_id UUID NOT NULL REFERENCES certifications(id) ON DELETE CASCADE,
    stripe_payment_intent_id VARCHAR(80),
    stripe_session_id VARCHAR(80),
    amount_paid_cents BIGINT NOT NULL DEFAULT 0,
    currency CHAR(3) NOT NULL DEFAULT 'EUR',
    status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'paid', 'started', 'passed', 'failed', 'expired', 'refunded')),
    score INTEGER,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    diploma_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cert_attempts_user ON certification_attempts (user_id, status);
CREATE INDEX idx_cert_attempts_cert ON certification_attempts (certification_id, status);

-- Diplôme émis (URL publique de vérification via code court)
CREATE TABLE certification_diplomas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attempt_id UUID NOT NULL REFERENCES certification_attempts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    certification_id UUID NOT NULL REFERENCES certifications(id) ON DELETE CASCADE,
    -- Code court à 8 caractères base32 pour la vérif publique
    verification_code VARCHAR(12) NOT NULL UNIQUE,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoke_reason TEXT,
    pdf_storage_key VARCHAR(500)
);

CREATE INDEX idx_diplomas_user ON certification_diplomas (user_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_diplomas_verification ON certification_diplomas (verification_code);
