-- Security competitions, on the tournament tables.
--
-- ## Why there is no `competitions` table
--
-- Ticket P-01 asked for three: `competitions`, `competition_participants`,
-- `competition_events`. They exist, under the names migration 0030 gave them —
-- `tournaments`, `tournament_participants`, `tournament_submissions` — with
-- juries, community votes, series, cash prizes held in escrow, sponsors, and
-- the podium hooks that award fragments and badges. `tournament_kinds` is the
-- table that says what formats exist, and it is where the five below go.
--
-- The one thing P-01 asked for that is genuinely not there is
-- `competition_events`: a running log of scoring events during a live
-- competition. It is not added, and the reason is that the events it listed —
-- a flag captured, a finding confirmed, a first solve — are already rows
-- somewhere better: `security_flag_attempts` (0549) has every solve with its
-- timestamp, and `security_findings` has every confirmation. A live scoreboard
-- is a query over those two windowed to the competition, which is a view of
-- the truth rather than a second copy of it that can disagree with it.
--
-- ## Five formats
--
-- `bug_bash` already exists for `quality` (0455), where it means hunting
-- defects. The security one is a different competition — vulnerabilities on a
-- live scope, scored by severity — so it is `sec_bug_bash` rather than a
-- second meaning for one slug. That is the ticket C-09 asked for.
--
-- ## The side a participant is on (P-04)
--
-- `side` on `tournament_participants`, and it is the only structural addition
-- here. A purple exercise is the one competition format on this platform where
-- participants are not all playing the same game: the red side is scored on
-- what it achieved and the blue side on what it saw, and a leaderboard that
-- mixed them would rank a detection against an exploit.
--
-- Nullable, because in every other format there are no sides, and a default
-- of 'red' would have quietly put every code-golf entrant on a team.

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, required_rule_keys, sort_order, is_juried,
     allows_community_vote)
VALUES

('sec_ctf_jeopardy', 'security', 'Jeopardy CTF',
 'A board of independent challenges, each worth points, solved in any order '
 'within a window. The format most people mean by "a CTF".',
 FALSE, TRUE, FALSE, ARRAY['scoreboard_frozen_minutes'], 610, FALSE, FALSE),

('sec_attack_defence', 'security', 'Attack and defence',
 'Every team runs the same vulnerable service and attacks everybody else''s. '
 'Points for flags taken and for keeping your own service up, which is what '
 'makes it the only format that scores patching.',
 FALSE, TRUE, FALSE, ARRAY['tick_seconds', 'service_uptime_weight'], 620, FALSE, FALSE),

('sec_bug_bash', 'security', 'Bug bash',
 'A defined scope, a window of a day or two, and everybody hunting at once. '
 'Scored on severity and on the quality of the report, not on the count — '
 'otherwise the winner is whoever files fastest.',
 TRUE, FALSE, FALSE, ARRAY['scope_url', 'severity_points'], 630, TRUE, FALSE),

('sec_purple_exercise', 'security', 'Purple exercise',
 'Two sides in one session: red runs techniques, blue detects and patches. '
 'Scored separately, because a detection and an exploit are not comparable, '
 'and debriefed together, which is the point.',
 TRUE, FALSE, FALSE, ARRAY['session_hours', 'red_points', 'blue_points'], 640, TRUE, FALSE),

-- Juried and not community-voted, which the CHECK on `tournament_kinds`
-- insists on choosing between. A jury, because whether a finding is real is
-- not a matter of opinion and a popularity vote on audit findings would
-- reward the most alarming write-up.
('sec_code_audit_rally', 'security', 'Code audit rally',
 'One codebase, a fixed window, everybody reading. Judged on the findings and '
 'on the dismissed scanner hits — a rally where nobody reports a false '
 'positive they refused is a rally nobody read carefully.',
 TRUE, FALSE, FALSE, ARRAY['repository_url', 'commit_sha'], 650, TRUE, FALSE);

-- ═══════════════════════════════════════════════════════════════════
-- Sides
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE tournament_participants
    ADD COLUMN side VARCHAR(10)
        CHECK (side IS NULL OR side IN ('red', 'blue', 'observer'));

COMMENT ON COLUMN tournament_participants.side IS
    'Which side of a purple exercise or an attack-and-defence competition this '
    'participant is on. NULL everywhere else: in every other format everybody '
    'is playing the same game, and a default would have put code-golf '
    'entrants on a team.';

-- A side is only meaningful in the two formats that have them. A trigger
-- rather than a CHECK because the format is a row on `tournaments`.
CREATE OR REPLACE FUNCTION trg_participant_side_fits_the_format()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    kind_slug TEXT;
BEGIN
    IF NEW.side IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT t.kind INTO kind_slug FROM tournaments t WHERE t.id = NEW.tournament_id;

    IF kind_slug NOT IN ('sec_purple_exercise', 'sec_attack_defence') THEN
        RAISE EXCEPTION
            'a % competition has no sides — participant side must be null',
            kind_slug;
    END IF;

    RETURN NEW;
END $$;

CREATE TRIGGER trg_tournament_participants_side
    BEFORE INSERT OR UPDATE OF side ON tournament_participants
    FOR EACH ROW EXECUTE FUNCTION trg_participant_side_fits_the_format();

-- A purple exercise ranks its two sides separately. The index is what makes
-- that leaderboard a single scan instead of two.
CREATE INDEX idx_tournament_participants_side
    ON tournament_participants (tournament_id, side, score DESC)
    WHERE side IS NOT NULL;
