-- Email infrastructure (Phase 1.7)

-- User email preferences. One row per user. Created lazily on first read/update.
CREATE TABLE user_email_preferences (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    digest_weekly BOOLEAN NOT NULL DEFAULT TRUE,
    streak_reminder BOOLEAN NOT NULL DEFAULT TRUE,
    marketing BOOLEAN NOT NULL DEFAULT FALSE,
    -- Transactional emails (verify, password reset, 2FA) are NOT opt-out — security/legal.
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Hard switch: if true, send no email at all (set after hard bounce or RGPD request).
ALTER TABLE users ADD COLUMN email_disabled BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN email_bounce_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN email_last_bounce_at TIMESTAMPTZ;

-- Log every email sent. Used for debug, compliance, and tracking opens via pixel later.
CREATE TABLE email_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(40) NOT NULL,  -- 'verify' | 'password_reset' | '2fa' | 'digest_weekly' | 'streak_reminder' | 'marketing_*' | ...
    subject TEXT,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    delivered_at TIMESTAMPTZ,
    opened_at TIMESTAMPTZ,
    bounced_at TIMESTAMPTZ,
    bounce_reason TEXT,
    provider_message_id VARCHAR(255)  -- Brevo / SendGrid / etc message ID for correlation
);

CREATE INDEX idx_email_log_user ON email_log (user_id, sent_at DESC);
CREATE INDEX idx_email_log_kind ON email_log (kind, sent_at DESC);
CREATE INDEX idx_email_log_provider_msg ON email_log (provider_message_id) WHERE provider_message_id IS NOT NULL;
