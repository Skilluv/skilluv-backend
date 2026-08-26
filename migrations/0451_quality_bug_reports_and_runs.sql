-- The two things quality work produces that no other domain has a table for.
--
-- ## 1. A bug report, and the fix that proves it was one
--
-- Every domain lets somebody claim they found a problem. What makes it a
-- quality artefact is that a stranger can reproduce it, and that somebody
-- else's fix shipped because of it. The table is shaped so that both are
-- structural rather than a matter of how well the description was written:
-- reproduction steps and environment are columns, and the attestation waits
-- for the fix to be confirmed by the person who reported it.
--
-- ## 2. A test run, imported rather than typed
--
-- The backlog (quality/W-03) asks for five integrations — GitHub Actions,
-- Codecov, JUnit XML, Playwright, Postman. They are one row with a source,
-- because what a reviewer needs is identical in all five cases: how many
-- tests ran, how many failed, what the coverage was, and a link they can
-- open. Five importers writing five shapes would have meant five readers,
-- and the fifth one is where the reviewer stops looking.
--
-- ## Why an imported figure is not a proof by itself
--
-- Anybody can point at a green badge on a repository they control. So the
-- row carries where it came from and stays unverified until a reviewer says
-- otherwise, and `quality_profile` counts only the verified ones. The import
-- saves typing; it does not decide anything.

-- ═══════════════════════════════════════════════════════════════════
-- Bug reports
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE quality_bug_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The slice this was filed under. A bug report always belongs to a piece
    -- of quality work: the slice is what carries the orientation, the
    -- reviewer routing and the attestation.
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    reporter_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    title VARCHAR(200) NOT NULL CHECK (btrim(title) <> ''),

    -- The three fields that separate a report from a complaint. Length
    -- floors rather than a bare non-empty check: "it does not work" satisfies
    -- `<> ''` and is the exact thing this table exists to refuse.
    repro_steps_md TEXT NOT NULL CHECK (length(btrim(repro_steps_md)) >= 40),
    expected_md TEXT NOT NULL CHECK (btrim(expected_md) <> ''),
    observed_md TEXT NOT NULL CHECK (btrim(observed_md) <> ''),

    -- Where it happened: {"os": ..., "browser": ..., "version": ...,
    -- "device": ..., "build": ...}. An object rather than columns because
    -- the useful keys are not the same for a web app, a game build and a
    -- command-line tool, and columns would have meant five nulls each time.
    --
    -- Required and non-empty: a report whose environment is unknown cannot
    -- be reproduced by somebody who does not already share it.
    environment JSONB NOT NULL
        CHECK (jsonb_typeof(environment) = 'object' AND environment <> '{}'::JSONB),

    severity VARCHAR(10) NOT NULL
        CHECK (severity IN ('critical', 'high', 'medium', 'low')),
    -- How often it happens. Kept separate from severity because they are
    -- routinely confused, and the pair is what decides whether a fix is
    -- urgent: a crash that happens once in a thousand runs and a cosmetic
    -- glitch that happens every time are both "one bug" without it.
    reproducibility VARCHAR(12) NOT NULL
        CHECK (reproducibility IN ('always', 'often', 'sometimes', 'rare', 'once')),

    -- Screenshots, recordings, logs. URLs rather than blobs: the storage
    -- service already holds the files, and a copy here would be a second
    -- place for them to be deleted from.
    attachment_urls TEXT[] NOT NULL DEFAULT '{}',

    -- ── The half that makes it a proof ──────────────────────────────
    --
    -- Where the fix landed, and the confirmation that the reporter went back
    -- and checked. Two fields and not one: a merged pull request is somebody
    -- else's claim that it is fixed, and a report is only closed when the
    -- person who found it says it is gone.
    fix_url VARCHAR(500)
        CHECK (fix_url IS NULL OR fix_url ~ '^https://'),
    fix_confirmed_at TIMESTAMPTZ,

    -- ── Review ──────────────────────────────────────────────────────
    reviewed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    -- Set when a reviewer disagrees with the reporter's severity. Kept
    -- rather than overwriting, because the disagreement is information: a
    -- reporter who consistently files criticals that are mediums is
    -- something a mentor should see.
    severity_adjusted_to VARCHAR(10)
        CHECK (severity_adjusted_to IS NULL
               OR severity_adjusted_to IN ('critical', 'high', 'medium', 'low')),
    rejected_reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A confirmation names what was confirmed.
    CONSTRAINT a_confirmed_fix_says_where CHECK (
        fix_confirmed_at IS NULL OR fix_url IS NOT NULL
    ),
    CONSTRAINT review_is_complete CHECK (
        (reviewed_at IS NULL) = (reviewed_by IS NULL)
    ),
    -- A rejection says why, and only a reviewer can reject.
    CONSTRAINT rejection_is_reviewed_and_explained CHECK (
        rejected_reason IS NULL OR reviewed_at IS NOT NULL
    ),
    CONSTRAINT severity_adjustment_is_reviewed CHECK (
        severity_adjusted_to IS NULL OR reviewed_at IS NOT NULL
    )
);

