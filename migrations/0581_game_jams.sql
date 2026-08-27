-- Game jams: the game domain's contest, built on the tournament machinery.
--
-- A jam is a tournament — `tournaments`, `tournament_participants`,
-- `tournament_submissions` — the same reuse security made in 0554, so a jam's
-- podium is a real `tournament_podium` and the jam-winner badge and the
-- game_jam_winner attestation both fall out of finalising it. No parallel
-- competition engine.
--
-- Three formats as `tournament_kinds` rows, all community-voted — the jam norm,
-- and the multi-axis scoring below IS the community voting. `game_jams` holds
-- what a jam has that a generic tournament does not: a theme revealed on a
-- clock, two deadlines, the scoring axes, and whether it is solo, team, or
-- either. Two small side tables carry the per-entry extras the shared
-- `tournament_submissions` should not grow columns for.

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, required_rule_keys, sort_order, is_juried,
     allows_community_vote)
VALUES
('game_jam_48h', 'game', 'Game jam (48h)',
 'A theme revealed on Friday evening, a playable build by Sunday evening. The '
 'weekly rhythm — small scope, finished, shown.',
 TRUE, FALSE, FALSE, ARRAY['theme', 'submission_deadline'], 800, FALSE, TRUE),
('game_jam_72h', 'game', 'Game jam (72h)',
 'A long weekend on one theme. Room for a second mechanic and a pass of polish.',
 TRUE, FALSE, FALSE, ARRAY['theme', 'submission_deadline'], 810, FALSE, TRUE),
('game_jam_week', 'game', 'Game jam (one week)',
 'The flagship: a week, a theme, and a finished small game. Scored across every '
 'axis a game has.',
 TRUE, FALSE, FALSE, ARRAY['theme', 'submission_deadline'], 820, FALSE, TRUE);

CREATE TABLE game_jams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The jam is a tournament; this is its game-specific half. One-to-one.
    tournament_id UUID NOT NULL UNIQUE REFERENCES tournaments(id) ON DELETE CASCADE,
    theme TEXT NOT NULL,
    -- NULL until the reveal — the theme is a secret a clock releases, which is
    -- half the point of a jam.
    theme_revealed_at TIMESTAMPTZ,
    submission_deadline TIMESTAMPTZ NOT NULL,
    voting_deadline TIMESTAMPTZ NOT NULL,
    -- The axes a submission is scored on. Independent scores, not one global
    -- number, because a game that is fun and ugly and a game that is beautiful
    -- and dull are different results a jam should tell apart.
    scoring_axes JSONB NOT NULL DEFAULT '["fun", "theme", "art", "audio", "innovation"]'::jsonb,
    solo_or_team VARCHAR(10) NOT NULL DEFAULT 'both'
        CHECK (solo_or_team IN ('solo_only', 'team_only', 'both')),
    team_size_max SMALLINT NOT NULL DEFAULT 4 CHECK (team_size_max >= 1),
    CONSTRAINT jam_votes_close_after_submissions
        CHECK (voting_deadline >= submission_deadline),
    CONSTRAINT scoring_axes_is_a_list
        CHECK (jsonb_typeof(scoring_axes) = 'array' AND jsonb_array_length(scoring_axes) > 0)
);

-- The per-entry extras. The build URL is the tournament submission's own
-- artifact_url (artifact_type 'demo'); these are what a jam adds to it.
CREATE TABLE game_jam_submission_details (
    submission_id UUID PRIMARY KEY REFERENCES tournament_submissions(id) ON DELETE CASCADE,
    source_code_url VARCHAR(500),
    postmortem_md TEXT
);

-- Multi-axis community voting. One score per (submission, voter, axis); the
-- jam's composite is computed from these at finalise time, never stored raw as
-- a single number that could drift from its parts.
CREATE TABLE game_jam_axis_votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    submission_id UUID NOT NULL REFERENCES tournament_submissions(id) ON DELETE CASCADE,
    voter_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    axis VARCHAR(20) NOT NULL,
    score SMALLINT NOT NULL CHECK (score BETWEEN 1 AND 5),
    voted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (submission_id, voter_user_id, axis)
);

CREATE INDEX idx_game_jam_axis_votes_submission
    ON game_jam_axis_votes (submission_id, axis);

COMMENT ON TABLE game_jams IS
    'A game jam, the game-specific half of a tournament. Finalising the '
    'tournament ranks its participants — a real tournament_podium — from the '
    'composite of game_jam_axis_votes, and issues game_jam_winner to the top '
    'three and game_jam_participant to the rest.';
