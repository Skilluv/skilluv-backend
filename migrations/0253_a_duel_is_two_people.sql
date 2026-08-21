-- A duel with nine entrants is not a duel.
--
-- ## What was missing
--
-- Migration 0235 added the `duel` kind and `contest::validate_rules` checks
-- that one states a task and a duration. Nothing checked the only property
-- that makes it a duel: **two people**.
--
-- A third entrant turns it into a small contest judged by the room with no
-- jury and no brief — which is a worse format than either of the two it sits
-- between, and it would be discovered when the results page showed a podium
-- for a head-to-head.
--
-- ## Why a trigger rather than a check in the service
--
-- Participants are inserted from several places: the registration endpoint,
-- the admin path, and the seeds. A rule enforced in one of them is a rule
-- that holds until somebody writes a second insert — and the second insert is
-- usually the one written in a hurry.
--
-- A CHECK constraint cannot do it: the answer depends on the other rows.
--
-- ## Why the count is of rows and not of people
--
-- `tournament_participants` is keyed on (tournament, type, id), so a person
-- cannot enter twice. Counting rows is counting entrants.

CREATE OR REPLACE FUNCTION tournament_duel_is_head_to_head()
RETURNS TRIGGER AS $$
DECLARE
    contest_kind TEXT;
    entrants INTEGER;
BEGIN
    SELECT kind INTO contest_kind FROM tournaments WHERE id = NEW.tournament_id;

    IF contest_kind <> 'duel' THEN
        RETURN NEW;
    END IF;

    SELECT count(*) INTO entrants
      FROM tournament_participants
     WHERE tournament_id = NEW.tournament_id;

    -- The row being inserted is not counted yet, so two existing entrants
    -- means this one would be the third.
    IF entrants >= 2 THEN
        RAISE EXCEPTION
            'a duel is two people: % already has both', NEW.tournament_id
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION tournament_duel_is_head_to_head() IS
    'Refuses a third entrant in a duel. Enforced here rather than in a '
    'service because participants are inserted from several places, and a '
    'rule that lives in one of them holds only until somebody writes the '
    'second insert.';

CREATE TRIGGER trg_tournament_duel_is_head_to_head
    BEFORE INSERT ON tournament_participants
    FOR EACH ROW
    EXECUTE FUNCTION tournament_duel_is_head_to_head();
