-- SKI-75 (P26 v2 C-03) — record the user's fork on claim.
--
-- When a challenger claims a `github_issue` slice, we attempt to fork the
-- target repo to their GitHub account (only if they have connected GitHub
-- via OAuth — see github_connections). The resulting fork URL is stored
-- here so the frontend can deep-link into it and downstream services
-- (submit-pr / CI checker) can reference the working copy.
--
-- Best-effort: if the fork call fails or the user has no GitHub
-- connection, the claim still succeeds and this column stays NULL. The
-- user can add their fork URL manually via submit-pr (SKI-76).

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS fork_repo_url TEXT,
    ADD COLUMN IF NOT EXISTS fork_created_at TIMESTAMPTZ;

-- Enforce that if one is set, both are set (avoid a half-filled state).
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_fork_coherent
    CHECK (
        (fork_repo_url IS NULL AND fork_created_at IS NULL)
        OR (fork_repo_url IS NOT NULL AND fork_created_at IS NOT NULL)
    );
