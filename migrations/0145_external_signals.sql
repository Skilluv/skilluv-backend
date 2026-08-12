-- SKI-42 (Post-MVP T2-03) — controlled import of off-platform reputation.
--
-- A developer with five years of GitHub history arrives on Skilluv empty.
-- That is honest — they have proven nothing *here* — but it is also a wall
-- that stops experienced people from ever starting. External signals let
-- them show their history while keeping the distinction that makes
-- Skilluv proofs worth anything.
--
-- ## The rule this table exists to protect
--
-- External signals are DISPLAY ONLY. They must never feed
-- `user_skills.weighted_proven_count`, `user_ranks`, badge rules, or
-- talent-search scoring. "Proven on Skilluv" stays literal: importing 500
-- GitHub stars must not make anyone a Doyen.
--
-- Structurally, that guarantee comes from isolation: this table has no
-- write path into any proof table, no trigger, and nothing in the proof
-- engine reads it. The invariant is covered by an integration test that
-- adds signals and asserts rank and weighted_proven_count do not move.
--
-- ## Why `verified_at` is usually NULL
--
-- Only `github` self-verifies, by matching the URL against the login in
-- `github_connections` — an OAuth flow the user already completed, so no
-- outbound request is needed.
--
-- Blog and talk references are recorded UNVERIFIED and can be confirmed by
-- a community moderator. The alternative considered was fetching the page
-- and scraping OpenGraph tags, which would mean the backend issuing HTTP
-- requests to arbitrary user-supplied URLs — an SSRF vector aimed straight
-- at the internal network, in exchange for a signal the ticket itself
-- describes as "soft evidence, verifiable manually". Manual review is the
-- honest version of the same guarantee.

CREATE TABLE IF NOT EXISTS external_signals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(20) NOT NULL
        CHECK (provider IN ('github', 'medium', 'dev_to', 'conf_ref')),
    url TEXT NOT NULL
        CHECK (url ~ '^https://' AND length(url) BETWEEN 12 AND 500),
    -- User-supplied label; what the front end shows in the list.
    title VARCHAR(200) NOT NULL CHECK (length(title) BETWEEN 3 AND 200),
    -- NULL until confirmed. See the header for why this is the normal state
    -- for everything except github.
    verified_at TIMESTAMPTZ,
    verification_method VARCHAR(20)
        CHECK (verification_method IS NULL OR
               verification_method IN ('oauth_github', 'manual_review')),
    verified_by UUID REFERENCES users(id) ON DELETE SET NULL,
    -- Provider-specific extras (github login, publication name, talk venue).
    meta JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A verified signal must say how it was verified, and vice versa.
    CONSTRAINT external_signals_verification_coherent CHECK (
        (verified_at IS NULL AND verification_method IS NULL)
        OR (verified_at IS NOT NULL AND verification_method IS NOT NULL)
    ),

    -- The same link twice is a duplicate, not two signals.
    CONSTRAINT external_signals_unique_url UNIQUE (user_id, url)
);

-- The read path: one profile's signals, verified ones first.
CREATE INDEX IF NOT EXISTS idx_external_signals_by_user
    ON external_signals (user_id, verified_at DESC NULLS LAST, created_at DESC);

-- Moderation queue: everything still awaiting review.
CREATE INDEX IF NOT EXISTS idx_external_signals_pending
    ON external_signals (created_at ASC)
    WHERE verified_at IS NULL;
