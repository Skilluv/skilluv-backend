-- Who can review a translation, and the record that they did.
--
-- ## Why ticket W-04's answer is not a capability per language
--
-- W-04 asked for `communication_reviewer:translator-fr`,
-- `…:translator-pt`, `…:translator-sw`, and so on. It does not survive
-- contact with the problem:
--
--   * there are about seven thousand living languages, and this platform is
--     built for a continent where most of them are spoken. The catalogue
--     would grow a row every time somebody asks for one, and
--     `capability_catalog.capability` is thirty-two characters wide;
--   * a capability nobody has been granted is a gate nobody passes, so the
--     first translation into a new language could never be reviewed at all;
--   * and it says the wrong thing. The right to review translations is one
--     right, granted once — `communication_reviewer:translation`, derived by
--     0404 from the trade. Which language somebody can read is not a
--     permission, it is a fact about them.
--
-- So the capability stays as it is, and the language becomes a declaration.
--
-- ## Declared, not proven, and that is deliberate
--
-- Nothing in this codebase can test somebody's Swahili. Pretending otherwise
-- — a quiz, a score — would produce a number that looks like evidence and is
-- not. What a declaration does give is accountability: the reviewer said, in
-- writing and under their name, that they read this language well enough to
-- review in it, and every review they signed carries that claim next to it.
--
-- `proficiency` is recorded because "native" and "professional working" are
-- different claims and a maintainer choosing a reviewer for a legal notice
-- may want the first. It is not enforced anywhere: a rule saying only native
-- speakers may review would, in practice, mean nobody reviews Lingala.
--
-- ## Why the review is a row and not just an attestation
--
-- The attestation is issued to the *translator*. It says the work was
-- validated; it does not say by whom, in what language, or with what
-- reservations. That is what a reader needs when they want to weigh the
-- claim, and it is what a moderator needs when a translation turns out to be
-- wrong — a bad review has an author, and an attestation alone does not name
-- one.

CREATE TABLE user_review_languages (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- A BCP 47 tag: `fr`, `pt-BR`, `sw`, `ar`, `wo`. Not constrained against
    -- a list: a closed vocabulary here is a list of the languages the
    -- platform has decided exist, and this is the one place that would be
    -- read as a statement.
    language VARCHAR(20) NOT NULL
        CHECK (language ~ '^[a-zA-Z]{2,3}(-[a-zA-Z0-9]{2,8})*$'),
    proficiency VARCHAR(20) NOT NULL DEFAULT 'professional'
        CHECK (proficiency IN ('native', 'bilingual', 'professional')),
    -- What they said, in their own words. Optional, and the place somebody
    -- writes "I grew up with it but I read technical French better".
    note TEXT NOT NULL DEFAULT '',
    declared_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (user_id, language)
);

COMMENT ON TABLE user_review_languages IS
    'Languages somebody has declared they read well enough to review a '
    'translation in. A declaration rather than a capability: the right to '
    'review translations is one right (communication_reviewer:translation), '
    'and which language a person reads is a fact about them, not a '
    'permission. Ticket W-04 asked for one capability per language; there are '
    'seven thousand languages and the catalogue column is 32 characters.';

COMMENT ON COLUMN user_review_languages.proficiency IS
    'Recorded because native and professional-working are different claims. '
    'Not enforced anywhere: requiring native speakers would in practice mean '
    'nobody reviews Lingala.';

CREATE INDEX idx_review_languages_by_language
    ON user_review_languages (language, proficiency);

-- ═══════════════════════════════════════════════════════════════════
-- The review itself
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE translation_reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    reviewer_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    language VARCHAR(20) NOT NULL,
    notes_md TEXT NOT NULL DEFAULT '',
    -- The attestation this review produced, when it produced one. NULL when
    -- the artefact already carried it: re-reviewing is allowed and issues
    -- nothing twice.
    attestation_id UUID REFERENCES attestations(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One review per person per language per artefact. A second pass by the
    -- same reader in the same language is the same statement.
    UNIQUE (slice_id, reviewer_user_id, language)
);

COMMENT ON TABLE translation_reviews IS
    'Who validated a translation, in which language, and what they said. The '
    'attestation goes to the translator and does not name the reviewer; this '
    'does, which is what a reader weighing the claim and a moderator handling '
    'a bad review both need.';

CREATE INDEX idx_translation_reviews_slice
    ON translation_reviews (slice_id, reviewed_at DESC);

CREATE INDEX idx_translation_reviews_reviewer
    ON translation_reviews (reviewer_user_id, reviewed_at DESC);
