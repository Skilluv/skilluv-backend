-- Mentorship money is held, not wired. Rewrite `payout_status` accordingly.
--
-- Migration 0152 introduced the column to record whether a transfer had
-- happened, because the code was stamping sessions as paid when nothing had
-- moved. It described a world where completing a session immediately wired
-- money to the mentor.
--
-- That world is gone. Completing a session records what is owed and holds
-- it: the student has a window to complain, and the mentor withdraws later
-- through whichever rail reaches them. The vocabulary follows:
--
--   held      the mentor's share is recorded and waiting out the window
--   released  the window closed or the student confirmed; withdrawable
--   disputed  frozen while a complaint is examined
--   refunded  returned to the student
--
-- The old values are removed rather than kept alongside. Two vocabularies
-- for one column means every reader has to know which era a row belongs to,
-- and every new query has to handle both. Existing rows are translated:
--
--   paid        -> released   the money had left; it is the mentor's
--   pending     -> held       recorded, not yet moved
--   failed      -> held       owed and unrecorded; the sweep will catch it
--   no_account  -> held       same, and routing now reaches these mentors

UPDATE mentorship_sessions SET payout_status = 'released' WHERE payout_status = 'paid';
UPDATE mentorship_sessions SET payout_status = 'held'
 WHERE payout_status IN ('pending', 'failed', 'no_account');

ALTER TABLE mentorship_sessions
    DROP CONSTRAINT IF EXISTS mentorship_sessions_payout_status_check;

ALTER TABLE mentorship_sessions
    ALTER COLUMN payout_status SET DEFAULT 'held';

ALTER TABLE mentorship_sessions
    ADD CONSTRAINT mentorship_sessions_payout_status_check
    CHECK (payout_status IN ('held', 'released', 'disputed', 'refunded'));

COMMENT ON COLUMN mentorship_sessions.payout_status IS
    'held: owed and waiting out the release window. released: withdrawable. '
    'disputed: frozen pending a decision. refunded: returned to the student.';

-- Sessions the student has confirmed, which releases the hold early.
--
-- Separate from `status`: a session can be completed by the mentor and not
-- yet confirmed by the student, and that gap is exactly what the window is
-- about.
ALTER TABLE mentorship_sessions
    ADD COLUMN confirmed_by_mentee_at TIMESTAMPTZ;

COMMENT ON COLUMN mentorship_sessions.confirmed_by_mentee_at IS
    'When the student confirmed the session took place. Releases the hold '
    'immediately instead of waiting out the window.';
