-- The quality domain: opened, given its trades, its surface and its bases.
--
-- ## What was already there
--
-- Migration 0400 declared `quality` as a row with `is_active = FALSE`, and
-- 0204 gave it six craft-score tiers. Both were written for the day the
-- catalogue behind the domain existed. This is that day: the flag flips
-- because there is now something to choose, not the other way round.
--
-- ## Two things the backlog asked for that are not written here
--
-- The backlog (quality/F-02, F-03) asks to rewrite
-- `orientations_primary_domain_check` and to extend a capability enum.
-- Neither exists any more. 0400 turned the CHECK into a foreign key onto
-- `skill_domains`, and 0404 turned the capability list into a table with a
-- trigger that derives `{primary_domain}_reviewer:{reviewer_group}` from the
-- orientations themselves. So seeding the five trades below is what makes
-- their review capabilities grantable; naming them again would be the
-- restatement those two migrations exist to stop.
--
-- That has one consequence worth stating out loud, because it contradicts the
-- backlog: the capability is `quality_reviewer:automation`, not
-- `qa_reviewer:code`. `qa_reviewer` would have been a family no guard derives
-- and no trigger maintains — grantable only by hand, and invisible to
-- `require_reviewer_for_orientation`. The name follows the domain slug for
-- the same reason every other domain's does.
--
-- ## Why the trades are not named after the domain they test
--
-- The backlog names them by target: qa-code, qa-cyber, qa-design, qa-game.
-- The slugs are kept — they are the decision that was taken — but the names
-- and the reviewer families describe the practice, because that is what a
-- reviewer needs to be able to read. Somebody who can judge a Playwright
-- suite cannot judge a usability protocol, and both of them test "the
-- product". The target lives on the artefact instead (`target_domain`
-- below), where it can vary per piece of work: a QA lead writes a strategy
-- for a game team one quarter and a platform team the next, and a trade that
-- encoded the target would have needed two rows for one person.
--
-- ## Language
--
-- Seeded content in this domain is written in English. The eight domains
-- that came before hold French content, and that difference is deliberate
-- rather than an oversight: this is a public repository and English is the
-- default going forward. Existing domains are not rewritten here — that is
-- its own change, on content that has already been reviewed.

-- ═══════════════════════════════════════════════════════════════════
-- The domain opens
-- ═══════════════════════════════════════════════════════════════════

UPDATE skill_domains
   SET is_active = TRUE,
       name = 'Quality',
       description =
           'Decide what has to be put to the test, put it to the test, and '
           'write down what was found. The trade that produces evidence a '
           'product holds up.'
 WHERE slug = 'quality';

-- ═══════════════════════════════════════════════════════════════════
-- Five trades
-- ═══════════════════════════════════════════════════════════════════
--
-- `secondary_domains` is where the target of the practice is declared once,
-- so the recommendation engine can show a `qa-game` challenge to somebody
-- whose orientations are game-first. It is a hint for listings, never a
-- claim: what somebody has actually tested is on their artefacts.

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated)
VALUES
    ('qa-code',
     'Software Test Engineer',
     'Decides what has to be covered, writes the tests that cover it, and '
     'can say what a green suite does not prove.',
     'quality', ARRAY['code'],
     ARRAY['tests', 'automation', 'coverage'], TRUE),

    ('qa-cyber',
     'Disciplined Penetration Tester',
     'Follows a written methodology rather than an instinct, and hands back '
     'a report somebody else can replay.',
     'quality', ARRAY['security'],
     ARRAY['pentest', 'dast', 'sast'], TRUE),

    ('qa-design',
     'Usability and Accessibility Researcher',
     'Builds a protocol, watches people use the thing, and reports what was '
     'seen rather than what was hoped for.',
     'quality', ARRAY['design'],
     ARRAY['usability', 'a11y', 'wcag'], TRUE),

    ('qa-game',
     'Playtest Facilitator',
     'Gets people playing, measures, and turns a session into decisions. The '
     'trade where an observation outweighs an opinion.',
     'quality', ARRAY['game'],
     ARRAY['playtest', 'balance', 'exploratory'], TRUE),

    ('qa-lead',
     'Test Strategy Lead',
     'Writes down what the team puts to the test, what it does not, and why. '
     'The deliverable is an owned decision, not a coverage figure.',
     'quality', ARRAY['code', 'design', 'game', 'security'],
     ARRAY['strategy', 'test-pyramid', 'quality-culture'], TRUE)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Who may review quality work
