-- The quality skill tree, and which trade needs which part of it.
--
-- ## What is deliberately not created here
--
-- The tree already knows about testing. `unit-testing`, `integration-testing`,
-- `e2e-testing-playwright`, `property-based-testing`, `test-doubles`,
-- `test-fixture-design` and `python-pytest` are code nodes; `usability-testing`,
-- `usability-heuristics`, `accessibility-design` and four `a11y-*` nodes are
-- design nodes; `playtesting-methodology` is a game node; `load-testing` is an
-- ops node; `bug-report-quality` is a soft-skills node.
--
-- Every one of them is what a quality trade needs, and every one of them
-- would have been re-created here under a `qa-` prefix if this migration had
-- followed the backlog's "~40 relations, 8 skills per orientation" literally.
-- Two nodes meaning the same thing is worse than a missing one: the skill
-- tree deduplicates nothing, the recommendation engine would show both, and a
-- profile would claim two skills for one competence.
--
-- So the map below points at them, and only what genuinely has no node
-- anywhere is created.
--
-- ## Where a node lives when two domains want it
--
-- It stays where it was first declared. `usability-testing` is a design node
-- because design declared it, and a `qa-design` orientation pointing at it is
-- exactly what `orientation_skill_map` is for — the map crosses domains, the
-- nodes do not have to.

-- ═══════════════════════════════════════════════════════════════════
-- The families
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('test-strategy', 'Test strategy',
 'Deciding what is put to the test, at which level, at what cost, and what is given up. The written omission is half the trade.', 'quality'),
('test-automation-craft', 'Test automation craft',
 'Writing suites that hold: independent, fast, and failing when the code is wrong.', 'quality'),
('defect-reporting', 'Defect reporting',
 'Describing precisely enough that a stranger makes the same observation, and ranking by impact rather than by irritation.', 'quality'),
('exploratory-testing', 'Exploratory testing',
 'Searching without a script, with a charter and a trace. The discipline that separates exploring from wandering.', 'quality'),
('user-research-methods', 'User research methods',
 'Protocol, recruitment, facilitation, debrief. What makes it possible to conclude anything from the sessions held.', 'quality'),
('security-testing-practice', 'Security testing practice',
 'A written scope, a named method, a replayable report, a bounded disclosure.', 'quality'),
