-- Anti-mimicry during the submission window, without an unquestionable result.
--
-- ## The disagreement this settles
--
-- The backend's position was that every entry is readable at all times: "a
-- contest whose entries cannot be read is a contest whose result cannot be
-- questioned." The design side asked for blind review, because in a brief
-- contest the first strong answer published pulls every later one towards it
-- — mimicry is the format's known failure, and it is invisible in the result.
--
-- Both are right, and they were treated as exclusive because the question was
-- asked as "are entries public?". The useful question is *when*.
--
-- ## What this does
--
-- A contest may declare a blind submission window. While it is open, an
-- entrant sees their own entry and nobody else's. At the deadline everything
-- becomes public — before the ranking is published, and permanently after.
--
-- So the transparency argument keeps everything it wanted: the result is
-- still questionable by anyone, against the complete field, for as long as
-- the contest exists. What is given up is only the ability to read other
-- people's work *while there is still time to copy it*, which was never what
-- contestability needed.
--
-- ## The jury is never blinded
--
-- The panel judges during the window on some formats, and a jury that cannot
-- read the entries cannot judge them. Blinding them too would not be a flag,
-- it would be a different contest calendar — judging could only start after
-- the deadline. That is a real format and this is not it. Jurors, and the
-- staff who arbitrate, read everything throughout.
--
-- ## Default
--
-- FALSE. Every contest that exists keeps behaving exactly as it does today,
-- and a contest opts in when its organiser judges mimicry to be the bigger
-- risk. Nothing is decided platform-wide by this migration except that the
-- choice is now available and recorded per contest.

ALTER TABLE tournaments
    ADD COLUMN blind_until_close BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN tournaments.blind_until_close IS
    'While the contest is open, entrants see only their own entry; everything '
    'becomes public at the deadline. Jurors and staff always read everything. '
    'FALSE means entries are public throughout, which is the default.';

-- ## No constraint tying the flag to the status
--
-- A first draft required `status IN ('upcoming', 'registration', 'active')`
-- whenever the flag was set, to stop somebody enabling blindness on a contest
-- that has already ended. It would have made a blind contest impossible to
-- conclude: moving it to `concluded` violates the very constraint that its
-- own flag put in force.
--
-- The flag needs no constraint. What it means is read together with the
-- status — `blind_until_close AND status is open` — so on a closed contest it
-- is inert by construction, and it stays on the row as the record of how that
-- contest was run.
