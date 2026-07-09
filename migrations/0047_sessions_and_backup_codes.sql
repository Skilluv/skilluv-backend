-- Vague 2 : gestion des sessions/devices avec rotation + détection de réutilisation, et codes de secours TOTP.

-- ─── user_sessions ─────────────────────────────────────────────────
-- Une ligne par device connecté. Le refresh token n'est pas stocké en clair : on garde
-- son SHA-256 (`refresh_hash`) et celui de la génération précédente (`previous_hash`) pour
-- détecter une réutilisation post-rotation (indicateur d'un token volé).
CREATE TABLE user_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    refresh_hash BYTEA NOT NULL,
    previous_hash BYTEA,
    ip TEXT,
    user_agent TEXT,
    device_label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_user_sessions_user_active
    ON user_sessions(user_id)
    WHERE revoked_at IS NULL;

CREATE INDEX idx_user_sessions_last_used
    ON user_sessions(last_used_at)
    WHERE revoked_at IS NULL;

-- ─── totp_backup_codes ─────────────────────────────────────────────
-- Codes de secours à usage unique générés au moment de l'activation TOTP.
-- On stocke uniquement l'Argon2 du code (comme un mot de passe court).
CREATE TABLE totp_backup_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_totp_backup_codes_user_unused
    ON totp_backup_codes(user_id)
    WHERE used_at IS NULL;

-- ─── pending_email_change ──────────────────────────────────────────
-- Demande de changement d'email en attente : le token est envoyé au NOUVEL email,
-- l'ancien reçoit une notification de sécurité (dans le code applicatif).
CREATE TABLE pending_email_change (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    new_email TEXT NOT NULL,
    token_hash BYTEA NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pending_email_change_expires ON pending_email_change(expires_at);
