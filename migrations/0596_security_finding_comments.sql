-- The thread between the people working a finding, invisible to its reporter.
--
-- ## Why the three places that already hold text do not hold this
--
-- `security_finding_events` is an append-only audit trail keyed by transition:
-- every row answers "who moved this, from what, to what, and why". There is no
-- transition for "I tried to reproduce it on staging, the endpoint has changed,
-- I am back on it tomorrow", and inventing one would put a sentence about a
-- Tuesday afternoon into the record an auditor reads.
--
-- `security_finding_rounds.notes_md` is a question *put to the reporter*. What
-- is written there leaves the building by notification. It is the exact
-- opposite of this table.
--
-- `security_findings.triage_notes_md` is one field, written once, by the
-- triager. A critical finding passes a triager, a reviewer and an
-- administrator over three weeks with an embargo running, and one field cannot
-- hold a conversation between three people.
--
-- ## Why it must never leak
--
-- People write frankly here precisely because they believe it is internal. A
-- comment thread that reaches the reporter is worse than no comment thread:
-- the first leak teaches everybody to write nothing, and then the
-- coordination goes back to Discord where the finding cannot follow it.
--
-- So the rule is enforced by where it is read from, not by a flag: this table
-- is joined only by `GET /api/admin/security/findings/{id}`, which is behind
-- `require_reader`. `GET /api/security/findings/{id}` — the reporter's own
-- view — does not name it, and `tests/test_security_domain.rs` asserts that a
-- comment written here does not appear in the reporter's card.
--
-- ## No editing, no deleting
--
-- Deliberate. A note that decided how a finding was handled is part of how it
-- was handled, and a thread somebody can rewrite is not a record. Nothing
-- ships an update or a delete route for these rows.

CREATE TABLE security_finding_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    finding_id UUID NOT NULL REFERENCES security_findings(id) ON DELETE CASCADE,
    -- RESTRICT, not SET NULL: an unattributed internal note is a note nobody
    -- answers for. A departing account is anonymised elsewhere; it is not
    -- deleted out from under the trail.
    author_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- A length floor rather than non-empty, for the reason 0451 gives: "ok"
    -- satisfies `<> ''` and is the thing being refused.
    body_md TEXT NOT NULL CHECK (length(btrim(body_md)) >= 3),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The only access pattern: one finding's thread, oldest first.
CREATE INDEX idx_security_finding_comments_finding
    ON security_finding_comments (finding_id, created_at);

COMMENT ON TABLE security_finding_comments IS
    'Internal notes between triagers, reviewers and administrators working one '
    'finding. Never returned by any route the reporter can reach, and never '
    'notified. Append-only: no update and no delete route exists.';
