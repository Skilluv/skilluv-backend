-- What has been seeded, so a deployment can seed itself.
--
-- ## The problem this closes
--
-- Every seed this platform has was a command somebody had to remember to run.
-- Nothing recorded that they had, so the only way to know whether a fresh
-- database had its catalogue was to go and look — and three of the scripts
-- could not have seeded it anyway:
--
--   * `seed_oss_partners.sql` carried a hard-coded owner UUID that exists on
--     exactly one developer's machine.
--   * `seed_flagships.sql`, `seed_onboarding_challenges.sql` and both season
--     files resolved their owner as `admin@skilluv.local`, while
--     `seed_admin` creates `admin@skill-uv.com`. The lookup returned no row,
--     the `INSERT ... SELECT` inserted nothing, and the script exited 0.
--
-- A seed that silently does nothing and reports success is worse than one that
-- was never run: the second is noticed.
--
-- ## Why a ledger and not `ON CONFLICT DO NOTHING` alone
--
-- Every step is still individually idempotent — that property is not given up,
-- because a ledger row is not a substitute for a safe re-run. What the ledger
-- adds is three things `ON CONFLICT` cannot:
--
--   1. **Cost.** Boot runs the whole catalogue; without a ledger that is
--      several hundred upserts on every restart of every replica.
--   2. **A record.** "Was the catalogue ever applied to this database, and
--      when" is a question an operator asks during an incident, and reading it
--      out of the data is guesswork.
--   3. **Re-application on change.** Each step carries a version — the SHA-256
--      of its SQL, or a hand-written string for the ones written in Rust. Edit
--      a seed and the version moves, so the next deployment applies it again
--      rather than leaving the database on the old content for ever.
--
-- ## Why `applied_at` is not a primary key
--
-- One row per step, updated in place. The history of when a step re-ran is not
-- something anybody has asked for, and a growing table would need pruning; the
-- previous version and the moment it changed are what an operator reads, and
-- both are here.

CREATE TABLE seed_runs (
    -- The step's stable name, e.g. `onboarding_challenges`. Renaming a step
    -- makes it run again, which is the correct behaviour: a renamed step is a
    -- step whose identity nobody can vouch for.
    name VARCHAR(64) PRIMARY KEY,
    -- SHA-256 of the SQL, or a declared version for a step written in Rust.
    version CHAR(64) NOT NULL,
    -- What it was before the last change, so a deployment that seeded the
    -- wrong thing can be read rather than reconstructed.
    previous_version CHAR(64),
    -- What the step reported doing. Free text, because "12 created, 45
    -- updated" and "skipped: no admin account" are both worth keeping and
    -- neither is a number.
    detail TEXT,
    first_applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE seed_runs IS
    'One row per seed step that has been applied to this database. Read by '
    'services::seed at boot: a step whose version matches its row is skipped. '
    'Deleting a row makes that step run again on the next start.';
