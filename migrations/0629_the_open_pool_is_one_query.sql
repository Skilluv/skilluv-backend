-- The open pool gets an index, now that twelve trades read it.
--
-- ## What changed above this
--
-- `GET /api/code/first-issues` was one domain's listing of `project_slices`,
-- and the only index it could use was `idx_project_slices_orientation` - which
-- only helps when the caller names a trade. It has become
-- `GET /api/open-slices`, answering for the twelve domains, with or without an
-- orientation, and it is public and uncredentialled. That is the shape that
-- earns an index of its own.
--
-- ## Why partial, and why these columns
--
-- The predicate is the whole point: open, unclaimed, not closed. Everything a
-- slice ever becomes afterwards - claimed, in review, merged, closed - is
-- excluded permanently, so the index stays roughly the size of the pool rather
-- than of the table's history. On a platform whose whole ambition is that most
-- slices eventually get merged, that difference only grows.
--
-- The columns follow the query exactly: `primary_domain` is the filter for the
-- three surfaces that belong to every trade, then `difficulty` and
-- `created_at DESC` are the ORDER BY, so the same scan that filters also
-- orders and the LIMIT stops it early.
--
-- `slice_type` is deliberately not in the key. It is filtered through the join
-- on `slice_types`, and for a slice whose surface names its domain the domain
-- filter has already done the same work.
--
-- Nothing here is claimed to be free: this is a second index to maintain on
-- every slice insert and every claim. The ingestion rate is a poller's, and
-- the read is on a public page. That trade is the right way round.

CREATE INDEX idx_project_slices_open_pool
    ON project_slices (primary_domain, difficulty, created_at DESC)
    WHERE status = 'open'
      AND claimed_by_user_id IS NULL
      AND claimed_by_team_id IS NULL
      AND closed_at IS NULL;

COMMENT ON INDEX idx_project_slices_open_pool IS
    'The open pool as GET /api/open-slices reads it: unclaimed work, by trade, '
    'easiest and newest first. Partial on the claim predicate so it holds the '
    'pool rather than the table.';
