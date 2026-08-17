-- Where somebody's code already lives.
--
-- Most of a developer's work exists before they ever open Skilluv, and on
-- platforms Skilluv does not own. Ignoring that would mean asking people to
-- rebuild ten years of history inside a product they just discovered, which
-- nobody does.
--
-- ## One table, several platforms
--
-- GitHub is the common case and not the only one: GitLab, Codeberg and
-- SourceHut exist precisely because some people will not put their work on
-- GitHub, and a platform about proving what you have done cannot then require
-- one company's account. Package registries sit here too — a published crate
-- is portfolio, not a repository.
--
-- ## The distinction that matters: claimed against verified
--
-- Connecting GitHub goes through OAuth, so the account is proved. Typing a
-- Codeberg username proves nothing — anybody can type anybody's. Both are
-- worth storing and only one is worth counting, so the difference is a column
-- and not a comment. Nothing unverified reaches the craft score.

CREATE TABLE user_code_portfolios (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform VARCHAR(30) NOT NULL CHECK (platform IN (
        -- Forges.
        'github', 'gitlab', 'codeberg', 'sourcehut',
        -- Registries. Same list the package statistics already recognise, so
        -- one URL parser serves both.
        'crates_io', 'npm', 'pypi', 'go_modules', 'rubygems',
        'maven_central', 'nuget', 'packagist', 'hex', 'homebrew'
    )),
    -- The account name on that platform.
    handle VARCHAR(120) NOT NULL,
    profile_url TEXT NOT NULL CHECK (profile_url ~ '^https://'),

    -- Set only when ownership was proved — today that means an OAuth flow.
    -- NULL means somebody typed a name, which is worth showing and not worth
    -- counting.
    verified_at TIMESTAMPTZ,
    -- How it was proved. Kept because "verified" will mean something
    -- different when a second method exists, and a row from today should
    -- still say which one it went through.
    verification_method VARCHAR(30),

    -- Headline figures, pulled out of the payload so they can be summed and
    -- sorted without opening JSON. NULL means not measured, never zero: a
    -- forge that publishes no star count must not read as a repository
    -- nobody starred.
    repos_count INTEGER CHECK (repos_count IS NULL OR repos_count >= 0),
    stars_received INTEGER CHECK (stars_received IS NULL OR stars_received >= 0),
    followers_count INTEGER CHECK (followers_count IS NULL OR followers_count >= 0),
    contributions_last_year INTEGER
        CHECK (contributions_last_year IS NULL OR contributions_last_year >= 0),
    packages_count INTEGER CHECK (packages_count IS NULL OR packages_count >= 0),
    downloads_total BIGINT CHECK (downloads_total IS NULL OR downloads_total >= 0),

    -- Everything else the platform answered, as it answered it. Shapes differ
    -- per platform and change under us; columns for all of them would be a
    -- migration every time somebody else ships a release.
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,

    last_synced_at TIMESTAMPTZ,
    -- Kept rather than logged and forgotten: a portfolio showing figures from
    -- three weeks ago should be able to say why.
    last_error TEXT,
    sync_enabled BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (user_id, platform, handle),

    CONSTRAINT metadata_is_an_object CHECK (jsonb_typeof(metadata) = 'object'),

    -- A verification with no method is one nobody can audit later.
    CONSTRAINT verification_names_its_method CHECK (
        verified_at IS NULL OR verification_method IS NOT NULL
    )
);

COMMENT ON TABLE user_code_portfolios IS
    'Accounts on other platforms. `verified_at` separates what was proved '
    'through OAuth from what somebody typed; only the first is countable.';

COMMENT ON COLUMN user_code_portfolios.verified_at IS
    'Ownership proved. NULL means claimed — worth showing, not worth counting.';

CREATE INDEX idx_code_portfolios_user ON user_code_portfolios (user_id, platform);
CREATE INDEX idx_code_portfolios_stale
    ON user_code_portfolios (last_synced_at NULLS FIRST)
    WHERE sync_enabled = TRUE;
CREATE INDEX idx_code_portfolios_verified
    ON user_code_portfolios (platform, handle)
    WHERE verified_at IS NOT NULL;

CREATE TRIGGER trg_code_portfolios_updated_at
    BEFORE UPDATE ON user_code_portfolios
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The same account cannot be verified by two people
-- ═══════════════════════════════════════════════════════════════════
--
-- Two people may both claim to be `torvalds` on Codeberg and neither claim
-- means anything. But once one of them has proved it through OAuth, the
-- second cannot: it is the same account, and the platform would be publishing
-- a contradiction.

CREATE UNIQUE INDEX uniq_verified_account_per_platform
    ON user_code_portfolios (platform, lower(handle))
    WHERE verified_at IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Bringing the existing GitHub connections across
-- ═══════════════════════════════════════════════════════════════════
--
-- `github_connections` already holds proved accounts: it is written by the
-- OAuth callback and nothing else. Those rows are exactly what this table
-- calls verified, and leaving them out would mean everybody who connected
-- GitHub before today appears to have no portfolio.

INSERT INTO user_code_portfolios
    (user_id, platform, handle, profile_url, verified_at, verification_method,
     last_synced_at)
SELECT c.user_id,
       'github',
       c.github_login,
       'https://github.com/' || c.github_login,
       c.created_at,
       'oauth',
       c.last_synced_at
  FROM github_connections c
ON CONFLICT (user_id, platform, handle) DO NOTHING;
