-- Vague 1 sécurité auth : suivi consentement RGPD, rotation mot de passe, lockout brute-force.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS terms_accepted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS password_changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS failed_login_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS locked_until TIMESTAMPTZ;

-- Backfill terms_accepted_at pour les comptes existants avec created_at (présomption de consentement historique).
UPDATE users
SET terms_accepted_at = created_at
WHERE terms_accepted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_users_locked_until
    ON users(locked_until)
    WHERE locked_until IS NOT NULL;
