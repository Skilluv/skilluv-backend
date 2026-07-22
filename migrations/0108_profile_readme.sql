-- Content strategy foundation — living profile README (2026-07-21 decision).
--
-- Rationale: Skilluv permits each user to personalize their profile with an
-- extended README, similar to GitHub `{username}/{username}` profile READMEs.
-- Three modes are supported: bio_short (default), native (edited on Skilluv),
-- github_sync (mirrored from `{username}/{username}` repo, refreshed daily).
--
-- Editorial choices (see content strategy doc §9 "Profil vivant"):
--   - Markdown source with safelisted rendering (no raw HTML, no scripts, no
--     unlisted iframes) — sanitization pipeline enforced application-side.
--   - Quota: 20 KB markdown source, 10 external images, 5 GIFs, 3 iframes.
--   - Moderation via capability community_moderator (already in P25).
--   - Automatic flag on keyword matches from banned_content_patterns table
--     (this migration creates that table).
--   - Auto-generated template on Bonjour Skilluv completion (application-side).
--
-- Related migrations:
--   - 0044 (Phase 5.11): base users table, we extend it here.
--   - 0068: attestations (linked to badges shown in README template).

-- ═══════════════════════════════════════════════════════════════════
-- 1. Extend users with profile README columns
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE users
    ADD COLUMN profile_readme_source VARCHAR(20) NOT NULL DEFAULT 'bio_short'
        CHECK (profile_readme_source IN ('bio_short', 'native', 'github_sync'));

ALTER TABLE users
    ADD COLUMN profile_readme_markdown TEXT
        CHECK (profile_readme_markdown IS NULL OR length(profile_readme_markdown) <= 20480);

ALTER TABLE users
    ADD COLUMN profile_readme_sync_url TEXT
        CHECK (profile_readme_sync_url IS NULL
               OR profile_readme_sync_url ~ '^https://github\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+/blob/[^/]+/.+$'
               OR profile_readme_sync_url ~ '^https://raw\.githubusercontent\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+/[^/]+/.+$');

ALTER TABLE users
    ADD COLUMN profile_readme_synced_at TIMESTAMPTZ;

ALTER TABLE users
    ADD COLUMN profile_readme_hidden_at TIMESTAMPTZ;

ALTER TABLE users
    ADD COLUMN profile_readme_hidden_reason TEXT;

ALTER TABLE users
    ADD COLUMN profile_readme_hidden_by UUID REFERENCES users(id) ON DELETE SET NULL;

-- Coherence constraints
ALTER TABLE users
    ADD CONSTRAINT users_profile_readme_native_has_markdown
    CHECK (
        profile_readme_source <> 'native'
        OR profile_readme_markdown IS NOT NULL
    );

ALTER TABLE users
    ADD CONSTRAINT users_profile_readme_github_has_url
    CHECK (
        profile_readme_source <> 'github_sync'
        OR profile_readme_sync_url IS NOT NULL
    );

ALTER TABLE users
    ADD CONSTRAINT users_profile_readme_hidden_coherent
    CHECK (
        (profile_readme_hidden_at IS NULL AND profile_readme_hidden_reason IS NULL AND profile_readme_hidden_by IS NULL)
        OR (profile_readme_hidden_at IS NOT NULL AND profile_readme_hidden_reason IS NOT NULL AND profile_readme_hidden_by IS NOT NULL)
    );

-- For scheduled sync cron (github_sync mode)
CREATE INDEX idx_users_profile_readme_sync
    ON users (profile_readme_synced_at NULLS FIRST)
    WHERE profile_readme_source = 'github_sync' AND profile_readme_hidden_at IS NULL;

COMMENT ON COLUMN users.profile_readme_source IS
    'Which mode the user chose for their profile README: bio_short (default, no extended README), native (edited in Skilluv UI, markdown stored in profile_readme_markdown), github_sync (mirrored from a GitHub URL, refreshed daily via cron).';

