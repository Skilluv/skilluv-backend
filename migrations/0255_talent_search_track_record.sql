-- What a recruiter search could not reach: a track record.
--
-- `/talents/search` filters on what somebody *is* — their trade, their skills,
-- their craft score, their country. It could not filter on what they have
-- *done*: contests won, missions delivered, a featuring.
--
-- That is the half a recruiter pays for. The filters are added to the one
-- search endpoint rather than to a second one per domain — v4 exists precisely
-- because three endpoints answering the same question differently is three
-- places for a filter to be subtly wrong.
--
-- No new columns: every fact already exists. What was missing is the ability
-- to read it from the direction a search comes at it.

-- A win is looked up by person, not by tournament. The primary key is
-- (tournament_id, participant_type, participant_id), which answers "who was in
-- this contest" and not "what has this person won" — the second needs a scan
-- of every participation ever recorded.
--
-- Partial on rank IS NOT NULL: an unranked participation is a contest still
-- running, and it is never what a recruiter filter matches.
CREATE INDEX IF NOT EXISTS idx_tournament_participants_by_person
    ON tournament_participants (participant_type, participant_id, rank)
    WHERE rank IS NOT NULL;

-- Portfolios on platforms Skilluv does not own, looked up by provider.
-- `idx_external_signals_by_user` orders by verification date, which serves the
-- profile page; a search asking "who has a confirmed Behance" reads the other
-- way round.
--
-- Partial on confirmed rows only: an unconfirmed signal is a URL somebody
-- typed, and this is the surface where that distinction matters most.
CREATE INDEX IF NOT EXISTS idx_external_signals_by_provider
    ON external_signals (provider, user_id)
    WHERE verified_at IS NOT NULL;

-- The most recent featuring, per person, across every domain. The primary key
-- is (skill_domain, week_of) — the editorial calendar's order, not a
-- recruiter's.
CREATE INDEX IF NOT EXISTS idx_featured_talents_by_person
    ON featured_talents (user_id, week_of DESC);
