-- Sprint 3 Phase 2 — forum + Q&A.
--
-- Categories are admin-curated (no user-created). Posts have a kind:
--   - 'discussion'    : free-form thread
--   - 'question'      : Q&A, can have a bounty + accepted_answer (a comment.id)
--   - 'announcement'  : admin-only, usually pinned, replies often locked
--
-- Replies live in the existing polymorphic `comments` table with target_type='post'.
-- Votes live in `reactions` with target_type='post' or 'comment' and kind='upvote'/'downvote'.

CREATE TABLE forum_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    icon VARCHAR(50),
    color VARCHAR(7),
    position INTEGER NOT NULL DEFAULT 0,
    locked BOOLEAN NOT NULL DEFAULT FALSE,  -- if true, only mods can post
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_forum_categories_position ON forum_categories (position, name);

CREATE TABLE posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    category_id UUID NOT NULL REFERENCES forum_categories(id) ON DELETE RESTRICT,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('discussion', 'question', 'announcement')),
    title VARCHAR(200) NOT NULL CHECK (length(title) BETWEEN 3 AND 200),
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 20000),
    -- Q&A specific
    bounty_fragments INTEGER NOT NULL DEFAULT 0 CHECK (bounty_fragments >= 0),
    accepted_answer_id UUID REFERENCES comments(id) ON DELETE SET NULL,
    -- Moderation flags
    pinned BOOLEAN NOT NULL DEFAULT FALSE,
    locked BOOLEAN NOT NULL DEFAULT FALSE,
    -- Counters (denormalised, updated by triggers in app code)
    view_count BIGINT NOT NULL DEFAULT 0,
    -- FTS
    search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('simple', coalesce(body, '')), 'B')
    ) STORED,
    edited BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_posts_category ON posts (category_id, pinned DESC, created_at DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_posts_author ON posts (author_id, created_at DESC);
CREATE INDEX idx_posts_kind ON posts (kind, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_posts_search ON posts USING GIN (search_vector);
CREATE INDEX idx_posts_open_bounty ON posts (bounty_fragments DESC)
    WHERE kind = 'question' AND accepted_answer_id IS NULL AND deleted_at IS NULL;

-- Seed categories — curated taxonomy. Slugs are stable identifiers used in front URLs.
INSERT INTO forum_categories (slug, name, description, icon, color, position, locked) VALUES
    ('announcements', 'Annonces',          'Communications officielles Skilluv',                'megaphone', '#1a1a2e', 0,  TRUE),
    ('help',          'Aide & questions',  'Bloqué sur un challenge ? Pose ta question.',       'question-circle', '#0984e3', 10, FALSE),
    ('challenges',    'Discussions challenges', 'Échanges autour des challenges (post-complétion)', 'flag',  '#6c5ce7', 20, FALSE),
    ('show-and-tell', 'Show & tell',       'Partage ton projet, ton CV, ta réussite',           'sparkles', '#fd79a8', 30, FALSE),
    ('career',        'Carrière',          'Salaires, négociation, entretiens, freelancing',    'briefcase', '#00b894', 40, FALSE),
    ('open-source',   'Open source',       'Discussions OSS, projets cherchant des contribs',   'git-branch', '#00b894', 50, FALSE),
    ('rust',          'Rust',              'Tout sur Rust',                                     'cog', '#dea584', 60, FALSE),
    ('python',        'Python',            'Tout sur Python',                                   'cog', '#3572A5', 61, FALSE),
    ('javascript',    'JavaScript & TS',   'Frontend, Node, TS',                                'cog', '#f1e05a', 62, FALSE),
    ('go',            'Go',                'Tout sur Go',                                       'cog', '#00ADD8', 63, FALSE),
    ('design',        'Design',            'UX/UI, design systems, outils',                     'palette', '#fd79a8', 70, FALSE),
    ('game-dev',      'Game dev',          'Game design, moteurs, prototypage',                 'gamepad', '#fdcb6e', 80, FALSE),
    ('security',      'Sécurité',          'AppSec, pentest, CTF, reverse',                     'shield', '#d63031', 90, FALSE),
    ('mobile',        'Mobile',            'iOS, Android, cross-platform',                      'smartphone', '#0984e3', 100, FALSE),
    ('community',     'Communauté',        'Vie de la plateforme, suggestions, feedback',       'users', '#6c5ce7', 110, FALSE),
    ('off-topic',     'Off-topic',         'Tout le reste — restez courtois',                   'coffee', '#74b9ff', 200, FALSE)
ON CONFLICT (slug) DO NOTHING;
