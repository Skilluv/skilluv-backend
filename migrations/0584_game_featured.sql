-- Featured game creator of the week.
--
-- The same editorial recognition security has (0552's featured basis, the
-- featured worker) and design and the rest: a person put forward for a week,
-- an attestation issued, a Discord post, a place on a landing page. One row per
-- week — the unique on `week_starts_at` makes two featurings for the same week
-- impossible, which is the whole point of "of the week".

CREATE TABLE game_featured (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    week_starts_at DATE NOT NULL UNIQUE,
    week_ends_at DATE NOT NULL,
    bio_md TEXT NOT NULL,
    -- The games put forward, by slice id. An array rather than a join table
    -- because it is a short, ordered editorial selection, not a queryable set.
    highlighted_projects UUID[] NOT NULL DEFAULT '{}',
    -- Optional itch embeds and a short interview, both editorial JSON the
    -- landing page renders and nothing queries.
    itch_embeds JSONB,
    interview_qa_json JSONB,
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT featured_week_is_ordered CHECK (week_ends_at >= week_starts_at)
);

CREATE INDEX idx_game_featured_user ON game_featured (user_id);
CREATE INDEX idx_game_featured_week ON game_featured (week_starts_at DESC);

COMMENT ON TABLE game_featured IS
    'One featured game creator per week. Publishing a row issues the '
    'featured_game_creator attestation and posts to Discord; the landing page '
    '/game/featured/{username} reads the most recent.';
