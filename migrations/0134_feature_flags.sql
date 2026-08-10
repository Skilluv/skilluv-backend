-- SKI-33 (Hygiène pré-prod HYG-06) — DB-backed feature flags.
--
-- Complements (does not replace) the existing env-var pattern for
-- infrastructure toggles (e.g. `SKILLUV_HELLO_WALL_MIRROR_ENABLED=1`).
-- Env-vars fit bootstrap-time toggles (worker enabled?); this table
-- fits product-level runtime flags (new_feed rolled out to 20% of
-- users?).
--
-- Design decisions:
--   - `rollout_percent` is 0..100 — inclusive. 0 = disabled for all,
--     100 = enabled for all (regardless of `enabled`). `enabled=false`
--     kill-switches everyone regardless of rollout.
--   - Deterministic hash-mod on user_id (see service) so a given user
--     lands in the same bucket across restarts — no "flag flickers".
--   - Anonymous / unauthenticated requests use ip hash for stickiness
--     (best-effort, service layer detail).

CREATE TABLE IF NOT EXISTS feature_flags (
    key TEXT PRIMARY KEY CHECK (key ~ '^[a-z][a-z0-9_]{0,62}$'),
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    rollout_percent SMALLINT NOT NULL DEFAULT 100
        CHECK (rollout_percent BETWEEN 0 AND 100),
    description TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL
);

-- Index for the eventual admin dashboard filter "show only disabled" /
-- "show only partial rollout". Cheap (small table expected).
CREATE INDEX IF NOT EXISTS idx_feature_flags_status
    ON feature_flags (enabled, rollout_percent);
