-- What has already been announced.
--
-- ## Why a table rather than a flag on the contest
--
-- The sweep that warns people about a deadline runs every hour. Without a
-- record of what it has already sent, an entrant gets the same "closes in 48
-- hours" every hour for two days, which is how somebody mutes the platform
-- the week they most need it.
--
-- A boolean on `tournaments` would answer "was the warning sent" and not "to
-- whom", which is the question that matters: people enter a contest one at a
-- time, and somebody who entered an hour before the deadline has still never
-- been warned. So the row is per person, per contest, per moment.
--
-- ## Why it is not the notifications table
--
-- `notifications` is what a person sees, and it is theirs: it can be read,
-- dismissed, and eventually pruned. Deciding whether to send by querying it
-- would make a cleanup job start sending duplicates months later. This table
-- is the sender's own record and nobody reads it but the sweep.

CREATE TABLE contest_reminders_sent (
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Which moment: `submission_deadline`, `jury_deadline`, `closed`.
    -- Deliberately not a CHECK — the sweep owns this vocabulary, and an
    -- unknown value here can at worst suppress one reminder, never corrupt a
    -- result.
    moment        VARCHAR(40) NOT NULL,
    sent_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (tournament_id, user_id, moment)
);

COMMENT ON TABLE contest_reminders_sent IS
    'The sweep''s own record of which contest reminder went to whom. Not a '
    'user-facing history — `notifications` is that, and reading it to decide '
    'what to send would make pruning it produce duplicates.';

-- The sweep asks "who in this contest has not been told yet", which is this
-- index followed by an anti-join.
CREATE INDEX idx_contest_reminders_lookup
    ON contest_reminders_sent (tournament_id, moment);
