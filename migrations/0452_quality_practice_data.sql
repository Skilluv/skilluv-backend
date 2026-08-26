-- The quality domain: score, tiers, missions, contests, awards, review grids.
--
-- ## Four tickets that needed no table
--
-- `qa_missions` (quality/M-01) is `missions` with `skill_domain = 'quality'`
-- and mission types of its own. A parallel table would have meant a second
-- application flow, a second invoice path and a second escrow, for work that
-- differs only in what it asks for.
--
-- Quality contests (C-01) are `tournaments` with two more kinds. Quality
-- awards (C-02) are `award_categories`, and the hybrid vote the backlog asks
-- for — seventy per cent community, thirty per cent jury — is already the
-- default on `award_editions`. Cross-domain tagging (W-05) is the column
-- migration 0450 added; what remains of it is two queries, which live in the
-- route module.

-- ═══════════════════════════════════════════════════════════════════
-- What a quality score counts
-- ═══════════════════════════════════════════════════════════════════
--
-- Rows rather than a formula in Rust, so the answer can be argued with by
-- somebody who does not compile the backend.
--
-- Three departures from the backlog's formula, each one deliberate.
--
-- A confirmed bug outranks a validated test plan (45 against 40). Both are
-- documents; only one of them required somebody else to ship a fix because
-- of it, and that outcome is not something the author can produce alone.
--
-- A facilitated playtest is worth 45 and not the backlog's 20. Running five
-- sessions with real players and turning them into decisions is the same
-- shape of work as a usability study, and pricing it at a third of one said
-- that game testing is a lesser trade. It is not; it is a harder recruitment
-- problem.
--
-- `critical_bugs_confirmed` is new, and it reads the reviewer's severity
-- rather than the reporter's. Self-rated severity would have made this term
-- a self-service multiplier, which is the failure mode every bug bounty
-- programme has already found.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('quality', 'attestations_quality', 5, 'count', NULL,
     'Each quality attestation issued.', 10),
    ('quality', 'test_plans_validated', 40, 'count', NULL,
     'Each test plan reviewed and accepted — what is covered, what is not, '
     'and why.', 20),
    ('quality', 'test_strategies_validated', 90, 'count', NULL,
     'Each validated team test strategy. Worth more than a plan because it '
     'commits an organisation rather than a feature, and because the '
     'omissions it names are what somebody has to defend.', 25),
    ('quality', 'automation_suites_shipped', 55, 'count', NULL,
     'Each test suite another team runs in its own pipeline.', 30),
    ('quality', 'bugs_confirmed', 45, 'count', NULL,
     'Each reported defect whose fix shipped and was then re-checked. Worth '
     'more than a plan: the outcome depends on somebody else, and cannot be '
     'produced alone.', 40),
    ('quality', 'critical_bugs_confirmed', 60, 'count', NULL,
     'Each confirmed defect rated critical or high. The severity counted is '
     'the reviewer''s, never the reporter''s.', 50),
    ('quality', 'usability_studies_completed', 60, 'count', NULL,
     'Each usability study carried out: protocol, sessions actually held, '
     'observations kept apart from inferences.', 60),
    ('quality', 'a11y_audits_delivered', 55, 'count', NULL,
     'Each accessibility audit delivered against a named standard and level.', 65),
    ('quality', 'playtests_facilitated', 45, 'count', NULL,
     'Each validated playtest report. The same shape of work as a usability '
     'study, with a harder recruitment problem.', 70),
    ('quality', 'coverage_analyses_accepted', 50, 'count', NULL,
     'Each accepted coverage analysis. A percentage on its own is not one.', 80),
    ('quality', 'target_domains_distinct', 35, 'count', NULL,
     'Each distinct domain a verified artefact was aimed at. This trade is '
     'also judged on what it can put to the test elsewhere.', 90),
    ('quality', 'missions_completed', 100, 'count', NULL,
     'Each paid quality mission carried through.', 100),
    ('quality', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'The average of the review grids received, counted from 3 out of 5.', 110),
    ('quality', 'years_active', 25, 'count', NULL,
     'Each year since the first verified artefact.', 120),
    ('quality', 'featured_times', 200, 'count', NULL,
     'Each featuring by the community.', 130)
