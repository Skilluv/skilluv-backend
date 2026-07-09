-- Phase 4.17 — magic link auth (mobile-first passwordless flow).
--
-- The token itself is not stored — only a SHA-256 hash of it. This mirrors the
-- Rails / Django conventions and prevents a leaked DB dump from being usable.

CREATE TABLE magic_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL,
    token_hash BYTEA NOT NULL UNIQUE,
    intent VARCHAR(20) NOT NULL DEFAULT 'login' CHECK (intent IN ('login', 'signup')),
    requested_ip VARCHAR(45),
    consumed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_magic_links_email ON magic_links (email, created_at DESC);
CREATE INDEX idx_magic_links_expiry ON magic_links (expires_at) WHERE consumed_at IS NULL;
