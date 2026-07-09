-- Activity log for heatmap (one row per user per day)
CREATE TABLE user_activity (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    activity_date DATE NOT NULL,
    challenges_completed INTEGER NOT NULL DEFAULT 0,
    fragments_earned INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, activity_date)
);

CREATE INDEX idx_user_activity_user ON user_activity (user_id, activity_date DESC);
