-- Orientations in more than one language, and a record of what replaced what.
--
-- ## Why the base row keeps its text
--
-- Twenty-four queries read `orientations.name` and `orientations.description`
-- today. Moving that text out would break every one of them for no gain, so
-- the base row stays authoritative for the default locale and this table
-- carries the others. One place per locale, nothing duplicated: a reader
-- asks for a locale, finds a row here or falls back to the base.
--
-- The CHECK forbidding the default locale is what keeps that invariant true.
-- Without it a French row could exist in both places and drift, and nothing
-- would say which one is right.
--
-- ## Why not a YAML catalogue
--
-- Notification copy lives in `locales/*.yml` because it ships with the code.
-- Orientations do not: an operator creates and edits them at runtime through
-- the admin panel, so their text has to live where they can write it.

CREATE TABLE orientation_translations (
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE CASCADE,
    locale VARCHAR(5) NOT NULL
        CHECK (locale IN ('en', 'ar')),
    name VARCHAR(120) NOT NULL
        CHECK (length(btrim(name)) > 0),
    -- Empty is allowed: a translated name with an untranslated description is
    -- a normal intermediate state, and better than blocking the name on it.
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (orientation_id, locale)
);

COMMENT ON TABLE orientation_translations IS
    'Orientation text in locales other than the default. The default lives on '
    'the orientations row itself; a reader falls back to it when no row here '
    'matches.';

COMMENT ON COLUMN orientation_translations.locale IS
    'Never the default locale: that text is on the orientations row, and '
    'holding it twice is how the two copies start disagreeing.';

-- Reading the catalogue in one language is the common query.
CREATE INDEX idx_orientation_translations_locale
    ON orientation_translations (locale);

-- ═══════════════════════════════════════════════════════════════════
-- What replaced what
-- ═══════════════════════════════════════════════════════════════════
--
-- `is_archived` already says an orientation can no longer be chosen while
-- staying visible on the profiles that hold it. What it does not say is
-- where its people went.
--
-- That gap is not cosmetic. A recruiter filtering on `web-frontend-developer`
-- silently misses every profile still carrying `dev-frontend`, and a link to
-- an archived orientation leads nowhere instead of forwarding. Both are
-- failures of the one thing this platform sells: finding who can do what.

ALTER TABLE orientations
    ADD COLUMN replaced_by UUID REFERENCES orientations(id) ON DELETE SET NULL;

COMMENT ON COLUMN orientations.replaced_by IS
    'The orientation this one became. Set when archiving a slug that was '
    'renamed or split, so search can follow the lineage and an old link can '
    'forward instead of dying.';

-- An orientation cannot replace itself, and a live one has nothing to point
-- at: only something archived has been replaced.
ALTER TABLE orientations
    ADD CONSTRAINT orientations_replaced_by_is_another
    CHECK (replaced_by IS NULL OR replaced_by <> id);

ALTER TABLE orientations
    ADD CONSTRAINT orientations_replaced_by_needs_archived
    CHECK (replaced_by IS NULL OR is_archived);

CREATE INDEX idx_orientations_replaced_by
    ON orientations (replaced_by)
    WHERE replaced_by IS NOT NULL;
