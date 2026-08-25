-- Paid education work, on the mission table that already exists.
--
-- ## Why there is no `education_missions`
--
-- Ticket M-01 asked for one. Migration 0301 refused that for AI, 0413 for
-- audio and 0509 for communication; the reason has not changed. `missions` is
-- keyed by `skill_domain` and already carries the applications, the payment
-- models, the IP terms, the commission and the state machine. A second table
-- means a second answer to "how many missions has this person finished", and
-- both get quoted.
--
-- The fields the ticket listed:
--
--   * `target_audience` — the one genuinely new column, added below. Junior,
--     senior or mixed is not a skill level of the *worker*, which is what
--     every other column on `missions` describes, and there was nothing to
--     hold it.
--   * `students_count_expected` — `missions.target_learners`, added below
--     with it. A commission for twelve people and one for two hundred are
--     different work and different money.
--   * `duration_hours` — `missions.estimated_days` exists and is the wrong
--     unit for this domain: a three-hour workshop is not a fraction of a day
--     of work, it is a day of work that produces three hours of delivery.
--     `delivery_hours` says how much delivery is being bought, and the
--     existing column keeps saying how long it takes to make.
--   * `deliverable_format` — `mission_deliverable_formats`, a table since
--     0413. Five rows are added below.
--
-- ## Per-hour pricing was already there
--
-- Ticket M-03 asked for `per_hour` and `per_cohort`. The first is a value of
-- `missions.payment_model` since 0192. The second is `fixed_price` with the
-- cohort as the deliverable, and a synonym for it would mean two ways to
-- express one deal and two branches in the payout code — the argument 0413
-- made about royalties.

ALTER TABLE missions
    -- Who is being taught. Not a level of the person doing the work, which is
    -- what every other level column on this table means.
    ADD COLUMN target_audience VARCHAR(20)
        CHECK (target_audience IS NULL OR target_audience IN (
            'beginner', 'junior', 'mid', 'senior', 'mixed'
        )),
    -- How many. A commission for twelve and one for two hundred are different
    -- work and different money.
    ADD COLUMN target_learners INTEGER
        CHECK (target_learners IS NULL OR target_learners > 0),
    -- How much delivery is being bought, in hours in front of people.
    -- Distinct from `estimated_days`, which says how long the work takes: a
    -- three-hour workshop is not three eighths of a day of work.
    ADD COLUMN delivery_hours INTEGER
        CHECK (delivery_hours IS NULL OR delivery_hours > 0);

COMMENT ON COLUMN missions.target_audience IS
    'Who is being taught. Education only. Not a level of the person doing the '
    'work, which is what every other level column on this table means.';

COMMENT ON COLUMN missions.delivery_hours IS
    'Hours in front of people. Distinct from estimated_days, which says how '
    'long the work takes — a three-hour workshop is a week of preparation.';

-- An education mission says who it is for. Without it an applicant cannot
-- tell whether they are the right trainer, and the matching has nothing to
-- match on.
ALTER TABLE missions
    ADD CONSTRAINT missions_education_states_its_audience CHECK (
        skill_domain <> 'education' OR target_audience IS NOT NULL
    );

INSERT INTO mission_deliverable_formats (slug, skill_domain, name, description, sort_order) VALUES
    ('cohort_run', 'education', 'Cohort delivered',
     'A cohort run to the end, with its completion rate and its measured outcomes handed over.', 410),
    ('workshop_package', 'education', 'Workshop and materials',
     'The session delivered, plus slides, exercises, solutions and the environment another trainer would need.', 420),
    ('curriculum_package', 'education', 'Curriculum',
     'The programme, its objectives, its sequencing and its assessment, in a state somebody else can run.', 430),
    ('assessment_framework', 'education', 'Assessment framework',
     'Rubrics and criteria specific enough that two assessors reach the same grade.', 440),
    ('recorded_course', 'education', 'Recorded course',
     'A self-paced course, its exercises and its solutions, delivered as files rather than as a link to a platform.', 450);

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order) VALUES
    ('edu_cohort_delivery', 'education', 'Cohort delivery',
     'Running a cohort end to end: sessions, follow-up, assessment, outcomes.', 410),
    ('edu_workshop_corporate', 'education', 'Corporate workshop',
     'A session or a short series inside an organisation, on their tooling and their constraints.', 420),
    ('edu_curriculum_authoring', 'education', 'Curriculum authoring',
     'Designing a programme somebody else will run: objectives, sequencing, assessment.', 430),
    ('edu_bootcamp_module', 'education', 'Bootcamp module',
     'One module of an existing programme, taught to that programme''s standard and calendar.', 440),
    ('edu_recorded_course_production', 'education', 'Recorded course production',
     'A self-paced course: script, recording, exercises, solutions.', 450),
    ('edu_assessment_design', 'education', 'Assessment design',
     'Rubrics, tests and project briefs that measure what a programme claims to teach.', 460);
