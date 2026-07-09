-- Challenge tags & categories
CREATE TABLE challenge_tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(50) NOT NULL UNIQUE,
    category VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE challenge_tag_map (
    challenge_id UUID NOT NULL REFERENCES challenges(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES challenge_tags(id) ON DELETE CASCADE,
    PRIMARY KEY (challenge_id, tag_id)
);

CREATE INDEX idx_challenge_tag_map_tag ON challenge_tag_map (tag_id);

-- Community fields on challenges
ALTER TABLE challenges
    ADD COLUMN is_community BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN community_status VARCHAR(20) DEFAULT 'draft'
        CHECK (community_status IN ('draft', 'review', 'approved', 'rejected')),
    ADD COLUMN review_feedback TEXT,
    ADD COLUMN featured BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN vote_count INTEGER NOT NULL DEFAULT 0;

-- Seed tags
INSERT INTO challenge_tags (name, category) VALUES
('algorithmes', 'topic'),
('web', 'topic'),
('data-structures', 'topic'),
('api', 'topic'),
('database', 'topic'),
('devops', 'topic'),
('machine-learning', 'topic'),
('security', 'topic'),
('frontend', 'topic'),
('backend', 'topic'),
('debutant', 'level'),
('intermediaire', 'level'),
('avance', 'level'),
('expert', 'level');
