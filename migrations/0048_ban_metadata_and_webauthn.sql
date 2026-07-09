-- Vague 3 : métadonnées de ban administrateur + credentials WebAuthn (passkeys).

-- ─── Ban metadata ─────────────────────────────────────────────────
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS ban_reason TEXT,
    ADD COLUMN IF NOT EXISTS banned_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS banned_by UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_users_banned
    ON users(banned_at)
    WHERE is_banned = TRUE;

-- ─── WebAuthn credentials (passkeys) ──────────────────────────────
-- Une ligne par credential (Yubikey, Touch ID, Windows Hello, passkey plateforme, etc.).
-- `credential` sérialise l'objet Passkey de webauthn-rs (JSON compact).
CREATE TABLE webauthn_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- ID du credential côté navigateur (base64url) — clé unique de lookup à la connexion.
    credential_id BYTEA NOT NULL UNIQUE,
    credential JSONB NOT NULL,
    label TEXT,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webauthn_user ON webauthn_credentials(user_id);
