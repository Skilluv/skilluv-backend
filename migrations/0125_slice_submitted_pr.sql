-- SKI-76 (P26 v2 C-04) — challenger declares their PR URL.
--
-- Once the challenger has opened a Pull Request against the target repo,
-- they call `POST /api/slices/{id}/submit-pr {pr_url}` to advance the
-- workflow from `claimed`/`in_progress` to `submitted`. `ci_green` and
-- beyond are driven by the CI webhook (SKI-87, later phase E).
--
-- URL shape: enforced at the service layer to accept only GitHub PR URLs
-- of the form https://github.com/{owner}/{repo}/pull/{n}. Storing the raw
-- URL (rather than parsed parts) keeps the schema stable if we later
-- support GitLab, Gitea, etc.

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS submitted_pr_url TEXT,
    ADD COLUMN IF NOT EXISTS submitted_at TIMESTAMPTZ;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_submission_coherent
    CHECK (
        (submitted_pr_url IS NULL AND submitted_at IS NULL)
        OR (submitted_pr_url IS NOT NULL AND submitted_at IS NOT NULL)
    );
