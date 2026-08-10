-- SKI-72 (P26 v2 B-01) — Track internal-tracker → GitHub Issue sync for
-- Skilluv challenges. When a ticket in our internal tracker is flagged as
-- `challenge-ready`, the linear_webhook route creates (or updates) a GitHub
-- Issue in the target repo with label `skilluv-challenge`; this table
-- records the mapping so subsequent updates (title, description, close)
-- reach the same issue.
--
-- Note on naming: only the table name references "linear" so operators can
-- correlate rows with the upstream ticket URL. Payload sent to GitHub does
-- not mention the tracker vendor (public repo, deliberate policy).
--
-- linear_issue_id is the upstream ticket identifier (e.g. "SKI-72"). The
-- unique index enforces one GitHub issue per upstream ticket.

CREATE TABLE IF NOT EXISTS linear_challenge_sync (
    linear_issue_id      TEXT PRIMARY KEY,
    linear_ticket_url    TEXT NOT NULL,
    github_owner         TEXT NOT NULL,
    github_repo          TEXT NOT NULL,
    github_issue_number  INTEGER NOT NULL,
    github_issue_url     TEXT NOT NULL,
    last_status          TEXT NOT NULL DEFAULT 'open',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_linear_challenge_sync_repo
    ON linear_challenge_sync (github_owner, github_repo);
