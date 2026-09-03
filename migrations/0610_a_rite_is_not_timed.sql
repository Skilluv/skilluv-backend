-- The Bonjour Skilluv rites carry no countdown.
--
-- ## What 0607 got wrong
--
-- It gave all twelve `duration_minutes = 60`, copying the shape of the
-- catalogue around them. But `start_challenge` reads that column and writes
-- `expires_at = NOW() + duration_minutes`, and `submit_challenge` marks
-- anything arriving after it `failure`. So the rite became a sixty-minute
-- wall-clock exercise, counted from the moment somebody opened the page.
--
-- That is wrong twice over. It is impossible on the `code` rite, whose gesture
-- is to fork a repository, clone it, commit and open a pull request — nobody
-- does that in an hour from a cold start, and the timer would have marked the
-- attempt failed while the pull request was still open. And it is arbitrary on
-- the other eleven: "design one screen", "record twenty seconds and declare
-- every source", "play a slice start to finish" are gestures somebody fits
-- into their week. A countdown would force them to prepare the artifact
-- *before* starting the rite, which is the opposite of what starting it means.
--
-- ## Why NULL rather than a bigger number
--
-- A timer measures speed. The rite measures whether somebody can produce one
-- real thing in the shape of their trade, and it is read by a person — after
-- SKI-361 it cannot be read by anything else. There is no number of minutes
-- that belongs in that sentence, and any number picked here would be a
-- constraint nobody decided.
--
-- `duration_minutes IS NULL` means `start_challenge` sets no `expires_at`,
-- which is the existing "no timer" path and not a new one.
--
-- ## In-flight attempts
--
-- The `expires_at` already written onto a `challenge_submissions` row is
-- cleared for attempts still in progress. Leaving them would expire the very
-- people who started before this ran, which is the population the fix is for.
-- Finished rows are left alone: they record what actually happened.

UPDATE challenge_templates
SET duration_minutes = NULL
WHERE is_domain_rite;

UPDATE challenge_submissions cs
SET expires_at = NULL
FROM challenge_templates ct
WHERE ct.id = cs.challenge_id
  AND ct.is_domain_rite
  AND cs.status = 'in_progress';
