-- Whether a test run's numbers were read or typed.
--
-- `quality_test_runs` took the figures as input and kept a public link as the
-- proof. That is the platform's usual shape — declared, then verified by a
-- person — and here it does not hold, for a reason specific to what these
-- numbers are.
--
-- The figure a quality attestation rests on is the number of tests. Checking
-- it means opening a CI artefact and counting, and nobody does that more than
-- twice. So the numbers were declared, unchecked in practice, and attested:
-- exactly the shape this platform exists to refuse everywhere else.
--
-- `services::junit` reads them now. `figures_source` records which happened,
-- because a parsed run and a typed one are two different claims and a
-- reviewer is entitled to know which they are looking at.
--
-- Only `junit_xml` is parsed today. The other five sources stay declared:
-- a GitHub Actions run needs a token to reach its artefacts, and Codecov's
-- figure is a coverage percentage rather than a test count. The column says
-- so per row instead of the distinction living in somebody's memory.

ALTER TABLE quality_test_runs
    ADD COLUMN figures_source VARCHAR(10) NOT NULL DEFAULT 'declared'
        CHECK (figures_source IN ('declared', 'parsed'));

COMMENT ON COLUMN quality_test_runs.figures_source IS
    'declared: somebody typed these numbers and the report link is the proof. '
    'parsed: the report was fetched and read, and the numbers are what it '
    'says. A reviewer checks a different thing in each case.';

-- The rows that exist predate the parser and were all typed. The default
-- says so, and this states it rather than leaving it to be inferred from a
-- default that could change later.
UPDATE quality_test_runs SET figures_source = 'declared' WHERE figures_source IS NULL;
