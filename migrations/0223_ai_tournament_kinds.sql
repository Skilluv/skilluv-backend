-- Two AI contest kinds, on the right column.
--
-- The backlog asked to add `ai_benchmark_rush` and `prompt_battle` to
-- `tournaments.format`. That column holds the pairing scheme — swiss, bracket
-- or ladder — and neither of those is one. A prompt battle head-to-head *is*
-- a bracket, and a benchmark rush is a ladder; what distinguishes them is
-- what people do, which is `kind`.
--
-- Putting them on `format` would have left every bracket tournament choosing
-- between "bracket" and "prompt_battle" as if those were alternatives, and
-- the pairing code reading a value it has no branch for.

ALTER TABLE tournaments
    DROP CONSTRAINT IF EXISTS tournaments_kind_check;

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_kind_check
    CHECK (kind IN (
        -- Migration 0030
        'individual', 'guild_war', 'hackathon',
        -- Migration 0114, la Grande Épreuve
        'marathon', 'defi_solitaire',
        -- AI, on a short clock.
        'benchmark_rush',  -- 48h to move a public benchmark, ladder-scored
        'prompt_battle'    -- head to head on one task, community vote
    ));

COMMENT ON COLUMN tournaments.kind IS
    'What people do. The pairing scheme is `format`, and the two are '
    'independent: a prompt battle is a bracket, a benchmark rush is a ladder.';
