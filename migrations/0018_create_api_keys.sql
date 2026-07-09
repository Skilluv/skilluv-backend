-- API keys for public API access
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    key_prefix VARCHAR(12) NOT NULL,
    key_hash VARCHAR(255) NOT NULL,
    permissions JSONB NOT NULL DEFAULT '["read:profile"]',
    last_used_at TIMESTAMPTZ,
    request_count BIGINT NOT NULL DEFAULT 0,
    rate_limit INTEGER NOT NULL DEFAULT 1000,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_keys_user ON api_keys (user_id);
CREATE INDEX idx_api_keys_prefix ON api_keys (key_prefix);
