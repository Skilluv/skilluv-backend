-- Sprint 1 Phase 2 — Couche 0 : primitives polymorphes (comments, reactions, tags, tag_map).
--
-- Tout est polymorphe via (target_type, target_id) pour éviter une table dédiée par entité.
-- target_type est une string libre côté DB ; la liste contrôlée vit côté code (services/social.rs).

-- ─── Comments ───────────────────────────────────────────────────
CREATE TABLE comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type VARCHAR(30) NOT NULL,
    target_id UUID NOT NULL,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    parent_id UUID REFERENCES comments(id) ON DELETE CASCADE,
    edited BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ  -- soft delete: row kept for moderation audit
);

CREATE INDEX idx_comments_target ON comments (target_type, target_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_comments_author ON comments (author_id, created_at DESC);
CREATE INDEX idx_comments_parent ON comments (parent_id) WHERE parent_id IS NOT NULL;

-- ─── Reactions ──────────────────────────────────────────────────
CREATE TABLE reactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type VARCHAR(30) NOT NULL,
    target_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('upvote', 'downvote', 'heart', 'fire', 'wow')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (target_type, target_id, user_id, kind)
);

CREATE INDEX idx_reactions_target ON reactions (target_type, target_id, kind);
CREATE INDEX idx_reactions_user ON reactions (user_id, created_at DESC);

-- ─── Tags (unified vocabulary across challenges, projects, posts, questions, users) ──
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    category VARCHAR(30) NOT NULL CHECK (category IN ('language', 'topic', 'level', 'framework', 'tool', 'other')),
    color VARCHAR(7),  -- hex color like '#6c5ce7'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tags_category ON tags (category);

CREATE TABLE tag_map (
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    target_type VARCHAR(30) NOT NULL,
    target_id UUID NOT NULL,
    attached_by UUID REFERENCES users(id) ON DELETE SET NULL,
    attached_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tag_id, target_type, target_id)
);

CREATE INDEX idx_tag_map_target ON tag_map (target_type, target_id);

-- ─── Mentions (extracted from comments at write time) ──────────
-- Stored as a fast-lookup table so we can show "@you was mentioned" feeds without
-- re-parsing every comment body.
CREATE TABLE mentions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mentioned_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_type VARCHAR(30) NOT NULL,  -- 'comment' for now ; later 'post', 'message', etc.
    source_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_mentions_user ON mentions (mentioned_user_id, created_at DESC);
CREATE INDEX idx_mentions_source ON mentions (source_type, source_id);

-- ─── Seed initial tags ──────────────────────────────────────────
INSERT INTO tags (slug, name, category, color) VALUES
    -- Languages
    ('rust', 'Rust', 'language', '#dea584'),
    ('python', 'Python', 'language', '#3572A5'),
    ('javascript', 'JavaScript', 'language', '#f1e05a'),
    ('typescript', 'TypeScript', 'language', '#3178c6'),
    ('go', 'Go', 'language', '#00ADD8'),
    ('java', 'Java', 'language', '#b07219'),
    ('csharp', 'C#', 'language', '#178600'),
    ('cpp', 'C++', 'language', '#f34b7d'),
    ('php', 'PHP', 'language', '#4F5D95'),
    ('ruby', 'Ruby', 'language', '#701516'),
    ('kotlin', 'Kotlin', 'language', '#A97BFF'),
    ('swift', 'Swift', 'language', '#F05138'),
    -- Topics
    ('algorithms', 'Algorithms', 'topic', '#6c5ce7'),
    ('data-structures', 'Data structures', 'topic', '#6c5ce7'),
    ('design-systems', 'Design systems', 'topic', '#fd79a8'),
    ('ux-research', 'UX research', 'topic', '#fd79a8'),
    ('game-design', 'Game design', 'topic', '#fdcb6e'),
    ('web-security', 'Web security', 'topic', '#d63031'),
    ('crypto', 'Cryptography', 'topic', '#d63031'),
    ('career', 'Career', 'topic', '#00b894'),
    ('open-source', 'Open source', 'topic', '#00b894'),
    ('mobile', 'Mobile', 'topic', '#0984e3'),
    -- Levels
    ('beginner', 'Beginner', 'level', '#74b9ff'),
    ('intermediate', 'Intermediate', 'level', '#0984e3'),
    ('advanced', 'Advanced', 'level', '#2d3436')
ON CONFLICT (slug) DO NOTHING;
