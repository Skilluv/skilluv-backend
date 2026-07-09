-- Sprint 4 Phase 2 — guilds (MMO/F1-style persistent groups).
--
-- Design decisions (cf. docs/community-architecture.md sections 3-4):
--   - 30 members max (50 once level ≥10).
--   - Solo membership : one user belongs to exactly one guild ; 7-day cooldown on leave.
--   - GP : 10% of fragments earned by a member is added to the guild's pot.
--   - Creation gating : founder is artisan+ (≥500 fragments), supplies 3 co-founders,
--     pays 200 fragments to mint the guild.
--   - 3 invitation flows : in-platform direct, shareable token link, application.

CREATE TABLE guilds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) NOT NULL UNIQUE,
    tag VARCHAR(5) NOT NULL UNIQUE CHECK (length(tag) BETWEEN 3 AND 5 AND tag = UPPER(tag)),
    name VARCHAR(60) NOT NULL CHECK (length(name) BETWEEN 3 AND 60),
    description TEXT,
    motto VARCHAR(140),
    logo_url VARCHAR(500),
    banner_url VARCHAR(500),
    color_primary VARCHAR(7),
    color_secondary VARCHAR(7),
    founder_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    membership_mode VARCHAR(20) NOT NULL DEFAULT 'application' CHECK (membership_mode IN ('open', 'application', 'invite_only')),
    max_members INTEGER NOT NULL DEFAULT 30 CHECK (max_members BETWEEN 1 AND 100),
    level INTEGER NOT NULL DEFAULT 1 CHECK (level >= 1),
    gp_total BIGINT NOT NULL DEFAULT 0 CHECK (gp_total >= 0),
    gp_season BIGINT NOT NULL DEFAULT 0 CHECK (gp_season >= 0),
    division VARCHAR(20) NOT NULL DEFAULT 'bronze' CHECK (division IN ('bronze', 'silver', 'gold', 'platinum', 'legende')),
    forum_category_id UUID,  -- FK added below after forum_categories has guild_id
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    disbanded_at TIMESTAMPTZ
);

CREATE INDEX idx_guilds_division ON guilds (division, gp_total DESC) WHERE disbanded_at IS NULL;
CREATE INDEX idx_guilds_gp_season ON guilds (gp_season DESC) WHERE disbanded_at IS NULL;

CREATE TABLE guild_members (
    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member' CHECK (role IN ('founder', 'officer', 'member', 'recruit')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    gp_contributed BIGINT NOT NULL DEFAULT 0 CHECK (gp_contributed >= 0),
    PRIMARY KEY (guild_id, user_id)
);

-- A user can be in at most one active guild at a time. Enforced at app level (cf. join logic) and
-- also via a unique index :
CREATE UNIQUE INDEX idx_guild_members_solo ON guild_members (user_id);
CREATE INDEX idx_guild_members_guild_role ON guild_members (guild_id, role);

CREATE TABLE guild_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    inviter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_user_id UUID REFERENCES users(id) ON DELETE CASCADE,  -- in-platform target
    token VARCHAR(64) UNIQUE,                                      -- shareable-link mode
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK ((invited_user_id IS NOT NULL) OR (token IS NOT NULL))
);

CREATE INDEX idx_guild_invitations_invited ON guild_invitations (invited_user_id) WHERE accepted_at IS NULL AND revoked_at IS NULL;
CREATE INDEX idx_guild_invitations_token ON guild_invitations (token) WHERE token IS NOT NULL AND accepted_at IS NULL AND revoked_at IS NULL;

CREATE TABLE guild_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    applicant_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    message TEXT NOT NULL CHECK (length(message) BETWEEN 1 AND 2000),
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected', 'withdrawn')),
    decided_by_id UUID REFERENCES users(id) ON DELETE SET NULL,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (guild_id, applicant_id, status) DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX idx_guild_applications_pending ON guild_applications (guild_id) WHERE status = 'pending';

CREATE TABLE user_guild_cooldown (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    available_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE guild_wars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenger_guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    defender_guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    stake_gp BIGINT NOT NULL CHECK (stake_gp > 0),
    -- Status flow : proposed → accepted (in_progress) → concluded ; OR proposed → rejected
    status VARCHAR(20) NOT NULL DEFAULT 'proposed' CHECK (status IN ('proposed', 'accepted', 'rejected', 'concluded')),
    challenger_score INTEGER NOT NULL DEFAULT 0,
    defender_score INTEGER NOT NULL DEFAULT 0,
    winner_guild_id UUID REFERENCES guilds(id),
    proposed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    decided_at TIMESTAMPTZ,            -- accept or reject decision
    ends_at TIMESTAMPTZ,               -- when war auto-concludes
    concluded_at TIMESTAMPTZ,
    CHECK (challenger_guild_id <> defender_guild_id)
);

CREATE INDEX idx_guild_wars_challenger ON guild_wars (challenger_guild_id, status, proposed_at DESC);
CREATE INDEX idx_guild_wars_defender ON guild_wars (defender_guild_id, status, proposed_at DESC);

-- Forum: guilds can own a private category (auto-created on guild creation).
ALTER TABLE forum_categories ADD COLUMN guild_id UUID REFERENCES guilds(id) ON DELETE CASCADE;
CREATE INDEX idx_forum_categories_guild ON forum_categories (guild_id) WHERE guild_id IS NOT NULL;

-- Wire the forum_category_id FK now that the column exists on forum_categories.
ALTER TABLE guilds ADD CONSTRAINT fk_guilds_forum_category
    FOREIGN KEY (forum_category_id) REFERENCES forum_categories(id) ON DELETE SET NULL;
