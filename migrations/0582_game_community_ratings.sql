-- Community ratings on shipped games, and the leaderboards they feed.
--
-- A validated game slice can be rated by the community — a single 1-to-5 per
-- person, editable, the way a jam vote is one verdict rather than a ballot box.
-- The "fun games this month" leaderboards read from here, and a game that earns
-- a real average across enough votes gets a craft-score bump.
--
-- Anti-fraud is structural where it can be — one row per (slice, rater), a
-- unique index — and a rule where it cannot: the service refuses a rater whose
-- account is younger than thirty days and flags a burst, because "account age"
-- and "rate over time" are not things a CHECK can see.

CREATE TABLE game_community_ratings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    rater_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment_md TEXT,
    rated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- One rating per person per game. Editable in place.
    UNIQUE (slice_id, rater_user_id)
);

CREATE INDEX idx_game_community_ratings_slice ON game_community_ratings (slice_id);
CREATE INDEX idx_game_community_ratings_rater ON game_community_ratings (rater_user_id);

COMMENT ON TABLE game_community_ratings IS
    'One community rating per person per validated game slice. The fun '
    'leaderboards rank by average rating weighted by vote count; a slice above '
    'the bar with enough votes earns a craft-score bump. Account-age and '
    'burst-rate checks are in the service, not here.';
