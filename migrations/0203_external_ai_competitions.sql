-- Competitions and leaderboards happening elsewhere.
--
-- ## Why the platform points at them at all
--
-- A Kaggle medal and a place on a public leaderboard are among the few AI
-- credentials a recruiter already recognises. Skilluv cannot mint those, and
-- pretending otherwise would be the sort of closed loop this platform exists
-- to avoid. Pointing at them, with a deadline somebody can still meet, is
-- worth more than a competition of our own that nobody outside has heard of.
--
-- ## Curated, and dated
--
-- `reviewed_by_user_id` because an automatic feed of every open competition
-- is a firehose, and the value is in the choice. `deadline` because a listing
-- that keeps showing closed competitions teaches people to stop reading it —
-- which is why the index only serves the ones still open.

CREATE TABLE external_ai_competitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform VARCHAR(30) NOT NULL
        CHECK (platform IN (
            'kaggle',
            'huggingface_leaderboard',
            'openreview',
            'aicrowd',
            'zindi',           -- African data-science competitions
            'other'
        )),
    title VARCHAR(200) NOT NULL CHECK (length(btrim(title)) > 0),
    url TEXT NOT NULL CHECK (url ~ '^https://'),
    -- Why this one and not the forty others open right now. The whole value
    -- of a curated feed is in this sentence.
    why_this_one TEXT NOT NULL CHECK (length(btrim(why_this_one)) > 0),

    -- NULL for a rolling leaderboard, which has no deadline by nature.
    deadline TIMESTAMPTZ,
    -- Free text: prizes come as money, as compute credits, as a conference
    -- ticket, and normalising that into a number would lose most of it.
    prize_note TEXT,
    -- Which trades it suits. Empty means the whole domain.
    orientation_slugs TEXT[] NOT NULL DEFAULT '{}',

    -- Who put it here. A curated list with no named curator is a list nobody
    -- is answerable for.
    reviewed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    is_published BOOLEAN NOT NULL DEFAULT FALSE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Publishing is a decision somebody makes. Without the reviewer, a row
    -- that arrived from an automated fetch could publish itself.
    CONSTRAINT external_ai_competitions_published_was_reviewed
        CHECK (is_published = FALSE OR reviewed_by_user_id IS NOT NULL)
);

COMMENT ON TABLE external_ai_competitions IS
    'Competitions and leaderboards outside Skilluv, chosen by a curator. A '
    'Kaggle medal is a credential recruiters already read, and no internal '
    'contest can substitute for one.';

COMMENT ON COLUMN external_ai_competitions.why_this_one IS
    'Why this competition and not the forty others open right now. A curated '
    'feed whose rows cannot answer that is an uncurated feed.';

-- The listing readers ask for: published, still open, soonest first.
CREATE INDEX idx_external_ai_competitions_open
    ON external_ai_competitions (deadline)
    WHERE is_published = TRUE;

CREATE INDEX idx_external_ai_competitions_orientations
    ON external_ai_competitions USING gin (orientation_slugs);

CREATE OR REPLACE FUNCTION touch_external_ai_competitions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_external_ai_competitions_updated_at
    BEFORE UPDATE ON external_ai_competitions
    FOR EACH ROW EXECUTE FUNCTION touch_external_ai_competitions_updated_at();
