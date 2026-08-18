-- Every contest kind, from every domain, in one list.
--
-- ## The bug this closes
--
-- Migration 0189 added `code_golf` and `tdd_contest`. Migration 0223 added
-- `benchmark_rush` and `prompt_battle` — and, being a rewrite rather than an
-- extension, dropped the two the code series had added four migrations
-- earlier.
--
-- The merge that brought the two branches together named this failure mode
-- exactly: "a CHECK cannot be extended, only replaced, so whichever ran last
-- would have deleted the other's silently". It renumbered the AI series to
-- stop the files colliding, which fixed the visible half. This is the other
-- half: 0223 still runs after 0189, and still drops what 0189 added.
--
-- Nothing caught it because `VALID_KINDS` in the service already lists all
-- seven. A code golf would have passed the Rust validation and been refused
-- by the database, with an error naming a constraint rather than the thing
-- that was wrong.
--
-- ## Why this is a new migration rather than an edit to 0223
--
-- 0223 is committed on a branch somebody may already have applied. Editing it
-- changes its checksum, and `sqlx` refuses to migrate a database whose
-- recorded checksum no longer matches — the exact failure the
-- `hotfix/restore-migration-0068-checksum` branch exists to remember.
-- Appending is the only change that is safe on a database that already ran.
--
-- ## The lesson, written down where the next person will hit it
--
-- Any migration that restates this CHECK must restate *all* of it. There is
-- no way to add one value without rewriting the list, which means every
-- addition is an opportunity to silently delete somebody else's.

ALTER TABLE tournaments
    DROP CONSTRAINT IF EXISTS tournaments_kind_check;

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_kind_check
    CHECK (kind IN (
        -- Migration 0030, the original three.
        'individual',
        'guild_war',
        'hackathon',
        -- Migration 0114, la Grande Épreuve.
        'marathon',
        'defi_solitaire',
        -- Migration 0189, code contests. A code hackathon is a `hackathon`
        -- with a domain, not a fourth kind.
        'code_golf',
        'tdd_contest',
        -- Migration 0223, AI contests on a short clock.
        'benchmark_rush',
        'prompt_battle'
    ));

COMMENT ON CONSTRAINT tournaments_kind_check ON tournaments IS
    'Every kind, from every domain. Restating this list drops whatever is '
    'missing from it — check `services::tournament::VALID_KINDS` before '
    'rewriting.';
