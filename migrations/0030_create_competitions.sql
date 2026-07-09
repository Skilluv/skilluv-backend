-- Sprint 6 Phase 2 — seasons + tournaments + division ladder.
--
-- Seasons are 3-month windows. At the end of a season:
--   - gp_season for every guild resets to 0
--   - top/bottom percentile guilds change division (admin-triggered for V1)
--   - winners of season-bound tournaments receive their prize_pool
--
-- Tournaments support 3 kinds (individual, guild_war, hackathon) and 3 formats
-- (swiss, bracket, ladder). The Sprint 6 V1 implementation only requires the schema +
-- registration + manual scoring/conclusion. Matchmaking algorithms come Phase 5.

CREATE TABLE seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(40) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'upcoming' CHECK (status IN ('upcoming', 'active', 'ended')),
    closed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (ends_at > starts_at)
);

CREATE INDEX idx_seasons_status ON seasons (status, starts_at DESC);

-- Optional reference to the active season at any given time (lookup convenience).
CREATE OR REPLACE FUNCTION current_season_id() RETURNS UUID AS $$
    SELECT id FROM seasons WHERE status = 'active' ORDER BY starts_at DESC LIMIT 1;
$$ LANGUAGE SQL STABLE;

CREATE TABLE tournaments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID REFERENCES seasons(id) ON DELETE SET NULL,
    slug VARCHAR(60) NOT NULL UNIQUE,
    name VARCHAR(120) NOT NULL,
    description TEXT,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('individual', 'guild_war', 'hackathon')),
    format VARCHAR(20) NOT NULL DEFAULT 'ladder' CHECK (format IN ('swiss', 'bracket', 'ladder')),
    -- Pricing / prizes
    prize_pool_fragments INTEGER NOT NULL DEFAULT 0 CHECK (prize_pool_fragments >= 0),
    prize_pool_gp INTEGER NOT NULL DEFAULT 0 CHECK (prize_pool_gp >= 0),
    -- Sponsor (hackathon only, optional)
    sponsor_enterprise_id UUID REFERENCES enterprises(id) ON DELETE SET NULL,
    sponsor_logo_url VARCHAR(500),
    sponsor_blurb TEXT,
    -- Schedule
    registration_opens_at TIMESTAMPTZ,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    -- State machine: upcoming → registration → active → concluded   (admin-driven for V1)
    status VARCHAR(20) NOT NULL DEFAULT 'upcoming' CHECK (status IN ('upcoming', 'registration', 'active', 'concluded', 'cancelled')),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (ends_at > starts_at)
);

CREATE INDEX idx_tournaments_season ON tournaments (season_id, starts_at);
CREATE INDEX idx_tournaments_status ON tournaments (status, starts_at);
CREATE INDEX idx_tournaments_sponsor ON tournaments (sponsor_enterprise_id) WHERE sponsor_enterprise_id IS NOT NULL;

CREATE TABLE tournament_participants (
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    participant_type VARCHAR(20) NOT NULL CHECK (participant_type IN ('user', 'guild')),
    participant_id UUID NOT NULL,
    score INTEGER NOT NULL DEFAULT 0,
    rank INTEGER,
    prize_fragments_awarded INTEGER NOT NULL DEFAULT 0,
    prize_gp_awarded INTEGER NOT NULL DEFAULT 0,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tournament_id, participant_type, participant_id)
);

CREATE INDEX idx_tournament_participants_score ON tournament_participants (tournament_id, score DESC);