COMMENT ON TABLE quality_bug_reports IS
    'A finding another person can reproduce. Reproduction steps, environment '
    'and severity are columns rather than prose, so that what makes a report '
    'usable does not depend on how carefully it was written out.';

COMMENT ON COLUMN quality_bug_reports.fix_confirmed_at IS
    'Set by the reporter after going back and checking, never by the fix '
    'landing. A merged pull request is somebody else''s claim that it is '
    'fixed; this column is the verification of that claim.';

COMMENT ON COLUMN quality_bug_reports.severity_adjusted_to IS
    'What the reviewer thought instead. Kept alongside the reporter''s '
    'figure rather than replacing it: a pattern of over-rating is something '
    'a mentor should be able to see.';

CREATE INDEX idx_quality_bug_reports_reporter
    ON quality_bug_reports (reporter_user_id, created_at DESC);

CREATE INDEX idx_quality_bug_reports_slice
    ON quality_bug_reports (slice_id);

-- What the review queue reads: everything nobody has looked at, oldest
-- first, worst first.
CREATE INDEX idx_quality_bug_reports_unreviewed
    ON quality_bug_reports (severity, created_at)
    WHERE reviewed_at IS NULL;

-- What the attestation sweep reads: accepted reports whose fix has been
-- confirmed and which have not produced their attestation yet.
CREATE INDEX idx_quality_bug_reports_confirmed
    ON quality_bug_reports (reporter_user_id)
    WHERE fix_confirmed_at IS NOT NULL AND rejected_reason IS NULL;

CREATE TRIGGER trg_quality_bug_reports_updated_at
    BEFORE UPDATE ON quality_bug_reports
    FOR EACH ROW EXECUTE FUNCTION touch_missions_updated_at();

-- A bug report belongs to a quality report and to nothing else. Enforced
-- here rather than left to the service, because the slice type is what
-- decides the reviewer routing: a bug report hanging off a design artefact
-- would be routed to somebody with no capability to judge it, and would sit
-- in a queue nobody reads.
CREATE FUNCTION trg_quality_bug_report_slice_is_a_report() RETURNS TRIGGER AS $$
DECLARE
    kind VARCHAR;
    subtype VARCHAR;
BEGIN
    SELECT slice_type, qa_subtype INTO kind, subtype
      FROM project_slices WHERE id = NEW.slice_id;

    IF kind <> 'qa_report' OR subtype <> 'bug_report' THEN
        RAISE EXCEPTION
            'slice % is % / %, and a bug report can only hang off a qa_report / bug_report slice',
            NEW.slice_id, kind, subtype
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_quality_bug_report_slice_is_a_report
    BEFORE INSERT OR UPDATE OF slice_id ON quality_bug_reports
    FOR EACH ROW EXECUTE FUNCTION trg_quality_bug_report_slice_is_a_report();

-- ═══════════════════════════════════════════════════════════════════
-- Imported test runs
-- ═══════════════════════════════════════════════════════════════════
--
-- The source list is a CHECK and not a table, unlike almost every other
-- vocabulary in this schema. The reason the others became tables is that
-- several domains restated them and each restatement dropped somebody's
-- value. This one is written by one importer, in one domain, and nothing
-- outside `quality` reads it. A table here would be ceremony.

