-- SKI-119 (P26 v2 J-01) — track opt-in public announcement on PR.
--
-- When the challenger opts in via `submit-pr {announce_publicly:true}`,
-- the backend posts a comment on the PR using the CHALLENGER's own
-- GitHub token (never the bot's). We stamp `announced_at` so a retry
-- doesn't double-post: the second submit-pr for the same slice is a
-- no-op on the announcement side.
--
-- Not on `submitted_at` itself because the challenger could resubmit
-- (typo, wrong URL first time) — we want the first announcement to
-- stick even if the slice cycles through `claimed` → `submitted` twice.

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS announced_at TIMESTAMPTZ;
