-- Two more things a contest format knows about itself.
--
-- Migration 0516 moved the formats out of a CHECK and into `tournament_kinds`,
-- carrying what the Rust constants `KINDS_WITH_SUBMISSIONS` and
-- `MEASURED_KINDS` held. Two constants of the same shape survived it in
-- `services/contest.rs` — `JURIED_KINDS` and `COMMUNITY_VOTED_KINDS`, written
-- for the design contests — and they are the same bug one file further along:
-- a list of formats maintained by hand, which the next domain to add a format
-- has to find and edit.
--
-- ## Why these two are not one column
--
-- A hackathon is judged by a panel. A duel is judged by whoever shows up. A
-- brief contest can be either, and its `rules` say which — so the two are
-- independent, and a single `judging_mode` would have to invent a third value
-- meaning "ask the rules", which is what the code already does by reading
-- them.

ALTER TABLE tournament_kinds
    -- A panel decides. Invitations, competence checks and a deadline for the
    -- jury follow from this.
    ADD COLUMN is_juried BOOLEAN NOT NULL DEFAULT FALSE,
    -- Whoever shows up decides. A format that allows it can still be run with
    -- a jury when its rules say so; the column is what the default reads.
    ADD COLUMN allows_community_vote BOOLEAN NOT NULL DEFAULT FALSE,

    -- A format decided by both at once is a ranking nobody can reproduce:
    -- which of the two answers wins would be decided by whichever query ran
    -- last.
    ADD CONSTRAINT a_format_is_not_judged_two_ways CHECK (
        NOT (is_juried AND allows_community_vote)
    );

COMMENT ON COLUMN tournament_kinds.is_juried IS
    'A panel decides. What the design branch held in JURIED_KINDS, moved here '
    'so a new format is a row rather than an edit to a constant somebody has '
    'to find.';

COMMENT ON COLUMN tournament_kinds.allows_community_vote IS
    'Whoever shows up decides, by default. A contest whose rules name a '
    'voting mode overrides this, which is why the column is a default rather '
    'than a permission.';

UPDATE tournament_kinds SET is_juried = TRUE
 WHERE slug IN ('hackathon', 'tdd_contest', 'brief_contest');

UPDATE tournament_kinds SET allows_community_vote = TRUE
 WHERE slug = 'duel';