('quality-culture', 'Quality culture',
 'Making the path that produces quality the easiest one. An imposed ritual empties out in six months.', 'quality')
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What each family contains
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT c.slug, c.display_name, c.description, 'quality', p.id
FROM (VALUES

-- Test strategy
('test-level-selection',   'Test level selection',       'Unit, integration or end-to-end: why this one rather than a cheaper one.', 'test-strategy'),
('risk-based-testing',     'Risk-based testing',         'Testing first what costs the most when it breaks, not what is easiest to cover.', 'test-strategy'),
('coverage-analysis',      'Coverage analysis',          'Where the gaps are and which ones matter. A percentage on its own is not an analysis.', 'test-strategy'),
('test-data-strategy',     'Test data strategy',         'Producing what each test needs rather than depending on a pre-filled database.', 'test-strategy'),
('regression-suite-design','Regression suite design',    'What is kept, what is deleted, and the time budget the team gives itself.', 'test-strategy'),
('quality-metrics',        'Quality metrics',            'The ones that will move if it works — and also if it does not.', 'test-strategy'),

-- Automation craft
('test-flakiness-triage',  'Flaky test triage',          'Fix or delete. An automatic retry is a way of no longer seeing them.', 'test-automation-craft'),
('mutation-testing',       'Mutation testing',           'Breaking the code on purpose to find the tests that would pass anyway.', 'test-automation-craft'),
('contract-testing',       'Contract testing',           'Pact, schemas. Testing the agreement between two services without deploying them together.', 'test-automation-craft'),
('visual-regression-testing','Visual regression testing','Comparing renderings, and handling the noise that produces.', 'test-automation-craft'),
('api-test-automation',    'API test automation',        'Postman, REST Assured, raw requests. The level with the best coverage per unit of cost.', 'test-automation-craft'),
('test-suite-performance', 'Test suite performance',     'A suite nobody has time to run is not run, so it does not exist.', 'test-automation-craft'),

-- Defect reporting
('reproduction-writing',   'Writing a reproduction',     'The steps, the environment, the expected and the observed. The rest is context.', 'defect-reporting'),
('severity-assessment',    'Severity assessment',        'What the user loses, not what a tool score displays.', 'defect-reporting'),
('defect-triage',          'Defect triage',              'Deciding the order, with the people who will have to fix them.', 'defect-reporting'),
('fix-verification',       'Fix verification',           'Going back to look. A merged fix is a claim, not a proof.', 'defect-reporting'),

-- Exploratory
('session-based-testing',  'Session-based testing',      'A charter, a duration, a trace. What makes exploration replayable.', 'exploratory-testing'),
('boundary-analysis',      'Boundary analysis',          'Zero, one, the maximum, one more. Where the defects live.', 'exploratory-testing'),
('game-balance-analysis',  'Game balance analysis',      'Win rates with their number of matches. Without volume it is not a measurement.', 'exploratory-testing'),

-- User research
('usability-protocol-design','Usability protocol design','Realistic tasks, instructions that do not give away the answer.', 'user-research-methods'),
('participant-recruitment','Participant recruitment',    'Who, how many, and why these people. Five colleagues are not five users.', 'user-research-methods'),
('session-facilitation',   'Session facilitation',       'Not helping. The moment the person gets stuck is the data.', 'user-research-methods'),
('research-synthesis',     'Research synthesis',         'Keeping what was observed apart from what is inferred, in two paragraphs.', 'user-research-methods'),
('wcag-auditing',          'WCAG auditing',              'Against a named standard and level, by hand as much as by tool.', 'user-research-methods'),
('research-consent',       'Consent and research data',  'Written agreement, anonymisation, and recordings that stay with the client.', 'user-research-methods'),

-- Security testing
('rules-of-engagement',    'Rules of engagement',        'The scope written and signed beforehand. Without them there is no test, there is an intrusion.', 'security-testing-practice'),
('dast-orchestration',     'DAST orchestration',         'ZAP, Nuclei in a pipeline, tuned until the report is readable.', 'security-testing-practice'),
('sast-rule-authoring',    'SAST rule authoring',        'Semgrep, CodeQL. Catching an in-house defect without firing on correct code.', 'security-testing-practice'),
('false-positive-triage',  'False positive triage',      'What the tool flagged that was not one, with the reason. An untriaged list is tool output.', 'security-testing-practice'),

-- Quality culture
('bug-bash-facilitation',  'Defect hunt facilitation',   'A scope, a duration, and a triage planned for the end.', 'quality-culture'),
('blameless-postmortem-practice','Blameless post-mortem practice','What the system allowed. There is no column for who typed what.', 'quality-culture'),
('quality-mentoring',      'Quality mentoring',          'Bringing somebody up in a trade that is learned mostly by reviewing.', 'quality-culture')

) AS c(slug, display_name, description, parent_slug)
JOIN skill_nodes p ON p.slug = c.parent_slug
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Which trade needs which
-- ═══════════════════════════════════════════════════════════════════
--
-- Core means somebody without it cannot do the job. Everything else is
-- recommended, which is not the same as "less important": a playtest
-- facilitator who has never written a Playwright suite is still one.
--
-- `JOIN skill_nodes` rather than a subquery per row: a slug that does not
-- exist drops out silently, and the guard against that is the count assertion
-- in `tests/test_quality_domain.rs`, which fails if the map comes out short.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended)
SELECT o.id, s.id, m.is_core, NOT m.is_core
FROM (VALUES

-- ── qa-code ────────────────────────────────────────────────────────
('qa-code', 'test-level-selection',       TRUE),
('qa-code', 'unit-testing',               TRUE),
('qa-code', 'integration-testing',        TRUE),
('qa-code', 'test-data-strategy',         TRUE),
('qa-code', 'test-flakiness-triage',      TRUE),
('qa-code', 'coverage-analysis',          TRUE),
('qa-code', 'e2e-testing-playwright',     FALSE),
('qa-code', 'property-based-testing',     FALSE),
('qa-code', 'mutation-testing',           FALSE),
('qa-code', 'contract-testing',           FALSE),
('qa-code', 'test-doubles',               FALSE),
('qa-code', 'test-fixture-design',        FALSE),
('qa-code', 'api-test-automation',        FALSE),
('qa-code', 'test-suite-performance',     FALSE),
('qa-code', 'regression-suite-design',    FALSE),
('qa-code', 'boundary-analysis',          FALSE),
('qa-code', 'reproduction-writing',       FALSE),
('qa-code', 'ci-cd',                      FALSE),

-- ── qa-cyber ───────────────────────────────────────────────────────
('qa-cyber', 'rules-of-engagement',       TRUE),
('qa-cyber', 'reproduction-writing',      TRUE),
('qa-cyber', 'severity-assessment',       TRUE),
('qa-cyber', 'dast-orchestration',        TRUE),
('qa-cyber', 'false-positive-triage',     TRUE),
('qa-cyber', 'sast-rule-authoring',       FALSE),
('qa-cyber', 'api-test-automation',       FALSE),
('qa-cyber', 'boundary-analysis',         FALSE),
('qa-cyber', 'session-based-testing',     FALSE),
('qa-cyber', 'writeup-quality',           FALSE),
('qa-cyber', 'defect-triage',             FALSE),

-- ── qa-design ──────────────────────────────────────────────────────
('qa-design', 'usability-protocol-design', TRUE),
('qa-design', 'participant-recruitment',   TRUE),
('qa-design', 'session-facilitation',      TRUE),
('qa-design', 'research-synthesis',        TRUE),
('qa-design', 'research-consent',          TRUE),
('qa-design', 'wcag-auditing',             TRUE),
('qa-design', 'usability-testing',         FALSE),
('qa-design', 'usability-heuristics',      FALSE),
('qa-design', 'accessibility-design',      FALSE),
('qa-design', 'a11y-screen-reader-design', FALSE),
('qa-design', 'a11y-color-contrast',       FALSE),
('qa-design', 'a11y-focus-states',         FALSE),
('qa-design', 'visual-regression-testing', FALSE),
('qa-design', 'reproduction-writing',      FALSE),

-- ── qa-game ────────────────────────────────────────────────────────
('qa-game', 'session-based-testing',      TRUE),
('qa-game', 'session-facilitation',       TRUE),
('qa-game', 'game-balance-analysis',      TRUE),
('qa-game', 'reproduction-writing',       TRUE),
('qa-game', 'playtesting-methodology',    TRUE),
('qa-game', 'participant-recruitment',    FALSE),
('qa-game', 'research-synthesis',         FALSE),
('qa-game', 'boundary-analysis',          FALSE),
('qa-game', 'severity-assessment',        FALSE),
('qa-game', 'research-consent',           FALSE),
('qa-game', 'defect-triage',              FALSE),

-- ── qa-lead ────────────────────────────────────────────────────────
('qa-lead', 'test-strategy',              TRUE),
('qa-lead', 'risk-based-testing',         TRUE),
('qa-lead', 'quality-metrics',            TRUE),
('qa-lead', 'quality-culture',            TRUE),
('qa-lead', 'defect-triage',              TRUE),
('qa-lead', 'test-level-selection',       FALSE),
('qa-lead', 'regression-suite-design',    FALSE),
('qa-lead', 'coverage-analysis',          FALSE),
('qa-lead', 'bug-bash-facilitation',      FALSE),
('qa-lead', 'blameless-postmortem-practice', FALSE),
('qa-lead', 'quality-mentoring',          FALSE),
('qa-lead', 'test-suite-performance',     FALSE)

) AS m(orientation_slug, skill_slug, is_core)
JOIN orientations o ON o.slug = m.orientation_slug
JOIN skill_nodes s ON s.slug = m.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;