-- ═══════════════════════════════════════════════════════════════════
--
-- Five families for five trades, which is unusual: ops has eight trades in
-- five families, code has more than thirty in eight. Here the grouping
-- collapses, and that is the honest answer rather than a modelling failure.
-- The competence is defined by what the reviewer has to be able to open — a
-- test suite, a pentest report, a session recording, a balance dataset, a
-- strategy document — and no two of those are read by the same person.
--
-- Naming them after the practice rather than the target keeps the capability
-- readable: `quality_reviewer:usability` says what the holder can judge.
-- `quality_reviewer:design` would have read as "can review design", which is
-- a capability that already exists and means something else.

UPDATE orientations SET reviewer_group = g.grp
  FROM (VALUES
    ('qa-code',   'automation'),
    ('qa-cyber',  'intrusion'),
    ('qa-design', 'usability'),
    ('qa-game',   'playtest'),
    ('qa-lead',   'strategy')
  ) AS g(slug, grp)
 WHERE orientations.slug = g.slug;

-- ═══════════════════════════════════════════════════════════════════
-- The surface quality work lives on
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO slice_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('qa_report', 'quality', 'Test report',
     'A test plan, an automated suite, a bug report, a usability study, a '
     'playtest write-up. Judged on one question: can somebody else make the '
     'same observation.', 70)
ON CONFLICT (slug) DO NOTHING;

ALTER TABLE project_slices
    ADD COLUMN qa_subtype VARCHAR(30),
    -- Which domain the work was aimed at.
    --
    -- Not `qa_target_domain`. The backlog asks for one of these on quality
    -- artefacts and another on leadership artefacts, and they would have held
    -- the same values, been read by the same two queries and drifted the way
    -- every pair of columns that means one thing eventually does. One column,
    -- carried by whichever trades work on somebody else's domain.
    --
    -- A foreign key rather than the CHECK the backlog proposed, for two
    -- reasons. The list it proposed contained `cyber`, which is not a domain
    -- slug — the domain is `security` — so every routing query written
    -- against it would have matched nothing and nobody would have found out.
    -- And the seventh domain would have meant restating it, which is the
    -- failure 0400 removed.
    --
    -- NULL means cross-domain on a slice whose type declares a target, and
    -- means nothing at all on one that does not: a code artefact is not
    -- "aimed at" a domain, it is in one, and `primary_domain` already says
    -- which.
    ADD COLUMN target_domain VARCHAR(30)
        REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    -- The tooling the work was produced with: playwright, zap, axe, junit.
    -- Same reasoning as `ops_tooling` and `code_languages` — it is what a
    -- client filters on, and it is plural because a suite written in
    -- Playwright and run under a JUnit reporter is one artefact.
    ADD COLUMN qa_tooling TEXT[] NOT NULL DEFAULT '{}';

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_qa_subtype_values
    CHECK (qa_subtype IS NULL OR qa_subtype IN (
        'test_plan',            -- what will be covered, and what will not
        'test_automation',      -- a suite somebody else's pipeline can run
        'bug_report',           -- a finding another person can reproduce
        'usability_study',      -- a protocol, sessions, and what was observed
        'a11y_audit',           -- an accessibility audit against a named standard
        'playtest_report',      -- sessions run, measured, and turned into decisions
        'coverage_analysis',    -- where the gaps are, and which ones matter
        'test_strategy'         -- what an organisation tests, and what it does not
    ));

-- A subtype only means something on a quality report, and a quality report
-- without one says nothing about what was actually produced.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_qa_subtype_belongs CHECK (
        (slice_type = 'qa_report') = (qa_subtype IS NOT NULL)
    );