ON CONFLICT (skill_domain, term) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The tiers get this domain's words
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0204 copied the code tiers here so the domain would have a scale
-- before it had a catalogue. The slugs and the thresholds stay exactly as
-- they are — 0204's argument for that is right, and a profile has to be
-- comparable to itself across two domains — and only the words change.
--
-- Ops took the other route and renamed a slug. That was a mistake worth not
-- repeating: `craft_scores.tier_slug` is what the recruiter search filters
-- on, and a domain whose second tier is called something else is a domain
-- the "Senior and above" filter silently reads differently.

UPDATE craft_score_tiers SET name = v.name, description = v.description
  FROM (VALUES
    ('apprentice',  'Apprentice',
     'The first reports. Nobody has fixed anything because of them yet.'),
    ('contributor', 'Tester',
     'Finds things, describes them precisely enough to be reproduced, and '
     'watches fixes ship.'),
    ('engineer',    'Quality Engineer',
     'Decides what to put to the test before testing it. The suites shipped '
     'run on other people''s machines.'),
    ('senior',      'Senior',
     'Can say what a green suite does not prove, and get that accepted.'),
    ('staff',       'Quality Lead',
     'The way a team puts its product to the test carries this person''s '
     'mark.'),
    ('principal',   'Principal',
     'A practice that changed how other teams decide what they verify.')
  ) AS v(slug, name, description)
 WHERE craft_score_tiers.skill_domain = 'quality'
   AND craft_score_tiers.slug = v.slug;

-- ═══════════════════════════════════════════════════════════════════
-- Quality missions
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('quality_test_plan', 'quality', 'Test plan authoring',
     'Write what will be put to the test and what will not, on a product '
     'being discovered. Fixed price.', 10),
    ('quality_automation_build', 'quality', 'Test suite build',
     'Build or take over an automated suite until the team runs it without '
     'its author. Fixed price.', 20),
    ('quality_bug_bash', 'quality', 'Defect hunt',
     'A short, intensive period over a named scope. Fixed price, or per '
     'confirmed report.', 30),
    ('quality_usability_study', 'quality', 'Usability study',
     'Protocol, recruitment, sessions, debrief. Fixed price, with '
     'participant compensation billed separately.', 40),
    ('quality_a11y_audit', 'quality', 'Accessibility audit',
     'An audit against a named standard, with fixes ranked by cost and '
     'impact. Fixed price.', 50),
    ('quality_playtest_facilitation', 'quality', 'Playtest facilitation',
     'Recruit, facilitate, measure, report back. Per session, or fixed '
     'price over a series.', 60),
    ('quality_security_testing', 'quality', 'Scoped penetration testing',
     'A written scope, signed rules of engagement, a replayable report. '
     'Fixed price.', 70),
    ('quality_strategy_consulting', 'quality', 'Test strategy consulting',
     'Decide what an organisation puts to the test, and with what. Daily '
     'rate or monthly retainer.', 80)
ON CONFLICT (slug) DO NOTHING;

-- What a quality mission hands over. Rows, not a restated CHECK: 0413 made
-- these a table precisely so the sixth domain would be an INSERT.
INSERT INTO mission_deliverable_formats
    (slug, skill_domain, name, description, sort_order)
VALUES
    ('test_plan_document', 'quality', 'Test plan',
     'A document saying what is covered, what is not, and what risk that '
     'corresponds to.', 410),
    ('test_suite_repository', 'quality', 'Test suite',
     'A repository or a contribution: the tests, their run configuration, '
     'and what it takes to run them elsewhere.', 420),
    ('findings_report', 'quality', 'Findings report',
     'The defects found, each reproducible, ranked by impact.', 430),
    ('research_report', 'quality', 'Research report',
     'Protocol, participants, sessions, observations and inferences kept '
     'apart. Recordings stay with the client.', 440),
    ('session_recordings', 'quality', 'Recorded sessions',
     'The recordings of the sessions facilitated, with the participants'' '
     'consent. Delivered to the client, never to a portfolio.', 450)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Quality contests
