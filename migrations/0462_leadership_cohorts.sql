-- Cohorts, which already exist.
--
-- ## The table the backlog asked for, and why it is not created
--
-- The backlog (leadership/W-05) asks for `mentoring_cohorts(id,
-- mentor_user_id, curriculum_slice_id, start_at, end_at, target_domain,
-- sub_orientations_targeted, mentees_max, mentees_current)` and
-- `mentoring_cohort_members(cohort_id, mentee_user_id, joined_at,
-- graduated_at, dropped_at, reason)`.
--
-- Nine of those columns already exist, on `cohorts` and `cohort_members`,
-- which have carried community study groups since migration 0269 — with a
-- slug, an orientation, a window, a member cap, milestones and a message
-- board. A parallel table would have meant two cohort listings, two join
-- flows, two notification paths and two places for somebody's membership to
-- exist.
--
-- What is genuinely missing is four things, and they are added below:
--
--   * who **leads** it. `cohort_members.role` has `organizer`, which is a
--     permission rather than a claim, and an attestation cannot rest on a
--     row anybody in the cohort could have been given;
--   * the **curriculum** it runs on, as a leadership artefact;
--   * whether somebody **finished**. `cohort_members` records joining and
--     nothing else, so today a cohort where everybody left on day three and
--     one where everybody graduated are the same rows;
--   * `mentees_current`, which is not added, because it is `count(*)` and a
--     stored copy of a count is a stored copy that goes wrong.
--
-- ## Why graduation is a column and not a badge
--
-- `leadership_cohort_completed` rests on most of the people who joined having
-- finished. Without a graduation column that claim cannot be made at all, and
-- with a self-declared one it means nothing — so it is set by the lead, and
-- the lead is not a member.

ALTER TABLE cohorts
    -- Who is answerable for it. Distinct from `created_by`, which records who
    -- typed the form, and from `cohort_members.role = 'organizer'`, which is
    -- a permission somebody can hold without leading anything.
    ADD COLUMN led_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    -- The curriculum this cohort runs on, when there is one. A leadership
    -- artefact like any other: written, reviewed, and attributable.
    ADD COLUMN curriculum_slice_id UUID REFERENCES project_slices(id) ON DELETE SET NULL,
    -- Which domain the cohort is aimed at. A cohort can teach game
    -- development while being led by somebody whose trade is mentoring, and
    -- `orientation_id` alone cannot say both.
    ADD COLUMN target_domain VARCHAR(30)
        REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    -- Closed by the lead when the run is over, whatever the outcome. What
    -- separates "finished" from "abandoned and never touched again", which
    -- an `ends_at` in the past cannot.
    ADD COLUMN concluded_at TIMESTAMPTZ,
    ADD COLUMN conclusion_note TEXT;

COMMENT ON COLUMN cohorts.led_by_user_id IS
    'Who is answerable for the cohort. Distinct from `created_by`, which is '
    'who typed the form, and from `cohort_members.role = organizer`, which is '
    'a permission somebody can hold without leading anything — an attestation '
    'cannot rest on a row anybody in the cohort could have been given.';

COMMENT ON COLUMN cohorts.concluded_at IS
    'Set by the lead when the run is over. `ends_at` in the past means the '
    'planned window closed, which is not the same as somebody having brought '
    'it to an end.';

CREATE INDEX idx_cohorts_led_by
    ON cohorts (led_by_user_id, starts_at DESC)
    WHERE led_by_user_id IS NOT NULL;

CREATE INDEX idx_cohorts_running
    ON cohorts (ends_at)
    WHERE concluded_at IS NULL AND archived_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Whether the people who joined finished
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE cohort_members
    ADD COLUMN graduated_at TIMESTAMPTZ,
    ADD COLUMN left_at TIMESTAMPTZ,
    -- Why they left. Required when they did, and not to police anybody: a
    -- cohort losing four people to "the schedule did not work" and one losing
    -- four to "the curriculum assumed knowledge I did not have" are different
    -- facts about the lead, and only the second is theirs to act on.
    ADD COLUMN leave_reason VARCHAR(30)
        CHECK (leave_reason IS NULL OR leave_reason IN (
            'schedule',        -- could not make the sessions
            'level_mismatch',  -- the curriculum assumed something they did not have
            'personal',        -- said, and not enquired into further
            'found_work',      -- the outcome the cohort existed for, arriving early
            'inactive',        -- stopped showing up, no reason given
            'other'
        )),
    ADD COLUMN leave_note TEXT;

