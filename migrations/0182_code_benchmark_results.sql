-- Performance claims, in a shape that can be disputed.
--
-- ## Why a table and not a paragraph
--
-- "Twice as fast" is the most common claim in a code portfolio and the least
-- checkable. Written as prose it cannot be compared, reproduced or refuted;
-- written as rows it can be all three.
--
-- ## What makes a benchmark admissible here
--
-- A baseline, a method, and the code that produced the numbers. All three are
-- required, and the constraint says so rather than the documentation:
--
--   * without a baseline, "twice as fast" has no second term;
--   * without a method, nobody can tell whether the comparison was fair —
--     same machine, same input, warm or cold;
--   * without the code, nobody can run it again, and a benchmark that cannot
--     be re-run is a screenshot.
--
-- Reproduction is what a reviewer checks. `reproduced_at` records that
-- somebody did, and by whom.

CREATE TABLE code_benchmark_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,

    benchmark_name VARCHAR(120) NOT NULL
        CHECK (length(btrim(benchmark_name)) > 0),
    -- What was measured, in the unit it was measured in. Two columns rather
    -- than one string, so "1.2 ms" and "1200 us" compare.
    metric_name VARCHAR(60) NOT NULL
        CHECK (length(btrim(metric_name)) > 0),
    metric_unit VARCHAR(20) NOT NULL
        CHECK (length(btrim(metric_unit)) > 0),
    metric_value DOUBLE PRECISION NOT NULL,

    -- Lower is better for latency, higher for throughput. Without this the
    -- direction of "improvement" is guessed from the metric name.
    lower_is_better BOOLEAN NOT NULL,

    -- What it is being compared against: [{"name": "...", "value": 1.0}].
    -- At least one, or the claim has no second term.
    comparison_baselines JSONB NOT NULL
        CHECK (jsonb_typeof(comparison_baselines) = 'array'
               AND jsonb_array_length(comparison_baselines) >= 1),

    -- How it was run: hardware, input size, warm-up, number of iterations.
    -- Free text because the honest answer varies, required because its
    -- absence is what makes a benchmark unfalsifiable.
    methodology_md TEXT NOT NULL
        CHECK (length(btrim(methodology_md)) >= 40),

    -- The harness. criterion, pytest-benchmark, JMH, go test -bench.
    harness VARCHAR(40),
    code_url TEXT NOT NULL
        CHECK (code_url ~ '^https?://'),

    -- Set when a reviewer ran it again and got comparable numbers.
    reproduced_at TIMESTAMPTZ,
    reproduced_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    reproduction_notes TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Reproduction is an event with an author. Half of it says nothing.
    CONSTRAINT code_benchmark_reproduction_is_complete
        CHECK (
            (reproduced_at IS NULL AND reproduced_by_user_id IS NULL)
            OR (reproduced_at IS NOT NULL AND reproduced_by_user_id IS NOT NULL)
        )
);

COMMENT ON TABLE code_benchmark_results IS
    'Performance claims attached to a slice, with the baseline, method and '
    'code needed to dispute them. A benchmark nobody can re-run is a '
    'screenshot.';

COMMENT ON COLUMN code_benchmark_results.lower_is_better IS
    'Whether a smaller number is the better one. Latency and throughput move '
    'in opposite directions and the metric name alone does not say which.';

CREATE INDEX idx_code_benchmark_results_slice
    ON code_benchmark_results (slice_id);

CREATE INDEX idx_code_benchmark_results_reproduced
    ON code_benchmark_results (reproduced_at)
    WHERE reproduced_at IS NOT NULL;