-- ═══════════════════════════════════════════════════════════════════
--
-- Both hand something in, and neither is scored by a jury reading taste.
-- A bug bash is scored on confirmed findings, which is a count somebody
-- verifies; a playtest marathon is scored on sessions turned into decisions,
-- which the team receiving them confirms.
--
-- `target_system` is required on both for the reason the ops chaos weekend
-- requires it: a testing contest with no named target is an invitation to
-- go and break something nobody offered.

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, required_rule_keys, sort_order)
VALUES
    ('bug_bash', 'quality', 'Defect hunt',
     'An offered scope, forty-eight hours, and what was found. Counted in '
     'confirmed defects, not reported ones: reporting a lot is not finding.',
     TRUE, TRUE, FALSE, '{target_system,rules_of_engagement}', 130),
    ('playtest_marathon', 'quality', 'Playtest marathon',
     'A set of games to put to the test, sessions facilitated, and the '
     'decisions the teams were able to take from reading them.',
     TRUE, TRUE, FALSE, '{target_system}', 140)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Quality awards
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO award_categories (slug, name, description, subject_type, sort_order)
VALUES
    ('quality_test_plan_of_the_year', 'Test plan of the year',
     'The one that said most clearly what it did not cover.',
     'deliverable', 310),
    ('quality_bug_of_the_year', 'Defect report of the year',
     'The report whose reproduction was so clean that the fix shipped the '
     'same day.',
     'deliverable', 320),
    ('quality_study_of_the_year', 'Study of the year',
     'The usability or accessibility study that made a product change its '
     'mind.',
     'deliverable', 330),
    ('quality_playtest_of_the_year', 'Playtest facilitation of the year',
     'The run of sessions that taught a game team the most.',
     'deliverable', 340),
    ('quality_lead_of_the_year', 'Quality lead of the year',
     'The person whose test strategy held for a whole year without turning '
     'into a ritual.',
     'user', 350)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What a quality reviewer looks at
-- ═══════════════════════════════════════════════════════════════════
--
-- Five families, five grids, plus the domain default that applies when
-- nothing narrower does. A seeded challenge copies the matching one as its
-- rubric, so a submission is read against criteria its author could read
-- first.
--
-- The three refusals at the top of the domain grid are not criteria to
-- score. A finding nobody else can reproduce, a figure with no source, and a
-- session run without the participant's consent: none of them is compensated
-- by the quality of the rest.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('quality', NULL, 'Quality — shared criteria', '[
  {"criterion": "Reproducible by a stranger", "looks_like": "The reviewer makes the same observation by following what was written, without asking a question. A finding only its author knows how to trigger is refused."},
  {"criterion": "Every figure has its source", "looks_like": "A coverage figure comes with its report, a duration with its measurement, a rate with its denominator. Refused outright."},
  {"criterion": "Consent of the people observed", "looks_like": "Every recorded session has the participant''s written agreement, and the report does not identify them. Refused outright."},
  {"criterion": "What was not tested is written down", "looks_like": "The scope states its holes. A report that lets a reader assume full coverage is more dangerous than no report."},
  {"criterion": "Observation and inference kept apart", "looks_like": "What was seen and what is concluded from it sit in two different paragraphs."},
  {"criterion": "AI use is declared", "looks_like": "Using an assistant is declared. It is allowed; hiding it is not."}
]'),

