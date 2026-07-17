-- FE-M9 — Table user_mutes pour modération forum (mute temporaire).
-- Migration 0103.
--
-- Rationale :
--   Un forum_moderator peut mute un user (empêche création posts + comments)
--   pour une durée donnée (24h par défaut). Différent de is_banned (permanent
--   admin-only). Le mute peut être révoqué manuellement ou expire seul.
--
--   Query typique côté /forum/posts POST : refuse si
--   `EXISTS(mute WHERE user_id = X AND expires_at > NOW() AND lifted_at IS NULL)`.

CREATE TABLE user_mutes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    muted_by UUID NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    reason TEXT NOT NULL CHECK (length(reason) >= 8),
    scope VARCHAR(20) NOT NULL DEFAULT 'forum'
        CHECK (scope IN ('forum', 'community', 'all')),
    expires_at TIMESTAMPTZ NOT NULL,
    lifted_at TIMESTAMPTZ,
    lifted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_user_mutes_active_by_user
    ON user_mutes (user_id, expires_at)
    WHERE lifted_at IS NULL;

CREATE INDEX idx_user_mutes_by_mod
    ON user_mutes (muted_by, created_at DESC);

COMMENT ON TABLE user_mutes IS
    'FE-M9 — mutes temporaires appliqués par forum_moderator/community_moderator.';
