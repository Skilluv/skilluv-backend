-- SKI-36 (Post-MVP T1-01) — polymorphic bookmarks.
--
-- A user browsing Skilluv runs into interesting things across unrelated
-- surfaces: a challenge template, an OSS project, a mentor's profile, a
-- team, someone else's deliverable, a bounty. Without a place to park
-- them, the session ends and none of it is ever revisited.
--
-- Polymorphic on purpose: one table, one endpoint family, one front-end
-- component. The alternative (six typed join tables) multiplies schema
-- and API surface for zero behavioural gain — bookmarks carry no
-- type-specific columns.
--
-- No foreign key on target_id: it would need six nullable typed columns
-- plus a CHECK to keep exactly one populated. Referential integrity is
-- enforced at the service layer (`saved_items::assert_target_exists`,
-- which resolves target_type to its real table) and dangling rows are
-- filtered out on read, so a deleted target degrades to an invisible
-- row rather than a broken response.

-- Note on `slice`: the ticket asked for a `bounty` target type. Standalone
-- bounties no longer exist — migration 0074 folded `oss_bounties` into
-- `project_slices`, where a paid opportunity is a slice with a non-zero
-- `credits_reward`. `slice` is therefore the faithful translation of the
-- intent (save a paid opportunity for later) onto the schema as it stands.

CREATE TABLE IF NOT EXISTS bookmarks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_type VARCHAR(20) NOT NULL
        CHECK (target_type IN (
            'challenge_template',
            'project',
            'user',
            'team',
            'deliverable',
            'slice'
        )),
    target_id UUID NOT NULL,
    -- Optional user-defined folder ("game-dev-projects", "mentors-frontend").
    -- Slug-shaped so it stays URL-safe as a filter query param.
    folder_slug VARCHAR(60)
        CHECK (folder_slug IS NULL OR
               (folder_slug ~ '^[a-z0-9-]+$' AND length(folder_slug) BETWEEN 1 AND 60)),
    notes TEXT CHECK (notes IS NULL OR length(notes) <= 1000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Bookmarking is a set membership, not a log: re-bookmarking the same
    -- target updates the existing row instead of stacking duplicates.
    CONSTRAINT bookmarks_unique_target UNIQUE (user_id, target_type, target_id)
);

-- Primary read path: "my bookmarks, optionally filtered by type, newest first".
CREATE INDEX IF NOT EXISTS idx_bookmarks_user_type
    ON bookmarks (user_id, target_type, created_at DESC);

-- Secondary read path: folder filter. Partial — most bookmarks are unfiled.
CREATE INDEX IF NOT EXISTS idx_bookmarks_user_folder
    ON bookmarks (user_id, folder_slug)
    WHERE folder_slug IS NOT NULL;

-- Reverse lookup, used by the dangling-row cleanup and by future
-- "N people bookmarked this" counters.
CREATE INDEX IF NOT EXISTS idx_bookmarks_target
    ON bookmarks (target_type, target_id);
