-- A cohort that somebody teaches, what came out of it, and who else ran the
-- curriculum.
--
-- ## Why there is no `education_cohorts`
--
-- Ticket W-02 said to reuse `mentoring_cohorts` from the leadership backlog.
-- That table does not exist and the leadership domain is not open, so the
-- instruction could not be followed as written. What it was reaching for is
-- right, and the table it should have named is `cohorts` — built by migration
-- 0221, with members, milestones, a window, a size cap and an orientation.
--
-- This database already holds three things called a cohort:
--
--   * `cohorts` — a group of people working through something together;
--   * `academy_cohorts` — an enterprise sponsoring a group it intends to hire
--     from, which is a commercial arrangement wrapped around one;
--   * `tenant_cohorts` — the same idea inside a white-labelled deployment.
--
-- A fourth would be the point at which nobody can answer "how many cohorts
-- has this person run" without asking which table. So `cohorts` gains two
-- columns and keeps its meaning: a peer study group is one with no teacher,
-- and a taught cohort is one with a teacher. Every existing row is the first
-- kind, and nothing about them changes.
--
-- ## What makes a taught cohort different, in one column
--
-- `led_by_user_id`. Somebody is answerable for whether the room arrived,
-- which is exactly what 0517 said separates this domain from mentoring and
-- from a study group. The attestation generator reads it: a cohort with no
-- teacher attests nobody.
--
-- ## Outcomes, and why they are not a JSONB blob on the cohort
--
-- Ticket W-03 asked for `pre_assessment_json` and `post_assessment_json` per
-- learner. The shape is right and the storage is not: a per-learner row is
-- what lets a completion rate be a count rather than a number somebody typed,
-- and it is what lets a single learner exercise erasure without rewriting a
-- document about nineteen other people.
--
-- The assessments themselves stay JSONB, because what is assessed differs per
-- programme and a schema for it would be wrong within a month.
--
-- ## Why the outcome row does not name the learner in the artefact
--
-- It names them here, in a table the learner controls through the platform's
-- own erasure path, and the *artefact* — the report somebody publishes — is
-- gated by the declaration migration 0523 added. Those are two different
-- records with two different audiences, and collapsing them is how a cohort
-- report ends up on a public profile with twenty names in it.
--
-- ## Adoption, and why it is the fact worth attesting
--
-- Ticket W-04 asked for `education_curriculum_adoptions` and it is right:
-- publishing a learning path is easy, and having another trainer run it is
-- the thing that says it worked. `education_curriculum_authored` (0521) rests
-- on this table rather than on the act of publishing.

-- ═══════════════════════════════════════════════════════════════════
-- A cohort can have a teacher
-- ═══════════════════════════════════════════════════════════════════

-- The three columns this migration used to add — `led_by_user_id`,
-- `curriculum_slice_id`, `concluded_at` — are added by migration 0462
-- instead, when the two branches met.
--
-- The reasoning above was written before the leadership domain existed and it
-- was right: the header says "the table it should have named is `cohorts`",
-- and 0462 reached the same conclusion from the other side, in the same words
-- about not creating a fourth thing called a cohort. Two branches independently
-- refusing the same parallel table is the strongest evidence either of them
-- had that it was the wrong table to create.
--
-- So the columns arrive once, at 0462. The index this migration created is
-- 0462's `idx_cohorts_led_by`, identical in every respect; only the
-- curriculum index below is new.
--
-- What remains genuinely this domain's is everything after it: the per-learner
-- outcome rows, and the adoptions.

COMMENT ON COLUMN cohorts.led_by_user_id IS
    'Whoever is answerable for this cohort — an educator here, a mentor in the '
    'leadership domain — or NULL for a peer study group with neither. Both '
    'attestation generators read it, and both refuse a cohort that has none: '
    'a run nobody was answerable for attests nobody.';

