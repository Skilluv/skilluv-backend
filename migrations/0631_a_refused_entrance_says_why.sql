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
-- The row stays at `forked`, with the reason beside it, and nothing else is
-- written: no Hello Wall entry, no deliverable, no review task. The person
-- pushes another commit - the webhook listens for `synchronize` as well as
-- `opened` - and the checks run from the start against the new head. A
-- refusal here is "not yet", never "no".
--
-- An earlier draft moved the row to `pr_opened` on refusal. That was final
-- in practice: the webhook stops listening at `pr_opened`, so a refused
-- person could never retry, while their refused introduction went up on the
-- wall anyway. Staying at `forked` is what makes "not yet" true.
--
-- Cleared by the `pr_opened` transition that every non-refused path takes, so
-- a reason from an earlier attempt cannot sit under a rite that has moved on.

ALTER TABLE onboarding_bonjour_skilluv
    ADD COLUMN IF NOT EXISTS check_refused_reason TEXT,
    ADD COLUMN IF NOT EXISTS check_ran_at TIMESTAMPTZ;

COMMENT ON COLUMN onboarding_bonjour_skilluv.check_refused_reason IS
    'Why the automatic entrance check refused this pull request, in words the '
    'person can act on. NULL when it passed or has not run.';

COMMENT ON COLUMN onboarding_bonjour_skilluv.check_ran_at IS
    'When the automatic check last ran, so a page can tell "not checked yet" '
    'from "checked and fine".';
