-- Paying a design mission as it is delivered, not once at the end.
--
-- ## Why the existing models do not cover it
--
-- Migration 0192 has `fixed_price`, `per_hour`, `per_deliverable`,
-- `retainer_monthly` and `revenue_share`. A brand identity is none of them:
-- it is one job, handed in three or four times, and the designer carries the
-- whole of it unpaid until the client is happy with the last round.
--
-- `per_deliverable` is the near miss — but a round is not a deliverable. It
-- is the same deliverable again, and calling four rounds four deliverables
-- would let a client pay four times for one job or a designer invoice four
-- times for one.
--
-- ## The split is on the mission, not in the code
--
-- The backlog's 20/20/20/40 is a reasonable default and a terrible rule:
-- some jobs front-load the work and some end in a week of production. The
-- ratio is agreed when the mission is posted and frozen with it, like the
-- commission.

ALTER TABLE missions
    DROP CONSTRAINT IF EXISTS missions_payment_model_check;

ALTER TABLE missions
    ADD CONSTRAINT missions_payment_model_check
    CHECK (payment_model IN (
        'fixed_price', 'per_hour', 'per_deliverable', 'retainer_monthly',
        'revenue_share',
        -- One job, paid as its rounds are accepted.
        'milestone_iteration'
    ));

-- What share of the budget each accepted round releases, in percent, in
-- order. `[20, 20, 20, 40]` is the default and nothing more than that.
ALTER TABLE missions
    ADD COLUMN IF NOT EXISTS milestone_split INTEGER[];

-- The shares add up to the whole job, every share is positive, and there are
-- between two and ten of them. A split that summed to ninety would leave a
-- tenth of the budget in escrow with nothing to release it, and nobody would
-- notice until the last mission of the year.
--
-- A function rather than the expression inline: a CHECK may not contain a
-- subquery, and summing an array needs one. `IMMUTABLE` is the honest label —
-- it reads nothing but its argument.
CREATE OR REPLACE FUNCTION milestone_split_is_whole(split INTEGER[])
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT split IS NULL
        OR (
            cardinality(split) BETWEEN 2 AND 10
            AND (SELECT bool_and(share > 0) FROM unnest(split) AS share)
            AND (SELECT sum(share) FROM unnest(split) AS share) = 100
        )
$$;

COMMENT ON FUNCTION milestone_split_is_whole(INTEGER[]) IS
    'Two to ten positive shares adding to a hundred. Exists because a CHECK '
    'may not contain a subquery and summing an array needs one.';

ALTER TABLE missions
    DROP CONSTRAINT IF EXISTS missions_milestone_split_is_whole;

ALTER TABLE missions
    ADD CONSTRAINT missions_milestone_split_is_whole
    CHECK (milestone_split_is_whole(milestone_split));

ALTER TABLE missions
    DROP CONSTRAINT IF EXISTS missions_milestone_split_belongs_to_its_model;

-- A split on a mission that is not paid by milestone is a number nobody
-- reads, and its absence on one that is would leave the model unimplementable
-- at the moment it matters.
ALTER TABLE missions
    ADD CONSTRAINT missions_milestone_split_belongs_to_its_model CHECK (
        (payment_model = 'milestone_iteration') = (milestone_split IS NOT NULL)
    );

COMMENT ON COLUMN missions.milestone_split IS
    'For `milestone_iteration`: the share of the budget each accepted round '
    'releases, in percent, in order. Agreed when the mission is posted and '
    'frozen with it, like the commission.';

-- ═══════════════════════════════════════════════════════════════════
-- What the commission was, and why
-- ═══════════════════════════════════════════════════════════════════

-- `commission_percent` is frozen when a mission is published, so that nobody
-- can move it afterwards. That is right, and it is also why the backlog's two
-- exceptions cannot both live at publication:
--
--   * a charity brief is a property of the mission, known when it is posted;
--   * a loyalty discount is a property of the *person who takes it*, and
--     nobody knows who that is yet.
--
-- So the rate is settled when the second party is known — at assignment — and
-- this column says which rule settled it. A rate with no reason is a rate
-- somebody will eventually argue about with nothing to point at.
ALTER TABLE missions
    ADD COLUMN IF NOT EXISTS commission_reason VARCHAR(30);

ALTER TABLE missions
    DROP CONSTRAINT IF EXISTS missions_commission_reason_check;

ALTER TABLE missions
    ADD CONSTRAINT missions_commission_reason_check
    CHECK (commission_reason IS NULL OR commission_reason IN (
        'standard',
        -- No commission at all. Skilluv does not take a cut of work given
        -- away.
        'charity_brief',
        -- Somebody who has delivered ten missions here. The platform costs
        -- the same to run for them and less to find them.
        'loyalty_discount'
    ));

-- Work posted by a cause rather than by a client. Declared by the enterprise
-- and visible on the mission: a claim made in public is a claim somebody can
-- contradict, which is the only check that costs nothing.
ALTER TABLE missions
    ADD COLUMN IF NOT EXISTS charity_brief BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN missions.charity_brief IS
    'Work posted by a cause. Commission is zero — Skilluv does not take a cut '
    'of work given away. Declared, and shown on the public mission so it can '
    'be contradicted.';
