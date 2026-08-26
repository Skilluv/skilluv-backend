-- A unit of education work, on the slice table that already exists.
--
-- ## Six subtypes
--
-- Ticket W-01's list, plus one, because each is delivered differently and
-- reviewed against a different grid:
--
--   * `course_delivered` — a cohort or a course that ran, with proof.
--   * `curriculum_document` — a learning path, a programme, a skill matrix.
--   * `workshop_material` — a session's recording, slides and exercises.
--   * `lesson_plan_series` — structured plans another teacher can run.
--   * `assessment_framework` — the rubrics and criteria a programme is
--     graded against.
--   * `students_outcome_report` — what actually changed for the learners.
--
-- The backlog listed five and this is six. `assessment_framework` was in the
-- backlog as a *challenge* (O3-02) and as an attestation basis (F-07), with
-- no artefact type to carry either. It is not a curriculum: a curriculum says
-- what is taught, and a rubric says how anybody knows it landed — they are
-- written by the same person, read by different ones, and the second is the
-- one that decides what actually gets learned.
--
-- ## The learner-data declaration, and why it is a gate
--
-- This is the only domain whose artefacts routinely contain facts about
-- identifiable third parties who are not members here, are sometimes minors,
-- and never asked to be evidence in somebody's portfolio. A cohort report
-- naming students, a testimonial screenshot with a face, an assessment
-- spreadsheet: each is a delivery that cannot be published as submitted.
--
-- Migration 0410 met the same shape in audio and answered it the same way: a
-- provenance problem there does not make the work weaker, it makes it
-- unusable, so a composition is not attested until the author states the
-- source list is complete. Here the author states that no identifiable
-- learner remains, or that consent is on file for the ones who do.
--
-- It is a declaration rather than an inferred check for the reason 0410 gave:
-- a report with no names and a declaration, and a report nobody looked at,
-- have exactly the same row count. Those two must not read the same to
-- something about to publish it.
--
-- Two subtypes are gated. `curriculum_document`, `workshop_material`,
-- `lesson_plan_series` and `assessment_framework` are not: none of them
-- contains a learner by nature, and gating them would make the declaration a
-- formality people click through — which is how a gate stops meaning
-- anything.
--
-- ## Why `education_learners_count` is here and not attested
--
-- It describes the delivery, and a reader wants it: twelve people and two
-- hundred are different work. It feeds no attestation and no badge, because
-- nothing here can check it — 0521 and 0522 write out that argument. It is
-- the size of the room, stated by the person who was in it.
--
-- ## Revision rounds
--
-- Reusing `slice_revision_rounds` (0412) like communication does. Three
-- rounds, against communication's four and audio's five: a curriculum that
-- has been through objectives, progression and materials has been through
-- everything it has, and a fourth pass means the programme was for a
-- different audience than the brief said.

-- ═══════════════════════════════════════════════════════════════════
-- The slice type
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order) VALUES
    ('education_artifact', 'education', 'Education artefact',
     'A session, a curriculum, a set of lesson plans, a rubric or an outcome report, delivered and reusable.',
     116);

ALTER TABLE project_slices
    ADD COLUMN education_subtype VARCHAR(30),
    -- Who it was for. A workshop for absolute beginners and one for senior
    -- engineers are different work, and the review reads it.
    ADD COLUMN education_target_audience VARCHAR(20),
    -- How many people were in the room. Stated by the person who was there,
    -- attested by nothing, and read by everybody.
    ADD COLUMN education_learners_count INTEGER
        CHECK (education_learners_count IS NULL OR education_learners_count >= 0),
    -- The statement that nothing identifiable about a learner is in here, or
    -- that consent is on file for what is. Read by the attestation
    -- generators, which refuse to issue without it.
    ADD COLUMN education_learner_data_cleared_at TIMESTAMPTZ,
    ADD COLUMN education_learner_data_cleared_by UUID REFERENCES users(id) ON DELETE SET NULL;

COMMENT ON COLUMN project_slices.education_subtype IS
    'What kind of education artefact this is. Constrained here rather than in '
    'slice_types because the review grid, the attestation basis and the '
    'learner-data gate all branch on it.';

COMMENT ON COLUMN project_slices.education_learner_data_cleared_at IS
    'When the author stated no identifiable learner remains, or that consent '
    'is on file. A declaration rather than an inferred check, for the reason '
    'migration 0410 gave about audio sources: a report with no names and a '
    'declaration, and a report nobody looked at, have the same row count.';

COMMENT ON COLUMN project_slices.education_learners_count IS
    'The size of the room, stated by the person who was in it. Feeds no '
    'attestation and no badge: nothing here can check it.';

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_education_subtype_belongs_to_artifact CHECK (
        (slice_type = 'education_artifact' AND education_subtype IS NOT NULL)
        OR (slice_type <> 'education_artifact' AND education_subtype IS NULL)
    ),
    ADD CONSTRAINT project_slices_education_subtype_values CHECK (
        education_subtype IS NULL OR education_subtype IN (
            'course_delivered',
            'curriculum_document',
            'workshop_material',
            'lesson_plan_series',
            'assessment_framework',
            'students_outcome_report'
        )
    ),
    ADD CONSTRAINT project_slices_education_audience_values CHECK (
        education_target_audience IS NULL OR education_target_audience IN (
            'beginner', 'junior', 'mid', 'senior', 'mixed'
        )
    ),
    -- A declaration has an author and a date, or neither. Same shape as the
    -- audio one of 0410.
    ADD CONSTRAINT project_slices_education_declaration_has_an_author CHECK (
        (education_learner_data_cleared_at IS NULL)
        = (education_learner_data_cleared_by IS NULL)
    ),
    -- Something that ran says how many it ran for. Not a large ask, and its
    -- absence is the difference between a delivery and a claim.
    ADD CONSTRAINT project_slices_delivered_teaching_says_how_many CHECK (
        education_subtype IS NULL
        OR education_subtype NOT IN ('course_delivered', 'students_outcome_report')
        OR education_learners_count IS NOT NULL
    );

CREATE INDEX idx_project_slices_education_subtype
    ON project_slices (education_subtype)
    WHERE education_subtype IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Revision rounds
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO revision_round_kinds (slug, skill_domain, name, description, sort_order) VALUES
    ('edu_objectives_revision', 'education', 'Objectives',
     'What the learner will be able to do is missing, unobservable, or not what the programme actually goes after.', 410),
    ('edu_progression_revision', 'education', 'Progression',
     'A step is not reachable from the last one. The silent jump, which is this domain''s most common defect.', 420),
    ('edu_materials_revision', 'education', 'Materials',
     'Exercises, solutions, environment, facilitator notes: what another trainer would need and does not have.', 430),
    ('edu_learner_data_revision', 'education', 'Learner data',
     'A name, a face, a mark or a message that should not be there. Not a matter of degree: the delivery cannot be published as it stands.', 440),
    ('edu_brief_change', 'education', 'Brief change',
     'The commissioner changes the audience, the length or the objectives. Counted like the others, because the work is the same.', 450);

INSERT INTO revision_round_limits (skill_domain, max_rounds, rationale) VALUES
    ('education', 3,
     'Three. Objectives, progression, materials: a programme that has been through those has been through everything it has. A fourth pass means the audience was not the one the brief named, and that is a different commission rather than a revision.');
