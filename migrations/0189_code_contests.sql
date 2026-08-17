-- Contests where the answer is a piece of code.
--
-- ## What was already there, and what was missing
--
-- `tournaments` already knew about hackathons and marathons, and could rank
-- participants by a score an admin typed in. Three things stopped it holding
-- a code contest:
--
--   1. Nothing said what the contest asked for. A code golf on Python and a
--      TDD contest on the same problem are different events, and the only
--      place to record the difference was the free-text description.
--   2. Nothing held what a participant submitted. The score arrived by an
--      admin calling an endpoint, with no record of what it was a score of.
--   3. Ranking assumed higher is better. In code golf, it is not — and a
--      leaderboard that crowns the longest solution is worse than none.
--
-- ## Why `hackathon_code` is not a new kind
--
-- The backlog names three formats: hackathon_code, code_golf, tdd_contest.
-- The first is the hackathon that already exists, held on code. Adding it as
-- a fourth kind would mean a design hackathon and a code hackathon share no
-- machinery while differing only in subject. A domain on the tournament says
-- the same thing and keeps one hackathon.

ALTER TABLE tournaments
    DROP CONSTRAINT tournaments_kind_check;

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_kind_check CHECK (kind IN (
        'individual',
        'guild_war',
        'hackathon',
        'marathon',
        'defi_solitaire',
        -- Shortest working solution to a stated problem, one language at a
        -- time. Weekly, and deliberately unserious.
        'code_golf',
        -- Same problem for everybody, judged on the tests as much as on the
        -- code that passes them.
        'tdd_contest'
    ));

ALTER TABLE tournaments
    -- NULL means "any domain" — a general hackathon stays possible.
    ADD COLUMN skill_domain VARCHAR(30),
    -- What this contest asks for, in a shape the format decides. Validated in
    -- the service rather than here: the required keys differ per kind, and a
    -- CHECK spanning five shapes is unreadable and unchangeable.
    ADD COLUMN rules JSONB NOT NULL DEFAULT '{}'::JSONB,
    -- Code golf ranks the smallest number. Everything else ranks the largest.
    ADD COLUMN scoring_direction VARCHAR(20) NOT NULL DEFAULT 'higher_is_better',
    ADD CONSTRAINT tournaments_rules_is_object CHECK (jsonb_typeof(rules) = 'object'),
    ADD CONSTRAINT tournaments_scoring_direction_check
        CHECK (scoring_direction IN ('higher_is_better', 'lower_is_better'));

COMMENT ON COLUMN tournaments.scoring_direction IS
    'Which end of the scale wins. Code golf ranks ascending; a leaderboard '
    'that crowns the longest solution is worse than no leaderboard.';

COMMENT ON COLUMN tournaments.rules IS
    'What the contest asks for. Shape depends on kind: code_golf names a '
    'language and a problem, marathon names a target and a window, hackathon '
    'names a theme.';

-- ═══════════════════════════════════════════════════════════════════
-- What a participant handed in
-- ═══════════════════════════════════════════════════════════════════
--
-- One row per participant per contest, revised in place. A contest where
-- somebody can submit five times and have the best one counted is a different
-- contest, and none of the three formats here is that one.

CREATE TABLE tournament_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    -- Who it counts for. Mirrors tournament_participants, so a guild entry in
    -- a team hackathon is one submission, not one per member.
    participant_type VARCHAR(20) NOT NULL CHECK (participant_type IN ('user', 'guild')),
    participant_id UUID NOT NULL,
    -- Who actually handed it in. Differs from participant_id for a guild, and
    -- is who to ask when the submission is questioned.
    submitted_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    artifact_url TEXT NOT NULL,
    artifact_type VARCHAR(30) NOT NULL CHECK (artifact_type IN (
        'repository', 'pull_request', 'gist', 'writeup', 'demo'
    )),
    -- A second link, for the formats that ask for two things: a hackathon
    -- wants the project and the writeup, a TDD contest the code and the run.
    secondary_url TEXT,
    summary TEXT NOT NULL,
    language VARCHAR(40),

    -- Code golf: the number being competed on, counted by the submitter and
    -- verifiable from the artifact. NULL for judged formats.
    measured_value INTEGER,

    status VARCHAR(20) NOT NULL DEFAULT 'submitted' CHECK (status IN (
        'submitted', 'accepted', 'rejected', 'disqualified'
    )),
    -- 0..100. NULL until somebody judges, which is the honest state for a
    -- contest still running.
    judge_score SMALLINT CHECK (judge_score BETWEEN 0 AND 100),
    judged_by UUID REFERENCES users(id) ON DELETE SET NULL,
    judged_at TIMESTAMPTZ,
    judge_notes TEXT,

    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One entry per participant. Revised, not stacked.
    UNIQUE (tournament_id, participant_type, participant_id),

    CONSTRAINT submission_measure_is_positive
        CHECK (measured_value IS NULL OR measured_value > 0),

    -- Refusing somebody's work without saying why is the one thing a contest
    -- must never do. `IS NOT NULL` first: btrim(NULL) is NULL, and a CHECK
    -- that evaluates to NULL passes.
    CONSTRAINT refusal_carries_a_reason CHECK (
        status NOT IN ('rejected', 'disqualified')
        OR (judge_notes IS NOT NULL AND btrim(judge_notes) <> '')
    ),

    -- A score nobody signed cannot be questioned.
    CONSTRAINT a_score_has_a_judge CHECK (
        judge_score IS NULL OR (judged_by IS NOT NULL AND judged_at IS NOT NULL)
    )
);

COMMENT ON TABLE tournament_submissions IS
    'What a participant handed in, and what a judge made of it. One row per '
    'participant, revised in place — best-of-five is a different contest.';

CREATE INDEX idx_tournament_submissions_tournament
    ON tournament_submissions (tournament_id, status);

CREATE INDEX idx_tournament_submissions_submitter
    ON tournament_submissions (submitted_by, submitted_at DESC);

CREATE OR REPLACE FUNCTION touch_tournament_submissions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tournament_submissions_updated_at
    BEFORE UPDATE ON tournament_submissions
    FOR EACH ROW EXECUTE FUNCTION touch_tournament_submissions_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- A submission belongs to somebody who registered
-- ═══════════════════════════════════════════════════════════════════
--
-- A foreign key would say this, but `tournament_participants` is keyed on a
-- polymorphic pair and the row is created by registration, which can be
-- withdrawn. A trigger states the rule at the moment it matters and gives a
-- message somebody can act on.

CREATE OR REPLACE FUNCTION submission_requires_registration()
RETURNS TRIGGER AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM tournament_participants p
         WHERE p.tournament_id = NEW.tournament_id
           AND p.participant_type = NEW.participant_type
           AND p.participant_id = NEW.participant_id
    ) THEN
        RAISE EXCEPTION 'submission for an unregistered participant'
            USING HINT = 'register for the contest before submitting to it';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_submission_requires_registration
    BEFORE INSERT ON tournament_submissions
    FOR EACH ROW EXECUTE FUNCTION submission_requires_registration();
