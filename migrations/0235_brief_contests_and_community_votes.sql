-- Contests decided by a jury, and contests decided by the room.
--
-- ## Two kinds, not two tables
--
-- The design backlog asked for `design_contests` and `design_submissions`.
-- Migration 0189 already built that shape on `tournaments` and
-- `tournament_submissions`, and stated the principle this migration follows:
-- a domain on the tournament says what the contest is about, so a design
-- hackathon and a code hackathon share their machinery instead of each
-- getting a private copy.
--
-- What design does need is one kind nothing covers. A brief contest is not a
-- hackathon: nobody builds against a clock, everybody answers the same
-- written brief, and a jury ranks the answers. That is `brief_contest`, and
-- it is deliberately not named `design_contest` — an agency briefing three
-- copywriters is the same event.
--
-- ## `prompt_battle` becomes `duel`
--
-- Migration 0223 added `prompt_battle`: head to head on one task, community
-- vote. A design battle is the same event with a different subject, and
-- adding `design_battle` next to it would be the per-domain kind that 0189
-- refused. The kind is renamed to what it actually is; `skill_domain` says
-- whether the two people are writing prompts or logos.
--
-- ## The list restated, including two that went missing
--
-- Migration 0223 replaced the `kind` CHECK and did not carry `code_golf` and
-- `tdd_contest` across, while `services::tournament` still offers both. A
-- code golf tournament cannot currently be inserted. Restating the full list
-- is the only way to extend a CHECK, so the two come back here.

ALTER TABLE tournaments DROP CONSTRAINT IF EXISTS tournaments_kind_check;

-- Rename before the constraint, or the rows would fail it.
UPDATE tournaments SET kind = 'duel' WHERE kind = 'prompt_battle';

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_kind_check
    CHECK (kind IN (
        -- Migration 0030
        'individual', 'guild_war', 'hackathon',
        -- Migration 0114, la Grande Épreuve
        'marathon', 'defi_solitaire',
        -- Migration 0189, code
        'code_golf', 'tdd_contest',
        -- Migration 0223, AI
        'benchmark_rush',
        -- Head to head on one task, community vote. Any domain.
        'duel',
        -- One written brief, N answers, a jury ranks them. Any domain.
        'brief_contest'
    ));

COMMENT ON COLUMN tournaments.kind IS
    'What people do. The pairing scheme is `format`, and the two are '
    'independent: a duel is a bracket, a benchmark rush is a ladder. The '
    'subject is `skill_domain`, never the kind — a design hackathon and a '
    'code hackathon are one kind.';

-- Design answers are files, boards and reels, none of which is a repository.
ALTER TABLE tournament_submissions
    DROP CONSTRAINT IF EXISTS tournament_submissions_artifact_type_check;

ALTER TABLE tournament_submissions
    ADD CONSTRAINT tournament_submissions_artifact_type_check
    CHECK (artifact_type IN (
        'repository', 'pull_request', 'gist', 'writeup', 'demo',
        'design_file',   -- a Figma node, a board, a source file
        'image_set',     -- boards, an identity, an illustration series
        'video',         -- a reel, a motion piece
        'audio'          -- a sound identity, an ambience
    ));

-- ═══════════════════════════════════════════════════════════════════
-- Who judges
-- ═══════════════════════════════════════════════════════════════════
--
-- `tournament_submissions.judged_by` records who scored one answer. It does
-- not say who was asked, who agreed, or who declined — and a jury that never
-- answered is the failure mode a contest organiser needs to see before the
-- deadline, not after.

CREATE TABLE tournament_juries (
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    juror_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    invited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at TIMESTAMPTZ,
    declined_at TIMESTAMPTZ,
    -- Declining because a family is outside your competence is the right
    -- answer, and saying so is what lets the organiser widen the panel.
    decline_reason TEXT,

    PRIMARY KEY (tournament_id, juror_user_id),

    CONSTRAINT tournament_juries_one_answer
        CHECK (accepted_at IS NULL OR declined_at IS NULL)
);

COMMENT ON TABLE tournament_juries IS
    'Who was asked to judge, and what they answered. A panel that never '
    'replied is a problem to see before the deadline.';

CREATE INDEX idx_tournament_juries_pending
    ON tournament_juries (juror_user_id, invited_at DESC)
    WHERE accepted_at IS NULL AND declined_at IS NULL;

