-- Challenge votes (community upvotes)
CREATE TABLE challenge_votes (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, challenge_id)
);

CREATE INDEX idx_challenge_votes_challenge ON challenge_votes (challenge_id);