CREATE TABLE quality_test_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slice_id UUID NOT NULL REFERENCES project_slices(id) ON DELETE CASCADE,
    imported_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    source VARCHAR(20) NOT NULL CHECK (source IN (
        'github_actions',  -- a workflow run, fetched through the existing app
        'codecov',         -- a coverage report on a public repository
        'junit_xml',       -- an uploaded report, parsed
        'playwright',      -- an uploaded HTML or JSON report, parsed
        'cypress',
        'postman'
    )),

    -- Where the run can be read. Required: a figure with no source is the
    -- claim this table exists to replace, not a smaller version of it.
    report_url VARCHAR(500) NOT NULL CHECK (report_url ~ '^https://'),
    -- What was being tested. Lets a reviewer check that the run belongs to
    -- the work, rather than to a green commit from three months earlier.
    commit_sha VARCHAR(40),
    repository_url VARCHAR(500)
        CHECK (repository_url IS NULL OR repository_url ~ '^https://'),

    tests_total INTEGER NOT NULL CHECK (tests_total >= 0),
    tests_failed INTEGER NOT NULL DEFAULT 0 CHECK (tests_failed >= 0),
    tests_skipped INTEGER NOT NULL DEFAULT 0 CHECK (tests_skipped >= 0),
    duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds >= 0),

    -- NULL when the source does not report coverage, which is most of them.
    -- Zero and unknown are different answers and the column says which.
    coverage_percent NUMERIC(5,2)
        CHECK (coverage_percent IS NULL
               OR (coverage_percent >= 0 AND coverage_percent <= 100)),

    -- Whatever else the source gave, kept as it arrived. Read by nothing;
    -- it exists so a parser that improves later can re-derive a column
    -- without asking people to import again.
    raw_summary JSONB,

    verified_by UUID REFERENCES users(id) ON DELETE SET NULL,
    verified_at TIMESTAMPTZ,

    imported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_run_counts_no_more_than_it_ran CHECK (
        tests_failed + tests_skipped <= tests_total
    ),
    CONSTRAINT verification_is_complete CHECK (
        (verified_at IS NULL) = (verified_by IS NULL)
    ),
    -- The same run imported twice is one run. Somebody re-importing after a
    -- parser fix updates the row rather than doubling their figures.
    UNIQUE (slice_id, source, report_url)
);

COMMENT ON TABLE quality_test_runs IS
    'A test run imported from a tool rather than typed. Five integrations, '
    'one shape: a reviewer needs the same four things from all of them, and '
    'five shapes would have meant five readers and one of them going unread.';

COMMENT ON COLUMN quality_test_runs.verified_at IS
    'Anybody can point at a green badge on a repository they control. The '
    'score counts verified runs only; the import saves typing and decides '
    'nothing.';

CREATE INDEX idx_quality_test_runs_slice
    ON quality_test_runs (slice_id, imported_at DESC);

CREATE INDEX idx_quality_test_runs_unverified
    ON quality_test_runs (imported_at)
    WHERE verified_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Revision rounds for quality work
-- ═══════════════════════════════════════════════════════════════════
--
-- The backlog (quality/W-02) asks for an iteration workflow and points at
-- the design ticket for the mechanism. Migration 0412 already built it for
-- every domain, so this is rows: the vocabulary of what a quality round is
-- about, and the limit.
--
-- Four rather than audio's five, and the backlog's reasoning holds — this
-- work is more factual than creative, and a fourth round on a bug report
-- usually means the two people disagree about what the product should do,
-- which is not a testing question.

INSERT INTO revision_round_kinds (slug, skill_domain, name, description, sort_order) VALUES
    ('quality_repro_insufficient', 'quality', 'Reproduction insufficient',
     'The reviewer cannot reproduce the finding from what was written. The '
     'most frequent round, and the only one that invalidates the report if '
     'it does not converge.', 210),
    ('quality_severity_disputed', 'quality', 'Severity disputed',
     'The defect is real, the scale is not. Settled by naming the user '
     'impact, not by negotiating the word.', 220),
    ('quality_coverage_gap', 'quality', 'Coverage gap',
     'The plan or the suite leaves out a path the brief named.', 230),
    ('quality_evidence_missing', 'quality', 'Evidence missing',
     'A figure with no source, a session with no recording, a coverage claim '
     'with no report.', 240),
    ('quality_protocol_revision', 'quality', 'Protocol needs rework',
     'The method does not support what it concludes: recruitment, tasks, or '
     'the number of sessions.', 250)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO revision_round_limits (skill_domain, max_rounds, rationale) VALUES
    ('quality', 4,
     'Four. One pass on reproduction, one on evidence, one on severity and '
     'one on form cover the journey of a report. A fourth round on substance '
     'means the two people disagree about what the product should do, and '
     'that is not a testing question.')
ON CONFLICT (skill_domain) DO NOTHING;
