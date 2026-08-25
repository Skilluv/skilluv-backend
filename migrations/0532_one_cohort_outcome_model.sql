-- One answer to "did this person finish a cohort".
--
-- Written when `feat/leadership-quality-domains` and
-- `feat/communication-education-domains` met. Both had, independently, refused
-- to create a parallel cohorts table and extended `cohorts` instead — which
-- was right, and is why the merge worked at all. Both then went on to record
-- completion in a different place.
--
--   * leadership put it on the membership: `cohort_members.graduated_at`,
--     with `left_at` and `leave_reason` beside it;
--   * education put it on a per-learner outcome row:
--     `education_learner_outcomes.completed`.
--
-- Two columns answering one question is a schema where "how many people
-- finished" has two answers, and nothing says which is right when they differ.
--
-- ## Which one survives, and why it is not the newer one
--
-- Membership. Finishing is a fact about somebody's participation, and
-- participation is `cohort_members` — the table that already knows when they
-- joined, and the one a person's own erasure runs through.
--
-- The outcome row keeps everything that is genuinely education's and has no
-- equivalent anywhere: the assessments before and after, the satisfaction
-- signal, and the testimonial with its consent. Those are about what changed
-- for a learner. Whether they finished is not.
--
-- ## The denominator, which is the part that actually mattered
--
-- `education_cohort_meets_threshold` counted completions over *the learners
-- somebody had recorded an outcome for*. A teacher who records outcomes only
-- for the people who finished scores a hundred per cent — the same failure as
-- a graduation rate computed over the survivors, one step removed.
--
-- It now counts over everybody who joined, which is what
-- `leadership_cohort_outcomes` already did and for the reason that view's
-- comment gives. The one carve-out survives with it: somebody who left because
-- they found work is removed from the denominator rather than counted as a
-- loss, because that is the outcome a training cohort exists to produce.
--
-- The condition education added and leadership did not is kept: at least one
-- learner outcome has to have been recorded. A cohort where nobody wrote down
-- what happened has not measured anything, whatever its graduation rate says.

-- ═══════════════════════════════════════════════════════════════════
-- The view stops belonging to one domain
-- ═══════════════════════════════════════════════════════════════════
--
-- `leadership_cohort_outcomes` is now read by two domains and describes
-- neither. A name that claims a domain is a name the second reader has to
-- explain every time.

ALTER VIEW leadership_cohort_outcomes RENAME TO cohort_outcomes;

COMMENT ON VIEW cohort_outcomes IS
    'Whether a cohort was led to its end, for whichever domain led it. The '
    'denominator is everybody who joined, not the survivors: a rate computed '
    'over survivors improves every time somebody gives up, which makes it '
    'reward the failure it should detect. People who left because they found '
    'work are removed from the denominator rather than counted as losses — '
    'that is the outcome the cohort existed for.';

-- ═══════════════════════════════════════════════════════════════════
-- The duplicate column goes
-- ═══════════════════════════════════════════════════════════════════
--
-- Carried across first. A row saying somebody completed, whose membership
-- does not say so, is the disagreement this migration exists to remove — and
-- the membership is what everything reads afterwards, so the fact has to
-- move there rather than be dropped with the column.
--
-- `left_at IS NULL` guards the constraint added by 0462: a member cannot have
-- both graduated and left. Where an outcome row says completed and the
-- membership says they left, the departure is kept: it carries a reason, and
-- a reason is a stronger record than a boolean.

UPDATE cohort_members m
   SET graduated_at = COALESCE(m.graduated_at, NOW())
  FROM education_learner_outcomes o
 WHERE o.cohort_id = m.cohort_id
   AND o.learner_user_id = m.user_id
   AND o.completed
   AND m.left_at IS NULL;

ALTER TABLE education_learner_outcomes DROP COLUMN completed;

COMMENT ON TABLE education_learner_outcomes IS
    'What changed for each learner in a taught cohort: the assessments, the '
    'satisfaction signal, the testimonial and its consent. Whether they '
    'finished is not here — that is a fact about their membership, and it '
    'lives on `cohort_members` where one model serves both domains that run '
    'cohorts.';

-- ═══════════════════════════════════════════════════════════════════
-- The threshold reads the membership
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION education_cohort_meets_threshold(_cohort_id UUID)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT
        -- Somebody wrote down what happened to at least one learner. A cohort
        -- with no outcome rows has not measured anything, whatever its
        -- graduation rate would have been. This is education's own condition
        -- and it is kept.
        EXISTS (SELECT 1 FROM education_learner_outcomes
                 WHERE cohort_id = _cohort_id)
        -- And it was led to its end, on the shared definition: concluded, at
        -- least three joined, and seventy per cent of the ones not lost to a
        -- job finished.
        AND COALESCE(
                (SELECT led_to_the_end FROM cohort_outcomes
                  WHERE cohort_id = _cohort_id),
                FALSE);
$$;

COMMENT ON FUNCTION education_cohort_meets_threshold(UUID) IS
    'Whether a cohort earns education_cohort_delivered: an outcome recorded '
    'for at least one learner, and the cohort led to its end on the shared '
    'definition in `cohort_outcomes`. A function rather than a constant in '
    'application code because it is a policy, and this platform keeps policy '
    'where an operator can read it — next to the review grids, the badge '
    'rules and the craft-score weights.';
