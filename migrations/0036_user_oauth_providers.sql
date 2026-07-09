-- Phase 3.1 + 3.2 — unified OAuth providers table.
--
-- Existing github_connections (Sprint 5) stays for backward compat and holds the
-- encrypted access token. This new table is the canonical registry of "which
-- OAuth identities are linked to which Skilluv user", regardless of provider.

CREATE TABLE user_oauth_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(20) NOT NULL CHECK (provider IN ('github', 'google', 'linkedin')),
    provider_user_id VARCHAR(120) NOT NULL,  -- external provider's user id (as string, some are int)
    email VARCHAR(255),
    display_name VARCHAR(120),
    avatar_url VARCHAR(500),
    linked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (provider, provider_user_id),
    UNIQUE (user_id, provider)
);

CREATE INDEX idx_user_oauth_providers_user ON user_oauth_providers (user_id);
CREATE INDEX idx_user_oauth_providers_email ON user_oauth_providers (email) WHERE email IS NOT NULL;

-- Backfill from existing github_connections rows.
INSERT INTO user_oauth_providers (user_id, provider, provider_user_id, email, display_name)
SELECT gc.user_id, 'github', gc.github_user_id::text, u.email, u.display_name
FROM github_connections gc
JOIN users u ON u.id = gc.user_id
ON CONFLICT (provider, provider_user_id) DO NOTHING;
