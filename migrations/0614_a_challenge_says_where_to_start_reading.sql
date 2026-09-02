-- Where to go and read, attached to the challenge that needs it.
--
-- ## The gap
--
-- The platform curates external resources already — `external_ai_resources`
-- (0220), `external_language_ecosystems` (0188), `external_credentials`
-- (0427), `external_bounty_programs` (0555), `external_opportunities` (0513).
-- Five tables, one per domain, each a catalogue of a whole trade. None of them
-- attaches to a challenge, so somebody who opens a brief and does not know
-- where to begin gets a domain-wide list or nothing.
--
-- That is the difference between a platform for people who already know how to
-- search and one for the students, self-taught and career-changers this is
-- for. Not giving the answer — giving the entrance. The half-hour spent
-- guessing which words to type is the half-hour that decides who stays.
--
-- ## Links, never copies
--
-- Every row is a URL somebody else hosts. That is what makes this legally
-- simple: linking is not reproduction. Migration 0558 established the rule for
-- the security catalogue — "every external target is linked, never rehosted,
-- and the attribution travels with the use" — because several of its forensic
-- datasets are CC-BY. Same rule, generalised: `attribution` carries whatever a
-- licence requires to travel, and it is empty for a link that requires
-- nothing.
--
-- ## Why `language` is not optional
--
-- The front is bilingual and the audience is francophone as much as
-- anglophone. Most of what is worth linking is in English, and that is fine —
-- but a French reader has to be able to see, at a glance, which of these they
-- can actually read. A resource that does not say is a resource somebody opens
-- and closes.
--
-- ## Why `access_note` is not optional either
--
-- 0220 introduced it and its comment is the reason: "what it takes to actually
-- reach this — free tier, GPU needed, course auditable without paying. Most
-- upstream lists assume a card and a fast connection, and that assumption is
-- the barrier." Empty string is allowed; NULL is not, so the author has to
-- have thought about it.

CREATE TABLE challenge_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenge_id UUID NOT NULL
        REFERENCES challenge_templates(id) ON DELETE CASCADE,

    kind VARCHAR(20) NOT NULL CHECK (kind IN (
        'documentation',  -- the official docs of the thing being used
        'course',         -- a course, a book, a series
        'article',        -- one article or post that explains one thing
        'video',          -- a talk, a screencast
        'community',      -- where practitioners of this actually answer
        'repository'      -- the repo the work lands in, or one to read
    )),

    title VARCHAR(200) NOT NULL CHECK (length(btrim(title)) >= 3),
    url TEXT NOT NULL CHECK (url ~ '^https://'),

    -- What language it is in. `mul` for a resource that carries several.
    language VARCHAR(8) NOT NULL DEFAULT 'en'
        CHECK (language ~ '^[a-z]{2,3}$'),

    -- One line, in the platform's own words, on why this one and not another.
    -- Not the publisher's own blurb.
    summary TEXT NOT NULL DEFAULT '',
    -- What it costs to actually reach it. Empty is allowed, NULL is not.
    access_note TEXT NOT NULL DEFAULT '',
    -- Whatever a licence requires to travel with the use. Empty for a plain
    -- link, which is most of them.
    attribution TEXT NOT NULL DEFAULT '',

    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- The same link twice on one challenge is a curation mistake, not a state
    -- to support.
    UNIQUE (challenge_id, url)
);

CREATE INDEX idx_challenge_resources_challenge
    ON challenge_resources (challenge_id, sort_order, id);

COMMENT ON TABLE challenge_resources IS
    'Where to start reading for one challenge. Links to material somebody else '
    'hosts — never a copy — with the language it is in and what it costs to '
    'reach it. The point is the entrance, not the answer: the search stays the '
    'learner''s work, and what is removed is the guessing about which words to '
    'type.';

-- ═══════════════════════════════════════════════════════════════════
-- The forum learns which challenge a question is about
-- ═══════════════════════════════════════════════════════════════════
--
-- `posts` (0027) has a category and an author and nothing else. So "I am stuck
-- on this one" is not expressible, and — the part that matters more — the
-- questions already answered about a challenge do not reach the next person
-- who starts it. That backlog is the most valuable teaching material this
-- platform will ever have, and it writes itself.

ALTER TABLE posts
    ADD COLUMN challenge_id UUID REFERENCES challenge_templates(id) ON DELETE SET NULL;

CREATE INDEX idx_posts_challenge
    ON posts (challenge_id, created_at DESC)
    WHERE challenge_id IS NOT NULL AND deleted_at IS NULL;

COMMENT ON COLUMN posts.challenge_id IS
    'The challenge this thread is about, when it is about one. NULL for every '
    'other post. What makes a question asked once readable by everybody who '
    'starts that challenge afterwards.';
