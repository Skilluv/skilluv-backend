-- Content strategy foundation — Hello Wall tracking (2026-07-21 decision).
--
-- Rationale: the Hello Wall (see repo skilluv-community/hello-wall) archives
-- every user's first commit (their HELLO.md from Bonjour Skilluv onboarding).
-- The GitHub bot workflow that mirrors HELLO.md → entries/{username}.md lives
-- on GitHub side. This migration is minimal DB-side: we only need to track
-- whether the mirroring succeeded and the URL to the archived entry, so users
-- can reference it in their profile.
--
-- The workflow itself (backend cron + gRPC to GitHub) is developed in a
-- follow-up code change (not this migration).
--
-- Right to erasure: if a user requests deletion, the entry is removed from
-- the GitHub repo AND hello_wall_entries.deleted_at is set (audit trail
-- preserved).

CREATE TABLE hello_wall_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    -- The HELLO.md content as archived (snapshot at mirror time)
    hello_markdown TEXT NOT NULL,
    hello_hash CHAR(64) NOT NULL,  -- SHA-256 for immutability proof

    -- Where it lives on GitHub (URL to entries/{username}.md)
    github_entry_url TEXT NOT NULL
        CHECK (github_entry_url ~ '^https://github\.com/skilluv-community/hello-wall/blob/main/entries/.+\.md$'),

    -- Source PR that triggered the archiving
    source_pr_url TEXT NOT NULL
        CHECK (source_pr_url ~ '^https://github\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+/pull/\d+$'),
    source_starter_repo VARCHAR(60) NOT NULL,
    -- e.g. "starter-fullstack-rust" — which template the user forked

    -- Timestamps
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    deletion_reason TEXT,

    CHECK (
        (deleted_at IS NULL AND deletion_reason IS NULL)
        OR (deleted_at IS NOT NULL AND deletion_reason IS NOT NULL)
    )
);

-- For fetching a user's hello wall entry from their profile
CREATE INDEX idx_hello_wall_entries_user
    ON hello_wall_entries (user_id)
    WHERE deleted_at IS NULL;

-- For the public timeline query (skilluv.io/hello-wall)
CREATE INDEX idx_hello_wall_entries_timeline
    ON hello_wall_entries (archived_at DESC)
    WHERE deleted_at IS NULL;

COMMENT ON TABLE hello_wall_entries IS
    'Tracks each user''s Hello Wall entry (their HELLO.md archived on skilluv-community/hello-wall repo). One entry per user (UNIQUE user_id). Right to erasure supported via deleted_at (audit trail preserved).';

COMMENT ON COLUMN hello_wall_entries.hello_hash IS
    'SHA-256 of hello_markdown at mirror time. Serves as immutability proof — pointing to this in the user''s profile attestation "Premier commit" section.';

COMMENT ON COLUMN hello_wall_entries.source_starter_repo IS
    'Which skilluv-community/starter-* template the user forked to complete Bonjour Skilluv. Used for analytics (which starters are most popular by orientation).';
