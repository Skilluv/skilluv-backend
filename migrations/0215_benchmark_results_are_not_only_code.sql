-- `code_benchmark_results` becomes `benchmark_results`.
--
-- ## Why rename rather than add a second table
--
-- The backlog asked for `ai_benchmark_results`, and its columns would have
-- been the ones already sitting in `code_benchmark_results`: a name, a metric
-- with its unit and direction, baselines to compare against, a method, and
-- the code that produced the numbers. An MMLU score fits that shape exactly —
-- `lm-eval-harness` is a harness, 66.6 against Llama-3-8B is a baseline, and
-- "somebody else re-ran it" is the same event either way.
--
-- Two tables would mean two sets of constraints drifting apart, two queries
-- behind every leaderboard, and a reviewer learning the reproduction workflow
-- twice. The table was never about code; only its name was.
--
-- ## What AI needed that was missing
--
-- One column. A performance benchmark is defined by its harness and its
-- machine; an evaluation benchmark is defined by its dataset and its split,
-- and without that the number cannot be situated — MMLU on the full test set
-- and MMLU on a two-hundred-question sample are different claims.

ALTER TABLE code_benchmark_results RENAME TO benchmark_results;

ALTER INDEX idx_code_benchmark_results_slice
    RENAME TO idx_benchmark_results_slice;
ALTER INDEX idx_code_benchmark_results_reproduced
    RENAME TO idx_benchmark_results_reproduced;

ALTER TABLE benchmark_results
    RENAME CONSTRAINT code_benchmark_reproduction_is_complete
    TO benchmark_reproduction_is_complete;

ALTER TABLE benchmark_results
    -- The dataset and split the score was measured on. Required for nothing,
    -- because a latency benchmark has no dataset; named explicitly because an
    -- evaluation score without it is unsituated.
    ADD COLUMN dataset_url TEXT
        CHECK (dataset_url IS NULL OR dataset_url ~ '^https?://'),
    ADD COLUMN dataset_split VARCHAR(60);

COMMENT ON TABLE benchmark_results IS
    'Measured claims attached to a slice, with the baseline, method and code '
    'needed to dispute them. Covers latency and throughput as well as '
    'evaluation scores: a benchmark nobody can re-run is a screenshot, '
    'whichever domain it comes from.';

COMMENT ON COLUMN benchmark_results.dataset_url IS
    'Which dataset the score was measured on, for evaluation benchmarks. '
    'MMLU on the full test set and MMLU on a two-hundred-question sample are '
    'different claims, and the number alone does not say which.';

COMMENT ON COLUMN benchmark_results.dataset_split IS
    'test, validation, a named subset. The half of the previous column that a '
    'URL does not carry.';
