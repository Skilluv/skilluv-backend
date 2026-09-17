-- A refused entrance says why, where the person can read it.
--
-- ## What this is for
--
-- The code entrance is decided by `services::hello_check` rather than by a
-- reviewer: its claim is "I can fork, edit and open a pull request", which is
-- mechanically true or false. Five checks, each with one right answer.
--
-- Four of them can refuse: the template was replaced rather than filled in,
-- the pull request touches more than HELLO.md, it was opened by a different
-- account, or the introduction is still the template.
--
-- Without somewhere to put the reason, a refusal would be a log line and a
-- page that stops moving - which is exactly the shape of the failure this
-- whole flow spent two days being: work done correctly, nothing visibly
-- happening, no way to tell "it is broken" from "you missed a step".
--
-- So the reason is a column, `/status` returns it, and the person reads a
-- sentence they can act on. The messages in `hello_check` are written for
-- somebody who has just arrived, not for whoever is reading the logs.
--
-- ## Why it is not an error state
--
-- The row stays at `pr_opened`. Nothing is lost and nothing is closed: the
-- person pushes another commit, the webhook fires again, and the checks run
-- against the new head. A refusal here is "not yet", never "no".
--
-- Cleared on acceptance, so a stale reason cannot sit under a completed rite.

ALTER TABLE onboarding_bonjour_skilluv
    ADD COLUMN IF NOT EXISTS check_refused_reason TEXT,
    ADD COLUMN IF NOT EXISTS check_ran_at TIMESTAMPTZ;

COMMENT ON COLUMN onboarding_bonjour_skilluv.check_refused_reason IS
    'Why the automatic entrance check refused this pull request, in words the '
    'person can act on. NULL when it passed or has not run.';

COMMENT ON COLUMN onboarding_bonjour_skilluv.check_ran_at IS
    'When the automatic check last ran, so a page can tell "not checked yet" '
    'from "checked and fine".';
