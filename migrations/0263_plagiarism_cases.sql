-- Accusing somebody of copying, and letting them answer.
--
-- ## Why this is not a `reports` row
--
-- `reports` records that somebody complained: a reason from a short list, a
-- free-text detail, and an admin who resolves or dismisses it. That is the
-- right shape for spam and for harassment.
--
-- It is the wrong shape here, and for one reason: **there is nowhere for the
-- accused to answer**. An accusation of plagiarism decided without hearing the
-- person accused is not a decision, it is a verdict — and the outcome is
-- disqualification, a confiscated prize, and a public record. The right to
-- answer is the substance of this table, not a field on it.
--
-- ## Seventy-two hours
--
-- Long enough to find the file, the timestamps, the client who commissioned
-- the piece. Short enough that a contest is not held open by an accusation
-- nobody follows up. It is a floor on the *decision*, not a deadline on the
-- person: a case may be decided after the window whether or not they answered,
-- and an answer that arrives late is still recorded.
--
-- ## What this table does not do
--
-- It does not ban anybody, and it counts strikes without acting on them. The
-- backlog's "second strike, ban" is a rule that reads well and would, one
-- Tuesday, ban somebody on a second accusation that a tired reviewer upheld in
-- four minutes. The count is here so a human can see it; the ban stays a
-- decision a human takes and signs.

CREATE TABLE IF NOT EXISTS plagiarism_cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    submission_id UUID NOT NULL
        REFERENCES tournament_submissions(id) ON DELETE CASCADE,

    -- Whose work is accused. Denormalised from the submission so the queue
    -- and the strike count are one query, and so the case survives with a
    -- name attached if the submission is withdrawn.
    accused_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Null once that account is gone. The accusation stands on its evidence,
    -- not on who made it — and an anonymous-by-deletion case still has to be
    -- answerable.
    raised_by UUID REFERENCES users(id) ON DELETE SET NULL,

    -- Eighty characters, as a constraint. "C'est copié" is not an accusation
    -- somebody can answer, and the person answering has three days to find
    -- what they are answering.
    reason_md TEXT NOT NULL CHECK (length(reason_md) BETWEEN 80 AND 4000),

    -- Where the original is. Required: an accusation with no link to the
    -- work it is compared against cannot be checked by anybody, including
    -- the reviewer who has to decide it.
    evidence_url TEXT NOT NULL
        CHECK (evidence_url ~ '^https://' AND length(evidence_url) BETWEEN 12 AND 2048),

    raised_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- When the accused may be decided against without having answered.
    respond_by TIMESTAMPTZ NOT NULL,

    response_md TEXT CHECK (response_md IS NULL OR length(response_md) BETWEEN 1 AND 8000),
    responded_at TIMESTAMPTZ,

    status VARCHAR(12) NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'upheld', 'dismissed')),

    -- Read by the accused, and by anybody reading the transparency page.
    -- Required on a decision, in both directions: dismissing an accusation
    -- without saying why leaves the accusation standing in everybody's memory.
    decision_md TEXT CHECK (decision_md IS NULL OR length(decision_md) BETWEEN 80 AND 8000),
    decided_by UUID REFERENCES users(id) ON DELETE SET NULL,
    decided_at TIMESTAMPTZ,

    CONSTRAINT plagiarism_response_is_dated CHECK (
        (response_md IS NULL) = (responded_at IS NULL)
    ),
    CONSTRAINT plagiarism_decision_is_complete CHECK (
        status = 'open'
        OR (decision_md IS NOT NULL AND decided_at IS NOT NULL)
    ),
    -- An open case has decided nothing. Without this, a status could be
    -- walked back to `open` while leaving a decision behind it.
    CONSTRAINT plagiarism_open_has_no_decision CHECK (
        status <> 'open' OR (decision_md IS NULL AND decided_at IS NULL)
    )
);

-- One open case per submission. A second accusation of the same piece while
-- the first is undecided splits the evidence across two files and gives the
-- accused two clocks.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_open_plagiarism_case
    ON plagiarism_cases (submission_id)
    WHERE status = 'open';

-- The queue a reviewer works: oldest first, because the accused is waiting.
CREATE INDEX IF NOT EXISTS idx_plagiarism_open
    ON plagiarism_cases (raised_at ASC)
    WHERE status = 'open';

-- The strike count, per person.
CREATE INDEX IF NOT EXISTS idx_plagiarism_by_accused
    ON plagiarism_cases (accused_id, status);

COMMENT ON TABLE plagiarism_cases IS
    'An accusation that a contest entry was copied, and the answer to it. Not '
    'a `reports` row: a report has nowhere for the accused to reply, and the '
    'reply is the substance of this procedure.';

COMMENT ON COLUMN plagiarism_cases.respond_by IS
    'When the case may be decided without an answer. A floor on the decision, '
    'not a deadline on the person — a late answer is still recorded.';

-- ═══════════════════════════════════════════════════════════════════
-- What an upheld case does to the entry
-- ═══════════════════════════════════════════════════════════════════
--
-- Nothing to add here: `tournament_submissions.status` already accepts
-- `disqualified`, and `refusal_carries_a_reason` already requires
-- `judge_notes` to be filled when it is set. Those two together are exactly
-- what an upheld case needs — the entry is marked rather than deleted, and
-- the marking carries its reason.
--
-- Deleting the row instead would erase the fact that it was entered at all,
-- which is what a disqualification must not do: the other entrants moved up,
-- and a ranking whose gaps are unexplained is a ranking nobody can check.

-- ═══════════════════════════════════════════════════════════════════
-- Telling the person accused
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional) VALUES

    -- Transactional, and every channel on by default. Somebody has three days
    -- to answer an accusation that can cost them a prize; a preference that
    -- silenced this would silence the only warning they get.
    ('moderation.plagiarism_case_opened',  'account',
     TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE),

    -- Told either way. Being cleared matters as much as being disqualified,
    -- and somebody who was accused and then heard nothing assumes the worst.
    ('moderation.plagiarism_case_decided', 'account',
     TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE)

ON CONFLICT (kind) DO NOTHING;

UPDATE notification_kinds
   SET cta_path = '/contests/plagiarism/{case_id}'
 WHERE kind IN ('moderation.plagiarism_case_opened',
                'moderation.plagiarism_case_decided');
