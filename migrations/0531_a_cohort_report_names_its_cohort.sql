-- What links a delivered-course artefact to the cohort it reports on, and the
-- threshold that decides whether it earns an attestation.
--
-- ## Why the link is needed at all
--
-- `education_cohort_delivered` (0521) claims that a cohort ran to the end
-- with measured outcomes. An attestation rests on a deliverable — a
-- `course_delivered` slice — and nothing so far connects that artefact to the
-- cohort whose completion rate would prove the claim.
--
-- Without the link the generator would have to guess: same author, overlapping
-- dates, similar title. Every one of those is wrong sometimes, and an
-- attestation issued on a guess is the failure mode this whole subsystem
-- exists to avoid. One column, stated by the person who ran it.
--
-- ## Why the threshold is a function and not a constant in Rust
--
-- Because it is a policy, and this platform puts policy in rows or in the
-- schema where an operator can see it. Review grids, badge rules,
-- craft-score weights, round ceilings and mission commissions are all
-- editable without a deployment; a number that decides whether somebody's
-- eight weeks of teaching is attestable belongs in the same category.
--
-- A function rather than a table because it is one rule with two parts —
-- enough people finished, and somebody recorded what happened to them — and a
-- table with one row is a constant with extra steps.
--
-- ## Why seventy per cent
--
-- The backlog's figure, and it is defensible in both directions. Lower and
-- the attestation stops meaning the cohort worked; higher and it punishes the
-- programmes that take the people most likely to drop out — which, on a
-- platform built for career changers, are the ones worth running.
--
-- What the threshold measures is *recorded* completion. A cohort where
-- fifteen of twenty finished and nobody wrote it down fails, and correctly:
-- the claim is that outcomes were measured, and unmeasured outcomes are the
-- thing this domain most often substitutes an impression for.

ALTER TABLE project_slices
    -- The cohort this artefact reports on. Only meaningful on a
    -- `course_delivered` slice, and stated rather than inferred.
    ADD COLUMN education_cohort_id UUID REFERENCES cohorts(id) ON DELETE SET NULL;

COMMENT ON COLUMN project_slices.education_cohort_id IS
    'The cohort a delivered-course artefact reports on. Stated by the person '
    'who ran it: matching on author and dates is wrong often enough that an '
    'attestation issued on the guess would be worthless.';

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_only_a_delivery_names_a_cohort CHECK (
        education_cohort_id IS NULL
        OR education_subtype = 'course_delivered'
    );

CREATE INDEX idx_project_slices_education_cohort
    ON project_slices (education_cohort_id)
    WHERE education_cohort_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Did it work?
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION education_cohort_meets_threshold(_cohort_id UUID)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT
        -- Somebody wrote down what happened to at least one learner. A cohort
        -- with no outcome rows has not measured anything, whatever its
        -- completion rate would have been.
        count(*) > 0
        -- And enough of them finished. Counted from the rows rather than from
        -- a percentage anybody typed, which is the entire reason
        -- `education_learner_outcomes` is per learner.
        AND count(*) FILTER (WHERE completed)::NUMERIC / count(*) >= 0.70
      FROM education_learner_outcomes
     WHERE cohort_id = _cohort_id;
$$;

COMMENT ON FUNCTION education_cohort_meets_threshold(UUID) IS
    'Whether a cohort earns education_cohort_delivered: outcomes recorded for '
    'at least one learner, and at least 70% of those recorded completed. A '
    'function rather than a constant in application code because it is a '
    'policy, and this platform keeps policy where an operator can read it — '
    'next to the review grids, the badge rules and the craft-score weights.';