COMMENT ON COLUMN project_slices.target_domain IS
    'The domain a piece of work was aimed at, for the trades that work on '
    'somebody else''s domain — quality and leadership. NULL means '
    'cross-domain on those, and means nothing on a slice type that declares '
    'no target. One column rather than one per domain: two would have held '
    'the same values and drifted.';

CREATE INDEX idx_slices_qa_subtype
    ON project_slices (qa_subtype)
    WHERE qa_subtype IS NOT NULL;

-- What the cross-domain routing reads: every artefact aimed at one domain,
-- newest first. This is the index behind `GET /api/quality/reports
-- ?target_domain=game` and behind the per-domain breakdown on a profile;
-- leadership reads it through the same column.
CREATE INDEX idx_slices_target_domain
    ON project_slices (target_domain, created_at DESC)
    WHERE target_domain IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- What a quality attestation can rest on
-- ═══════════════════════════════════════════════════════════════════
--
-- The prefix is `quality_` and not `qa_`, matching the domain slug the way
-- every other domain's bases do. The backlog wrote `qa_`; a basis whose
-- prefix does not match its `skill_domain` is one that every per-domain
-- listing has to special-case, and there are already six of them.
--
-- `requires_deliverable` is TRUE for all but two. This domain is unusual in
-- how little it needs the exception: almost everything it produces is a
-- document, and a claim of having tested something without the report is
-- exactly the claim this platform refuses. The two exceptions are a
-- featuring, which is a decision about a person, and a bug whose value is
-- that somebody else's fix shipped — the proof there is the merged fix,
-- which lives on the bug report rather than on a deliverable of its own.
--
-- Nine bases where the backlog listed six. Three of its mappings would have
-- had two different artefacts making the same claim:
--
--   * a team test strategy is not a feature test plan. One decides what an
--     organisation gives up, the other decides how one feature is covered,
--     and a recruiter filtering on the first would have got both;
--   * an accessibility audit is not a usability study. Different method,
--     different standard, different evidence, and the only thing they share
--     is that a person was involved;
--   * a suite another team runs is not a plan that was accepted, which is
--     why `quality_automation_shipped` exists rather than being folded in.

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order)
VALUES
    ('quality_test_plan_validated', 'quality', 'Test plan validated',
     'A plan that says what will be covered, what will not, and why. '
     'Reviewed and accepted.',
     TRUE, 10),
    ('quality_test_strategy_validated', 'quality', 'Test strategy validated',
     'What an organisation puts to the test and what it gives up testing, '
     'with the risk each omission accepts and who accepted it.',
     TRUE, 15),
    ('quality_automation_shipped', 'quality', 'Test suite shipped',
     'A suite another team runs in its own pipeline without its author.',
     TRUE, 20),
    ('quality_bug_report_validated', 'quality', 'Bug report confirmed',
     'A defect described precisely enough to be reproduced, whose fix '
     'shipped and was then re-checked by the person who found it.',
     FALSE, 30),
    ('quality_usability_study_completed', 'quality', 'Usability study completed',
     'A protocol, sessions that actually happened, and findings that keep '
     'what was observed apart from what was inferred.',
     TRUE, 40),
    ('quality_a11y_audit_delivered', 'quality', 'Accessibility audit delivered',
     'An audit against a named standard and level, with every defect carrying '
     'its criterion and a fix somebody costed.',
     TRUE, 45),
    ('quality_playtest_report_validated', 'quality', 'Playtest report validated',
     'Sessions facilitated, measured, and turned into decisions the game '
     'team was able to take.',
     TRUE, 50),
    ('quality_coverage_analysis_accepted', 'quality', 'Coverage analysis accepted',
     'Where the gaps are, which ones matter, and in what order to close '
     'them. A percentage on its own is not an analysis.',
     TRUE, 60),
    ('featured_quality_engineer', 'quality', 'Featured by the quality community',
     'Testing work the community singled out as exemplary.',
     FALSE, 70)
ON CONFLICT (basis) DO NOTHING;
