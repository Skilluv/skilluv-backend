-- Profile fields
ALTER TABLE users
    ADD COLUMN bio TEXT,
    ADD COLUMN avatar_url VARCHAR(500),
    ADD COLUMN github VARCHAR(100),
    ADD COLUMN linkedin VARCHAR(200),
    ADD COLUMN website VARCHAR(500),
    ADD COLUMN twitter VARCHAR(100);

-- Privacy settings
CREATE TABLE user_privacy (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    show_email BOOLEAN NOT NULL DEFAULT FALSE,
    show_heatmap BOOLEAN NOT NULL DEFAULT TRUE,
    show_skill_tree BOOLEAN NOT NULL DEFAULT TRUE,
    show_badges BOOLEAN NOT NULL DEFAULT TRUE,
    show_streak BOOLEAN NOT NULL DEFAULT TRUE,
    allow_interest_requests BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