-- Nobody judges a contest they entered.
CREATE OR REPLACE FUNCTION tournament_juror_is_not_a_participant()
RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM tournament_submissions s
         WHERE s.tournament_id = NEW.tournament_id
           AND s.participant_type = 'user'
           AND s.participant_id = NEW.juror_user_id
    ) THEN
        RAISE EXCEPTION 'user % has an entry in tournament % and cannot judge it',
            NEW.juror_user_id, NEW.tournament_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tournament_juror_is_not_a_participant
    BEFORE INSERT OR UPDATE ON tournament_juries
    FOR EACH ROW EXECUTE FUNCTION tournament_juror_is_not_a_participant();

-- And nobody enters a contest they judge.
CREATE OR REPLACE FUNCTION tournament_participant_is_not_a_juror()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.participant_type = 'user' AND EXISTS (
        SELECT 1 FROM tournament_juries j
         WHERE j.tournament_id = NEW.tournament_id
           AND j.juror_user_id = NEW.participant_id
           AND j.declined_at IS NULL
    ) THEN
        RAISE EXCEPTION 'user % is on the jury of tournament % and cannot enter it',
            NEW.participant_id, NEW.tournament_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tournament_participant_is_not_a_juror
    BEFORE INSERT ON tournament_submissions
    FOR EACH ROW EXECUTE FUNCTION tournament_participant_is_not_a_juror();

-- ═══════════════════════════════════════════════════════════════════
-- What the room thinks
-- ═══════════════════════════════════════════════════════════════════
--
-- A jury answers "is this good craft". The community answers "does this
-- land". Neither replaces the other, and a contest declares in its `rules`
-- which one decides, or in what proportion.
--
-- One vote per account per contest, moved rather than stacked: the row is
-- keyed on the voter, so changing your mind updates it. A contest where
-- somebody can spread five votes is a different contest.

CREATE TABLE tournament_community_votes (
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    voter_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    submission_id UUID NOT NULL REFERENCES tournament_submissions(id) ON DELETE CASCADE,
    voted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (tournament_id, voter_user_id)
);

COMMENT ON TABLE tournament_community_votes IS
    'One vote per account per contest, movable until the deadline. Kept as '
    'rows rather than a counter so a suspicious result can be looked at.';

CREATE INDEX idx_tournament_community_votes_submission
    ON tournament_community_votes (submission_id);

-- Burst detection reads this: a spike on one entry in a few minutes is the
-- cheap signal that a vote is being bought.
CREATE INDEX idx_tournament_community_votes_recent
    ON tournament_community_votes (tournament_id, voted_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- What a vote has to clear
-- ═══════════════════════════════════════════════════════════════════
--
-- In the database rather than in a handler, because a vote arrives from four
-- places — the web app, the API, an admin correction, a test fixture — and
-- one of them will forget.

CREATE OR REPLACE FUNCTION tournament_community_vote_is_eligible()
RETURNS TRIGGER AS $$
DECLARE
    min_age_days INT;
    sub_tournament UUID;
    sub_owner_type TEXT;
    sub_owner UUID;
    sub_status TEXT;
    account_created TIMESTAMPTZ;
BEGIN
    SELECT s.tournament_id, s.participant_type, s.participant_id, s.status
      INTO sub_tournament, sub_owner_type, sub_owner, sub_status
      FROM tournament_submissions s WHERE s.id = NEW.submission_id;

    IF sub_tournament IS DISTINCT FROM NEW.tournament_id THEN
        RAISE EXCEPTION 'submission % is not in tournament %',
            NEW.submission_id, NEW.tournament_id;
    END IF;

    IF sub_status IN ('rejected', 'disqualified') THEN
        RAISE EXCEPTION 'submission % is out of the running', NEW.submission_id;
    END IF;

    IF sub_owner_type = 'user' AND sub_owner = NEW.voter_user_id THEN
        RAISE EXCEPTION 'user % cannot vote for their own entry', NEW.voter_user_id;
    END IF;

    -- The floor lives in the contest's rules so a high-stakes contest can ask
    -- for more. Thirty days is the default: creating accounts is free, and a
    -- vote that costs nothing is worth nothing.
    SELECT COALESCE((t.rules ->> 'community_vote_min_account_age_days')::INT, 30)
      INTO min_age_days
      FROM tournaments t WHERE t.id = NEW.tournament_id;

    SELECT u.created_at INTO account_created FROM users u WHERE u.id = NEW.voter_user_id;
    IF account_created IS NULL THEN
        RAISE EXCEPTION 'voter % does not exist', NEW.voter_user_id;
    END IF;
    IF account_created > NOW() - make_interval(days => min_age_days) THEN
        RAISE EXCEPTION
            'account % is younger than the % day floor this contest asks for',
            NEW.voter_user_id, min_age_days;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tournament_community_vote_is_eligible
    BEFORE INSERT OR UPDATE ON tournament_community_votes
    FOR EACH ROW EXECUTE FUNCTION tournament_community_vote_is_eligible();
