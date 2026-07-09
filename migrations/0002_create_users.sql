-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    username VARCHAR(30) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    first_name VARCHAR(50) NOT NULL,
    last_name VARCHAR(50) NOT NULL,
    display_name VARCHAR(100) NOT NULL,
    skill_domain VARCHAR(20) NOT NULL CHECK (skill_domain IN ('code', 'design', 'game', 'security')),
    role VARCHAR(20) NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'mentor', 'admin', 'enterprise')),
    title VARCHAR(20) NOT NULL DEFAULT 'apprenti' CHECK (title IN ('apprenti', 'artisan', 'maitre', 'legende')),
    golden_stars INTEGER NOT NULL DEFAULT 0,
    total_fragments INTEGER NOT NULL DEFAULT 0,
    streak_current INTEGER NOT NULL DEFAULT 0,
    streak_last_activity DATE,
    trust_score REAL NOT NULL DEFAULT 100.0,
    country VARCHAR(3),
    -- Email verification
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    -- TOTP 2FA
    totp_secret BYTEA,
    totp_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    -- Email 2FA
    email_2fa_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    -- Account state
    profile_active BOOLEAN NOT NULL DEFAULT FALSE,
    is_banned BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users (email);
CREATE UNIQUE INDEX idx_users_username ON users (username);
CREATE INDEX idx_users_skill_domain ON users (skill_domain);
CREATE INDEX idx_users_role ON users (role);
CREATE INDEX idx_users_country ON users (country);
CREATE INDEX idx_users_total_fragments ON users (total_fragments DESC);
