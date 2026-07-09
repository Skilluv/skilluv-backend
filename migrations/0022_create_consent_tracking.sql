-- Consent tracking (Phase 1.9 + 1.10)

-- Each row = one acceptance event. Audit log for legal compliance.
CREATE TABLE consent_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    version INTEGER NOT NULL,
    analytics BOOLEAN NOT NULL DEFAULT FALSE,
    marketing BOOLEAN NOT NULL DEFAULT FALSE,
    essential BOOLEAN NOT NULL DEFAULT TRUE,  -- toujours TRUE, gardé pour audit
    ip VARCHAR(45),
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_consent_log_user ON consent_log (user_id, created_at DESC);

-- Latest version accepted by the user (cached for quick checks).
ALTER TABLE users ADD COLUMN consent_version_accepted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN consent_analytics BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN consent_marketing BOOLEAN NOT NULL DEFAULT FALSE;
