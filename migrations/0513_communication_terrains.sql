-- Where communication work can actually be done.
--
-- Two different things, and the tickets asked for them as if they were one.
--
-- ## T-01: projects that want documentation — `terrain_proposals`
--
-- The table 0418 built. A proposal names an upstream repository, the labels
-- its ingestion should watch, and why it is worth somebody's first
-- contribution. A steward adopts it or declines it with a reason; nothing is
-- seeded straight into `projects`, because a terrain nobody has looked at is
-- a terrain that sends beginners at a repository which stopped answering two
-- years ago.
--
-- The five below are chosen for one property: their maintainers have said, in
-- writing, that documentation and translation contributions are welcome. That
-- is the difference between a project a newcomer can help and a project a
-- newcomer will be ignored by, and it cannot be inferred from stars.
--
-- ## T-02: a call for papers is not a terrain — `external_opportunities`
--
-- Ticket T-02 asked for `external_devrel_opportunities`, and education's
-- ticket T-03 asked for `external_education_platforms`. They are the same
-- table: a curated outside opportunity, with a deadline, that a member can
-- apply to and the platform does not run.
--
-- Two tables would mean two curation flows, two listings, two staleness
-- problems and two answers to "what is open right now". The same argument
-- 0415 made about portfolios and 0413 about missions. One table, a `kind`,
-- and a domain.
--
-- It is deliberately *not* `terrain_proposals`: a terrain becomes a project
-- with slices somebody claims, and a conference deadline becomes nothing —
-- it passes. Storing the second as the first would leave the adoption
-- columns permanently NULL and the listing full of dead dates.
--
-- ## Nothing is seeded into it here
--
-- A call for papers is true for about three months. Seeding a list in a
-- migration would ship a file that is wrong before the first user reads it,
-- and the wrongness would be invisible — a closed CFP looks exactly like an
-- open one until somebody applies. The table, its curation flow and its
-- listing are what this migration provides; the rows come from a curator,
-- which is what `curated_by` is for.

-- ═══════════════════════════════════════════════════════════════════
-- Projects that want documentation
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO terrain_proposals
    (slug, name, skill_domain, kind, upstream_url, ingestion_labels, why_md, sort_order) VALUES

('rust-lang-docs', 'Rust — documentation and translations', 'communication', 'oss_repo',
 'https://github.com/rust-lang/rust',
 ARRAY['A-docs', 'E-easy', 'T-doc'],
 'Rust documentation is reviewed by people who take reviewing seriously, and documentation contributions are accepted without anybody having to know the compiler. A good first ground: the feedback is demanding and kind at once, which is rare.',
 310),

('godot-docs', 'Godot Engine — documentation', 'communication', 'oss_repo',
 'https://github.com/godotengine/godot-docs',
 ARRAY['documentation', 'good first issue', 'translation'],
 'A documentation repository separate from the engine, which means contributing without compiling anything. Translations are explicitly asked for there, one language at a time.',
 320),

('bevy-book', 'Bevy — the book and the examples', 'communication', 'oss_repo',
 'https://github.com/bevyengine/bevy-website',
 ARRAY['A-Docs', 'C-Needs-Documentation', 'good first issue'],
 'The Bevy book moves every release and the documentation runs behind it: there is always identified, labelled work waiting. The project says itself that documentation is what it lacks most.',
 330),

('home-assistant-docs', 'Home Assistant — documentation and i18n', 'communication', 'oss_repo',
 'https://github.com/home-assistant/home-assistant.io',
 ARRAY['documentation', 'translation', 'has-parent'],
 'An enormous documentation set, translated into dozens of languages, with a tooled translation pipeline. The best ground for a technical translator starting out: the process is documented and the maintainers answer.',
 340),

('skilluv-docs', 'Skilluv — its own documentation', 'communication', 'internal',
 'https://github.com/skilluv/skilluv-backend',
 ARRAY['documentation', 'i18n', 'good first doc'],
 'The platform is the first ground that has to accept being corrected by its own community. What is written here — API references, guides, release notes — is reviewed by the people who wrote it, and it is the only place a contributor can see their correction live the same day.',
 350);

-- ═══════════════════════════════════════════════════════════════════
-- Outside opportunities
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE external_opportunities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE
        CHECK (slug ~ '^[a-z0-9-]+$' AND length(slug) BETWEEN 3 AND 80),
    -- What kind of thing this is. Decides what the dates mean and what a
    -- listing shows next to it.
    kind VARCHAR(30) NOT NULL CHECK (kind IN (
        'conference_cfp',      -- a call for papers, with a submission deadline
        'meetup_speaker_slot', -- a local group looking for somebody to speak
        'writing_call',        -- a publication commissioning articles
        'translation_call',    -- a project asking for a language
        'teaching_position',   -- a school or bootcamp hiring trainers
        'curriculum_call'      -- an organisation commissioning a programme
    )),
    skill_domain VARCHAR(30) NOT NULL
        REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),
    -- Who is offering it. Named rather than left inside the title, because a
    -- listing filters and groups by it.
    organisation VARCHAR(160) NOT NULL CHECK (btrim(organisation) <> ''),
    url TEXT NOT NULL CHECK (url ~ '^https://'),
    summary TEXT NOT NULL DEFAULT '',
    -- Where it happens. NULL for something entirely online.
    location VARCHAR(120),
    country CHAR(2),
    is_remote BOOLEAN NOT NULL DEFAULT FALSE,
    -- When applications close. The one date that must be right: everything
    -- else about a stale row is cosmetic, and this one decides whether the
    -- listing is lying.
    closes_at TIMESTAMPTZ,
    -- When the thing itself happens, where that is a different date.
    happens_at TIMESTAMPTZ,
    -- Trades this is aimed at, by orientation slug. Empty means the whole
    -- domain.
    orientation_slugs TEXT[] NOT NULL DEFAULT '{}',
    curated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    curated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Taken down by a curator, with the reason. Rows are never deleted: a
    -- member who applied to something has a right to still find it.
    withdrawn_at TIMESTAMPTZ,
    withdrawn_reason TEXT,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT an_opportunity_runs_forward CHECK (
        closes_at IS NULL OR happens_at IS NULL OR happens_at >= closes_at
    ),
    CONSTRAINT a_withdrawal_says_why CHECK (
        withdrawn_at IS NULL OR btrim(COALESCE(withdrawn_reason, '')) <> ''
    )
);

COMMENT ON TABLE external_opportunities IS
    'Curated outside opportunities a member can apply to and the platform '
    'does not run: calls for papers, speaker slots, teaching positions. One '
    'table rather than the two the communication and education backlogs each '
    'asked for — a deadline, an organisation and a link are the same three '
    'facts whichever domain wants them.';

COMMENT ON COLUMN external_opportunities.closes_at IS
    'When applications close. The one date that has to be right: a closed '
    'call looks exactly like an open one until somebody applies.';

-- What a listing reads: open, in this domain, soonest deadline first.
CREATE INDEX idx_external_opportunities_open
    ON external_opportunities (skill_domain, closes_at)
    WHERE withdrawn_at IS NULL;

CREATE INDEX idx_external_opportunities_orientations
    ON external_opportunities USING GIN (orientation_slugs);

CREATE TRIGGER trg_external_opportunities_updated_at
    BEFORE UPDATE ON external_opportunities
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();
