-- Badges & achievements system
CREATE TABLE badges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT NOT NULL,
    icon VARCHAR(100) NOT NULL,
    category VARCHAR(30) NOT NULL CHECK (category IN ('streak', 'challenge', 'fragment', 'special')),
    condition_type VARCHAR(30) NOT NULL,
    condition_value INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_badges (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    badge_id UUID NOT NULL REFERENCES badges(id) ON DELETE CASCADE,
    earned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, badge_id)
);

CREATE INDEX idx_user_badges_user ON user_badges (user_id, earned_at DESC);

-- Seed initial badges
INSERT INTO badges (slug, name, description, icon, category, condition_type, condition_value) VALUES
('first_challenge', 'Premier Pas', 'Completer ton premier challenge', 'trophy', 'challenge', 'challenges_completed', 1),
('challenges_10', 'Defi Accepte', '10 challenges completes', 'sword', 'challenge', 'challenges_completed', 10),
('challenges_50', 'Gladiateur', '50 challenges completes', 'colosseum', 'challenge', 'challenges_completed', 50),
('streak_7', 'Flamme Naissante', '7 jours consecutifs', 'flame', 'streak', 'streak_days', 7),
('streak_30', 'Flamme Ardente', '30 jours consecutifs', 'fire', 'streak', 'streak_days', 30),
('streak_100', 'Centurion', '100 jours consecutifs', 'shield', 'streak', 'streak_days', 100),
('fragments_100', 'Collecteur', 'Accumuler 100 fragments', 'gem', 'fragment', 'total_fragments', 100),
('fragments_500', 'Artisan Confirme', 'Accumuler 500 fragments', 'diamond', 'fragment', 'total_fragments', 500),
('fragments_2000', 'Maitre Forgeron', 'Accumuler 2000 fragments', 'crown', 'fragment', 'total_fragments', 2000);
