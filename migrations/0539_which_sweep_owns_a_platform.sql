-- Two sweeps, one table, and neither knew which rows were the other's.
--
-- ## What was actually happening
--
-- `services::code_portfolio::sync_stale` and
-- `services::portfolio_sync::sync_stale` both run as workers, both read
-- `user_external_portfolios`, and both stamp `last_synced_at`.
--
-- `code_portfolio` selects **every** stale row regardless of platform. Its
-- `fetch` returns an empty profile for anything that is not a forge, and the
-- row is then written with `last_synced_at = NOW()`. So a dev.to account was
-- marked freshly synced by the module that cannot read dev.to, and
-- `portfolio_sync` — which selects on `last_synced_at` being a week old —
-- did not see it as stale. The module that could read it kept losing the
-- race to the module that could not, and nothing failed: the row simply
-- carried a sync date and no figures.
--
-- `portfolio_sync` had the mirror-image problem. It selected on
-- `has_public_api`, which is TRUE for the forges, so it picked up GitHub rows
-- and handed them to a `match` with no arm for them.
--
-- ## `sync_implemented` was the wrong shape
--
-- Migration 0537 added it as a boolean, and a boolean cannot answer the
-- question that matters here, which is not *whether* a fetcher exists but
-- *which one*. It also recorded something false: the forges were set FALSE
-- because `portfolio_sync` has no arm for them, and `code_portfolio` reads
-- them perfectly well.
--
-- `synced_by` names the module. Each sweep selects its own rows and nothing
-- else, so neither can stamp a row it did not read.

ALTER TABLE portfolio_platforms DROP COLUMN sync_implemented;

ALTER TABLE portfolio_platforms
    ADD COLUMN synced_by VARCHAR(20)
        CHECK (synced_by IN ('portfolio_sync', 'code_portfolio'));

COMMENT ON COLUMN portfolio_platforms.synced_by IS
    'Which worker refreshes this platform, or NULL for one that is declared '
    'and never fetched. Two sweeps read this table and both used to select '
    'rows belonging to the other, stamping last_synced_at on accounts they '
    'could not read.';

COMMENT ON COLUMN portfolio_platforms.has_public_api IS
    'Whether the platform publishes figures that could be fetched. A fact '
    'about the platform rather than about this codebase: it is the shortlist '
    'of what is worth building, and synced_by says what was built.';

UPDATE portfolio_platforms SET synced_by = 'portfolio_sync'
 WHERE slug IN ('dev_to', 'hashnode', 'personal_blog', 'youtube', 'weblate');

-- The three forges with an anonymous profile API. SourceHut needs a token to
-- read even a public profile, so there is nothing to call: recognised,
-- listed, and honestly not measured.
UPDATE portfolio_platforms SET synced_by = 'code_portfolio'
 WHERE slug IN ('github', 'gitlab', 'codeberg');