COMMENT ON COLUMN users.profile_readme_markdown IS
    'Markdown source of the extended profile README. NULL if source=bio_short. Max 20 KB enforced by CHECK. Sanitization pipeline enforced application-side before rendering.';

COMMENT ON COLUMN users.profile_readme_sync_url IS
    'GitHub URL to fetch the README from (in github_sync mode). Validated against GitHub URL patterns. NULL if source != github_sync.';

COMMENT ON COLUMN users.profile_readme_hidden_at IS
    'Set by a moderator (capability community_moderator or admin) when the profile README is hidden for moderation reasons. Preserved for audit trail — never deleted, only masked in rendering.';

-- ═══════════════════════════════════════════════════════════════════
-- 2. Banned content patterns for auto-flagging
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE banned_content_patterns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The pattern itself (regex or literal, depending on kind)
    pattern TEXT NOT NULL,
    pattern_kind VARCHAR(20) NOT NULL DEFAULT 'literal'
        CHECK (pattern_kind IN ('literal', 'regex')),

    -- Where to apply the pattern
    scope VARCHAR(30) NOT NULL DEFAULT 'profile_readme'
        CHECK (scope IN ('profile_readme', 'forum_post', 'dm', 'any')),

    -- Reason category (helps route the flag to the right reviewer)
    category VARCHAR(30) NOT NULL
        CHECK (category IN (
            'spam',
            'harassment',
            'discrimination',
            'illegal_content',
            'crypto_scam',
            'phishing',
            'plagiarism_signal',
            'other'
        )),

    -- Auto-action when matched
    auto_action VARCHAR(20) NOT NULL DEFAULT 'flag'
        CHECK (auto_action IN (
            'flag',            -- create moderation task, no immediate hide
            'hide_pending',    -- hide content until moderator reviews
            'notify_admin'     -- alert admin immediately (never used alone; combined with above)
        )),

    -- Metadata
    active BOOLEAN NOT NULL DEFAULT TRUE,
    added_by UUID REFERENCES users(id) ON DELETE SET NULL,
    added_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_banned_content_patterns_active_scope
    ON banned_content_patterns (scope, category)
    WHERE active = TRUE;

COMMENT ON TABLE banned_content_patterns IS
    'Keywords / regex patterns automatically flagged in user content (profile READMEs, forum posts, DMs). Reviewed by community_moderator capability. Only additive: patterns are deactivated (active=FALSE), never deleted, for audit history.';

-- ═══════════════════════════════════════════════════════════════════
-- 3. Seed a few initial patterns (spam/scam basics, non-exhaustive)
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO banned_content_patterns (pattern, pattern_kind, scope, category, auto_action, added_reason) VALUES
-- Crypto/MLM scams (charte §5.8)
('mlm|multi.?level.?marketing|pyramid.?scheme|matrix.?cycler', 'regex', 'any', 'crypto_scam', 'hide_pending', 'Charte §5.8 — pyramid schemes and MLM banned (Tier 3).'),
('pump.?and.?dump|to.?the.?moon|hodl.?bag|shitcoin', 'regex', 'any', 'crypto_scam', 'flag', 'Aggressive crypto trading vocabulary — Tier 2 flag.'),

-- Phishing signals
('nigerian.?prince|inheritance.?claim', 'regex', 'any', 'phishing', 'hide_pending', 'Classic phishing pattern.'),
('urgent.?transfer.?fund|send.?bitcoin.?address', 'regex', 'any', 'phishing', 'hide_pending', 'Financial phishing pattern.'),

-- Harassment starters (partial list, extended by moderation team over time)
('kill.?yourself|kys', 'regex', 'any', 'harassment', 'hide_pending', 'Direct harassment — Tier 3.');

COMMENT ON COLUMN banned_content_patterns.pattern IS
    'The pattern to match. Use pattern_kind=literal for exact strings, pattern_kind=regex for POSIX regular expressions. Case-insensitive matching enforced application-side.';
