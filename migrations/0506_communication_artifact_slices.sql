-- A unit of communication work, on the slice table that already exists.
--
-- ## Six subtypes
--
-- The list is ticket W-01's, unchanged, because each of the six is delivered
-- differently and reviewed against a different grid:
--
--   * `documentation` — a change to somebody's documentation, delivered as a
--     pull request.
--   * `devrel_talk` — an intervention, delivered as a recording plus slides.
--   * `blog_post` — a written piece at a public address.
--   * `video_content` — a video, an episode or a stream recording.
--   * `translation` — a documentation set carried into another language.
--   * `research_paper` — a whitepaper, a report or an external specification.
--
-- ## `external_publish_url` is not added
--
-- Ticket W-01 asked for one. Migration 0435 removed the third column of that
-- shape and named the pattern: `code_package_registry_url`,
-- `ai_external_hosting_url` and `design_external_url` were three columns
-- answering one question, each written by somebody who had read the previous
-- one. `published_artifact_url` is that question, and it is what a published
-- article, video or paper fills in.
--
-- What is domain-specific is *when it is compulsory*, and 0435 said that
-- stays per domain. Here: everything except a documentation change and a
-- translation, both of which land in somebody else's repository and are
-- traced by `pr_url` instead.
--
-- ## Languages are an array, and the source language is separate
--
-- A translation carries both: the language it came from and the ones it went
-- to. One array holding all of them would make "translated into French" and
-- "translated from French" the same row, and the review families are chosen
-- by the target — a reviewer for the Swahili version has to read Swahili.
--
-- The array is on the slice rather than in a join table because nothing ever
-- queries "every slice targeting Wolof" across the platform; what is read is
-- one slice's languages, and the badge that counts distinct targets for one
-- person. A GIN index covers both.
--
-- ## Revision rounds
--
-- Ticket W-02 asked for an iteration workflow reusing design's. It is
-- reusable, and 0412 already generalised it: `slice_revision_rounds` with
-- per-domain kinds and a per-domain ceiling. Four rounds here, against
-- audio's five, because the fourth round on a written piece is where the
-- editing stops being editing and starts being a different commission.

-- ═══════════════════════════════════════════════════════════════════
-- The slice type
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order) VALUES
    ('communication_artifact', 'communication', 'Communication artefact',
     'A page, a talk, a video, a translation or a piece of research writing, delivered and publishable.',
     115);

ALTER TABLE project_slices
    ADD COLUMN communication_subtype VARCHAR(30),
    -- Where the work went, in BCP 47 tags. Empty for everything that is not a
    -- translation.
    ADD COLUMN communication_target_languages TEXT[] NOT NULL DEFAULT '{}',
    -- Where it came from. Separate from the targets on purpose: the review
    -- family is chosen by the target, and one array holding both would make
    -- "translated into French" indistinguishable from "translated from
    -- French".
    ADD COLUMN communication_source_language VARCHAR(20);

COMMENT ON COLUMN project_slices.communication_subtype IS
    'What kind of communication artefact this is. Constrained here rather '
    'than in slice_types because the review grid, the attestation basis and '
    'the publication obligation all branch on it.';

COMMENT ON COLUMN project_slices.communication_target_languages IS
    'BCP 47 tags a translation was carried into. The badge that counts '
    'distinct target languages reads this, which is why it is an array on the '
    'slice rather than free text in the description.';

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_communication_subtype_belongs_to_artifact CHECK (
        (slice_type = 'communication_artifact' AND communication_subtype IS NOT NULL)
        OR (slice_type <> 'communication_artifact' AND communication_subtype IS NULL)
    ),
    ADD CONSTRAINT project_slices_communication_subtype_values CHECK (
        communication_subtype IS NULL OR communication_subtype IN (
            'documentation',
            'devrel_talk',
            'blog_post',
            'video_content',
            'translation',
            'research_paper'
        )
    ),
    -- A translation says which way it went. Without it the artefact claims a
    -- language pair nobody can check, and the reviewer matching has nothing
    -- to match on.
    ADD CONSTRAINT project_slices_translation_states_its_languages CHECK (
        communication_subtype IS DISTINCT FROM 'translation'
        OR (communication_source_language IS NOT NULL
            AND array_length(communication_target_languages, 1) >= 1)
    ),
    -- Languages belong to a translation. A blog post tagged `fr` is not
    -- wrong, it is a different claim, and letting it through would make the
    -- polyglot badge countable from prose.
    ADD CONSTRAINT project_slices_languages_belong_to_a_translation CHECK (
        communication_subtype IS NOT DISTINCT FROM 'translation'
        OR (communication_source_language IS NULL
            AND communication_target_languages = '{}')
    ),
    -- What is published says where. Documentation changes and translations
    -- land in somebody else's repository and are traced by `pr_url`; the
    -- other four exist at an address or do not exist.
    ADD CONSTRAINT project_slices_published_communication_says_where CHECK (
        communication_subtype IS NULL
        OR communication_subtype IN ('documentation', 'translation')
        OR published_artifact_url IS NOT NULL
    );

CREATE INDEX idx_project_slices_communication_subtype
    ON project_slices (communication_subtype)
    WHERE communication_subtype IS NOT NULL;

CREATE INDEX idx_project_slices_communication_languages
    ON project_slices USING GIN (communication_target_languages);

-- ═══════════════════════════════════════════════════════════════════
-- Revision rounds
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revision_round_kinds (slug, skill_domain, name, description, sort_order) VALUES
    ('comm_accuracy_review', 'communication', 'Technical accuracy',
     'A claim is wrong, an example does not run, a version moved. The round never to skip.', 310),
    ('comm_structure_revision', 'communication', 'Structure',
     'The plan does not hold: the order, the headings, what is missing and what is said twice.', 320),
    ('comm_language_review', 'communication', 'Language',
     'Linguistic review: phrasing, register, consistency of terms. On a translation, done by somebody who reads both languages.', 330),
    ('comm_editorial_polish', 'communication', 'Editorial polish',
     'The cut, the pace, the title. What is left once the substance is right.', 340),
    ('comm_brief_change', 'communication', 'Brief change',
     'The commissioner changes what they are asking for. Counted like the others, because the work is the same.', 350);

INSERT INTO revision_round_limits (skill_domain, max_rounds, rationale) VALUES
    ('communication', 4,
     'Four. Accuracy, structure, language, polish: the complete journey of a text. Beyond that it is not editing any more, it is a different commission — and on a written piece, the fourth rewrite is where the author stops being the author.');
