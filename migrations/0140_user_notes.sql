-- SKI-37 (Post-MVP T1-02) — private notes on anything a user has looked at.
--
-- Same polymorphic family as `bookmarks` (SKI-36), deliberately a separate
-- table rather than a column on it: the two have different lifecycles. A
-- note is written about something you read once and may never save; a
-- bookmark is something you save and may never annotate. Folding notes
-- into bookmarks would force a bookmark row into existence just to store
-- a thought, and folding bookmarks into notes would do the reverse.
--
-- Notes are ALWAYS private. There is no visibility column and no read path
-- that returns another user's notes — that is the whole point. Making them
-- shareable later would be a new feature with its own moderation surface,
-- not a flag flip.

CREATE TABLE IF NOT EXISTS user_notes (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Same target family as `bookmarks`; see migration 0139 for why the
    -- ticket's `bounty` is spelled `slice` here.
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
    -- 1000 chars: a note, not an essay. Mirrored in the route so callers
    -- get a 400 instead of a database error.
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 1000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One note per (user, target). The endpoint is an upsert, so the
    -- primary key IS the conflict target.
    PRIMARY KEY (user_id, target_type, target_id)
);

-- "My notes, newest edit first", optionally narrowed to one target type.
CREATE INDEX IF NOT EXISTS idx_user_notes_user_updated
    ON user_notes (user_id, updated_at DESC);