ALTER TABLE cohort_members
    ADD CONSTRAINT a_member_finished_or_left_not_both CHECK (
        graduated_at IS NULL OR left_at IS NULL
    ),
    ADD CONSTRAINT leaving_says_why CHECK (
        left_at IS NULL OR leave_reason IS NOT NULL
    ),
    -- `other` says which, in words.
    ADD CONSTRAINT an_other_reason_says_which CHECK (
        leave_reason IS DISTINCT FROM 'other'
        OR (leave_note IS NOT NULL AND btrim(leave_note) <> '')
    );

COMMENT ON COLUMN cohort_members.leave_reason IS
    'Why somebody left. Not to police them: four people leaving because the '
    'schedule did not work and four leaving because the curriculum assumed '
    'knowledge they did not have are different facts about the lead, and only '
    'the second is theirs to act on.';

CREATE INDEX idx_cohort_members_graduated
    ON cohort_members (cohort_id)
    WHERE graduated_at IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Whether a cohort was actually led to the end
-- ═══════════════════════════════════════════════════════════════════
--
-- A view, for the same reason the retrospective follow-through is one: a
-- stored rate is wrong from the next graduation onwards.
--
-- Seventy per cent is the backlog's threshold and it is kept. The denominator
-- is what needed a decision, and it is **everybody who joined**, including the
-- people who left. A rate computed over the survivors is a rate that improves
-- every time somebody gives up, which would make the number reward exactly
-- the failure it is meant to detect.
--
-- The one exception is `found_work`: somebody who left a cohort because the
-- cohort worked is not a loss, and counting them as one would penalise the
-- outcome the whole thing exists for.

CREATE VIEW leadership_cohort_outcomes AS
SELECT c.id AS cohort_id,
       c.slug,
       c.led_by_user_id,
       c.curriculum_slice_id,
       c.target_domain,
       c.starts_at,
       c.ends_at,
       c.concluded_at,
       count(m.user_id) FILTER (WHERE m.role = 'member') AS joined_total,
       count(m.user_id) FILTER (
           WHERE m.role = 'member' AND m.graduated_at IS NOT NULL
       ) AS graduated_total,
       count(m.user_id) FILTER (
           WHERE m.role = 'member' AND m.left_at IS NOT NULL
             AND m.leave_reason = 'found_work'
       ) AS left_for_work,
       -- What `leadership_cohort_completed` rests on: the cohort was brought
       -- to an end, at least three people joined, and seventy per cent of the
       -- ones who were not lost to a job finished it.
       (c.concluded_at IS NOT NULL
        AND count(m.user_id) FILTER (WHERE m.role = 'member') >= 3
        AND (count(m.user_id) FILTER (WHERE m.role = 'member')
             - count(m.user_id) FILTER (
                   WHERE m.role = 'member' AND m.leave_reason = 'found_work')) > 0
        AND count(m.user_id) FILTER (
                WHERE m.role = 'member' AND m.graduated_at IS NOT NULL
            )::NUMERIC
            / NULLIF(count(m.user_id) FILTER (WHERE m.role = 'member')
                     - count(m.user_id) FILTER (
                           WHERE m.role = 'member' AND m.leave_reason = 'found_work'), 0)
            >= 0.70
       ) AS led_to_the_end
  FROM cohorts c
  LEFT JOIN cohort_members m ON m.cohort_id = c.id
 GROUP BY c.id;

COMMENT ON VIEW leadership_cohort_outcomes IS
    'Whether a cohort was led to the end. The denominator is everybody who '
    'joined, not the survivors: a rate computed over survivors improves every '
    'time somebody gives up. People who left because they found work are '
    'removed from the denominator rather than counted as losses — that is the '
    'outcome the cohort exists for.';