CREATE INDEX idx_cohorts_by_curriculum
    ON cohorts (curriculum_slice_id)
    WHERE curriculum_slice_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What came out of it, per learner
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE education_learner_outcomes (
    cohort_id UUID NOT NULL REFERENCES cohorts(id) ON DELETE CASCADE,
    learner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- What was assessed, before and after. JSONB because what is assessed
    -- differs per programme, and a schema for it would be wrong inside a
    -- month.
    pre_assessment JSONB NOT NULL DEFAULT '{}'
        CHECK (jsonb_typeof(pre_assessment) = 'object'),
    post_assessment JSONB NOT NULL DEFAULT '{}'
        CHECK (jsonb_typeof(post_assessment) = 'object'),
    -- Whether they finished. The completion rate is a count of these rather
    -- than a percentage somebody typed, which is the entire reason this table
    -- is per learner.
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    -- One to five, and read as what it is: a signal about whether people come
    -- back, never as evidence that anybody learned. The review grid of 0520
    -- says the same thing in the same words.
    satisfaction SMALLINT CHECK (satisfaction IS NULL OR satisfaction BETWEEN 1 AND 5),
    -- What the learner is willing to have quoted, and only what they wrote
    -- themselves. Empty by default, because the default has to be silence.
    testimonial_md TEXT NOT NULL DEFAULT '',
    -- Consent for the testimonial to leave this table. Without it the text
    -- exists and is quotable by nobody.
    testimonial_consent_at TIMESTAMPTZ,
    recorded_by UUID REFERENCES users(id) ON DELETE SET NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (cohort_id, learner_user_id),

    -- A quoted testimonial has consent, or it is not quoted. Enforced rather
    -- than trusted: this is the row most likely to be read by something that
    -- publishes.
    CONSTRAINT a_testimonial_carries_its_consent CHECK (
        btrim(testimonial_md) = '' OR testimonial_consent_at IS NOT NULL
    )
);

COMMENT ON TABLE education_learner_outcomes IS
    'What changed for each learner in a taught cohort. Per learner rather '
    'than a document on the cohort, so a completion rate is a count rather '
    'than a typed number, and so one learner can exercise erasure without '
    'rewriting a report about nineteen others.';

COMMENT ON COLUMN education_learner_outcomes.satisfaction IS
    'A signal about whether people come back. Never evidence that anybody '
    'learned — the review grid of 0520 names the same distinction.';

CREATE INDEX idx_learner_outcomes_by_learner
    ON education_learner_outcomes (learner_user_id, recorded_at DESC);

CREATE TRIGGER trg_learner_outcomes_updated_at
    BEFORE UPDATE ON education_learner_outcomes
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- Who else ran the curriculum
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE education_curriculum_adoptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The published curriculum artefact.
    curriculum_slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    -- The trainer who ran it. Never its author: an adoption is by definition
    -- somebody else, and counting the author would make every curriculum
    -- adopted once on the day it was published.
    adopter_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The cohort they ran it for, when it was on this platform. NULL when it
    -- was run elsewhere, which is the common case and still worth recording.
    cohort_id UUID REFERENCES cohorts(id) ON DELETE SET NULL,
    -- What they changed and what they would change. The part the author reads.
    feedback_md TEXT NOT NULL DEFAULT '',
    adopted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One adoption per trainer per curriculum. Running it three times is the
    -- same statement about the curriculum.
    UNIQUE (curriculum_slice_id, adopter_user_id)
);

COMMENT ON TABLE education_curriculum_adoptions IS
    'Trainers other than the author who have run a published curriculum. '
    'What education_curriculum_authored rests on: publishing a learning path '
    'is easy, and somebody else running it is the fact worth attesting.';

CREATE INDEX idx_curriculum_adoptions_by_curriculum
    ON education_curriculum_adoptions (curriculum_slice_id, adopted_at DESC);

CREATE INDEX idx_curriculum_adoptions_by_adopter
    ON education_curriculum_adoptions (adopter_user_id, adopted_at DESC);

-- The author cannot adopt their own curriculum. A CHECK cannot see the
-- artefact's author, so it is a trigger — and it is enforced rather than left
-- to the endpoint, because the count feeds an attestation.
CREATE OR REPLACE FUNCTION trg_adoption_is_by_somebody_else()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM deliverables d
         WHERE d.slice_id = NEW.curriculum_slice_id
           AND d.user_id = NEW.adopter_user_id
    ) THEN
        RAISE EXCEPTION
            'a curriculum is not adopted by the person who wrote it';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_curriculum_adoption_is_by_somebody_else
    BEFORE INSERT OR UPDATE ON education_curriculum_adoptions
    FOR EACH ROW EXECUTE FUNCTION trg_adoption_is_by_somebody_else();
