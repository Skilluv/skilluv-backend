-- Challenge teams
CREATE TABLE challenge_teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    max_members INTEGER NOT NULL DEFAULT 4,
    status VARCHAR(20) NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'full', 'in_progress', 'submitted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_teams_challenge ON challenge_teams (challenge_id);

CREATE TABLE team_members (
    team_id UUID NOT NULL REFERENCES challenge_teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, user_id)
);

CREATE INDEX idx_team_members_user ON team_members (user_id);

-- Timer + team reference on submissions
ALTER TABLE challenge_submissions
    ADD COLUMN expires_at TIMESTAMPTZ,
    ADD COLUMN team_id UUID REFERENCES challenge_teams(id);