('quality', 'automation', 'Software testing — review grid', '[
  {"criterion": "The assertion says something", "looks_like": "A test that would also pass with the code broken costs without guaranteeing anything. The reviewer looks for that one first."},
  {"criterion": "Independent of order", "looks_like": "The suite passes in a random order and in parallel. A test that depends on its neighbour is a deferred outage."},
  {"criterion": "The level is justified", "looks_like": "Unit, integration or end-to-end: why this one rather than a cheaper one."},
  {"criterion": "Test data is produced, not found", "looks_like": "Each test builds what it needs. A shared pre-filled database is an invisible dependency."},
  {"criterion": "Run time is measured", "looks_like": "A suite nobody has time to run is not run, so it does not exist."},
  {"criterion": "Flakiness is fixed, not retried", "looks_like": "An intermittent test is repaired or deleted. Putting it behind an automatic retry is a way of no longer seeing it."}
]'),

('quality', 'intrusion', 'Penetration testing — review grid', '[
  {"criterion": "The scope was written first", "looks_like": "Signed rules of engagement, and nothing outside them. Refused outright if missing."},
  {"criterion": "The method is named", "looks_like": "OWASP, PTES, or a written in-house method. An instinct is not a method, and does not transfer."},
  {"criterion": "Every finding is replayable", "looks_like": "The request, the payload, the response. A reviewer reproduces without the author."},
  {"criterion": "Severity is argued", "looks_like": "What an attacker actually gets, not a tool score copied across."},
  {"criterion": "False positives are stated", "looks_like": "What the tool flagged that was not one, with the reason. An untriaged list is tool output, not a report."},
  {"criterion": "Disclosure is bounded", "looks_like": "A timeline agreed with the affected party, and nothing published before it."}
]'),

('quality', 'usability', 'Usability and accessibility — review grid', '[
  {"criterion": "The protocol supports the conclusion", "looks_like": "Tasks are realistic, instructions do not give away the answer, and the number of sessions matches what is being claimed."},
  {"criterion": "Recruitment is described", "looks_like": "Who, how many, and why these people. Five colleagues are not five users."},
  {"criterion": "Quotes are raw", "looks_like": "What the person said, not what they meant. Rephrasing comes afterwards, flagged as such."},
  {"criterion": "The audit names its standard and level", "looks_like": "WCAG 2.2 AA, and the exact criterion in default. \"Not accessible\" is not a finding."},
  {"criterion": "Every defect has a proposed fix", "looks_like": "With its estimated cost. An audit with no way out becomes a list nobody opens."},
  {"criterion": "What works is reported too", "looks_like": "A report that only lists failures gets read once."}
]'),

('quality', 'playtest', 'Playtesting — review grid', '[
  {"criterion": "Observation outweighs opinion", "looks_like": "What the player did before what they think. \"They re-read the tutorial three times\" beats \"they found it confusing\"."},
  {"criterion": "The protocol is the same across sessions", "looks_like": "Otherwise the sessions do not add up and the synthesis compares different things."},
  {"criterion": "The facilitator does not help", "looks_like": "The moment the player gets stuck is the data. Giving away the answer destroys it."},
  {"criterion": "Balance data comes with its volume", "looks_like": "A win rate comes with the number of matches. Without it, it is not a measurement."},
  {"criterion": "Findings become decisions", "looks_like": "Each finding proposes a possible trade-off, and the game team decides."},
  {"criterion": "The player profile is stated", "looks_like": "Familiar with the genre or not, and what that changes about how to read the session."}
]'),

('quality', 'strategy', 'Test strategy — review grid', '[
  {"criterion": "What is not tested is owned", "looks_like": "A strategy claiming to cover everything has decided nothing. The reviewer looks for the list of things given up."},
  {"criterion": "The cost is quantified", "looks_like": "Machine time, human time, time waiting on a merge. A test pyramid with no cost is a diagram."},
  {"criterion": "Ownership is named", "looks_like": "Who writes, who maintains, who decides to delete. \"The team\" is not an answer."},
  {"criterion": "Success is measurable", "looks_like": "An indicator that will move if the strategy works, and will also move if it does not."},
  {"criterion": "It survives a departure", "looks_like": "The strategy holds if the person who wrote it leaves. Otherwise it is a personal practice."},
  {"criterion": "The culture change is described", "looks_like": "What is being asked of people, and what makes the new path easier than the old one."}
]')

ON CONFLICT DO NOTHING;
