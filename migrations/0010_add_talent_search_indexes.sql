-- Full-text search on users (display_name + username)
ALTER TABLE users ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        to_tsvector('simple', coalesce(display_name, '') || ' ' || coalesce(username, ''))
    ) STORED;

CREATE INDEX idx_users_search ON users USING GIN (search_vector);

-- Composite index for talent search filtering
CREATE INDEX idx_users_talent_search
    ON users (skill_domain, title, total_fragments DESC)
    WHERE role = 'user' AND profile_active = TRUE AND is_banned = FALSE;
