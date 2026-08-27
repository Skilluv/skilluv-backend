-- DB-01 -- massive volume seed for EXPLAIN work.
--
-- The bugs that hide at 50 rows and bite at 5M live in the plan, not the logic.
-- This fills `users` to a chosen size so the hot read paths (feed/for-you,
-- talent search, leaderboards, craft score) can be profiled under real volume.
--
-- Run against a THROWAWAY database only (it inserts junk accounts):
--
--   psql "$DATABASE_URL" -v n=1000000 -f scripts/seed_volume.sql
--
-- Then read the plans (pg_stat_statements should be on):
--
--   EXPLAIN (ANALYZE, BUFFERS) SELECT ... ;   -- the 30 hot queries
--
-- The bet, per the ticket: missing indexes + N+1 on feed/for-you, talent
-- search v4 and the leaderboard. A sequential scan on a 1M-row users table in
-- any of those is the thing to fix (add an index) or to justify.
--
-- These accounts never authenticate: password_hash is a fixed dummy, and the
-- emails use the reserved .invalid TLD so no mailer can ever reach them.

\set n :n
\echo Seeding :n users...

INSERT INTO users (
    email, username, password_hash,
    first_name, last_name, display_name,
    skill_domain, email_verified, profile_active, total_fragments
)
SELECT
    'loadtest_' || g || '@example.invalid',
    'loadtest_' || g,
    '$argon2id$v=19$m=19456,t=2,p=1$c2VlZA$c2VlZHNlZWRzZWVkc2VlZA',  -- dummy, never verified
    'Load',
    'Test',
    'Load Test ' || g,
    (ARRAY['code', 'design', 'game', 'security'])[1 + (g % 4)],
    TRUE,
    TRUE,
    (g % 2000)   -- spread of fragments so leaderboard/craft-score sorts do work
FROM generate_series(1, :n) AS g
ON CONFLICT (username) DO NOTHING;

\echo Done. Users now:
SELECT count(*) FROM users;

-- ── Extending to deliverables / skill_fragments ─────────────────────────────
-- The ticket also wants ~5M deliverables and ~20M skill fragments. Those tables
-- carry foreign keys (challenge/slice/reviewer ids) that must reference real
-- rows, so their seed is schema-specific and intentionally not guessed here:
-- add it against the current deliverables / skill_fragments columns, drawing
-- the user_id from the loadtest_* accounts above. Until then, the user-volume
-- seed alone already surfaces the users-table scans on the leaderboard and
-- talent-search paths.
