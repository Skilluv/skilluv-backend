-- Seeded quality challenges, one set per trade.
--
-- Drafts, as every seeded challenge is: a human publishes them after reading
-- them. Each one asks for a document or a suite somebody else can act on,
-- because that is what this domain's proof looks like — and each one carries
-- the grid of its family, so a submission is read against criteria its
-- author could read first.
--
-- ## Why several of them are aimed at things we do not control
--
-- A test plan for a product nobody uses is an exercise. Most of these name a
-- real target — an open-source project, a public site, a jam game — because
-- the thing that makes this trade hard is arriving at a system you did not
-- build and deciding what is worth putting to the test. A challenge on a toy
-- codebase removes exactly that.
--
-- ## The one the backlog asked for that is not here
--
-- "Speedrun-friendly bugs discovery" (quality/O5-04). As written it asks
-- somebody to look for exploitable defects in a game whose author has not
-- offered it, which is the thing our own charter refuses everywhere else. It
-- appears below in a form that keeps what was interesting about it — reading
-- a system against its intended rules — on a target that consents.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## What there is to do' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## What is expected' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'In every case: what is observed has to be reproducible by somebody who ' ||
    'was not there, and what was not put to the test is written down. A ' ||
    'report that lets a reader assume full coverage is refused.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid of the family applies, and it is public: you can read ' ||
    'it before you submit.',
    'quality', c.difficulty,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'quality' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'quality' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── qa-code (5) ────────────────────────────────────────────────────
('qa-code', 'A test plan before the feature',
 'Take a feature that has not been built yet and write what will be put to the test, at which level, and what will not be',
 'The plan, with the accepted risk behind each omission and who accepted it.', 2),

('qa-code', 'The test that would pass anyway',
 'Find in an existing suite the tests that would still pass with the code broken, and demonstrate it by breaking the code',
 'The list, the trace of the mutation that failed nothing, and the rewritten tests.', 3),

('qa-code', 'An end-to-end suite on a real journey',
 'Write a Playwright or Cypress suite over the most used journey of a public application whose source you have',
 'The suite, its measured run time, and what it does not cover.', 3),

('qa-code', 'A coverage figure that says where the holes are',
 'Analyse the coverage of a real project and rank the gaps by risk rather than by percentage',
 'The coverage report, the ranking, and the first gap closed.', 3),

('qa-code', 'Tests that search instead of waiting',
 'Introduce property-based tests — proptest, hypothesis, fast-check — on a module whose edge cases are poorly understood',
 'The properties stated, the counter-example found if there is one, and the fix.', 4),

-- ── qa-cyber (4) ───────────────────────────────────────────────────
('qa-cyber', 'A replayable report on a training target',
 'Run a full penetration test against a target built for it — Juice Shop, DVWA — following a written methodology',
 'The report: scope, named method, each finding with its request and its response.', 2),

('qa-cyber', 'A security test plan for an API',
 'Write the security test plan for a REST API: authentication, authorisation, rate limiting, injection, data exposure',
 'The plan, with how to verify each point and what would count as a failure.', 3),

('qa-cyber', 'A scanner in the pipeline, without the noise',
 'Add a dynamic scanner to a continuous integration pipeline and tune it until its report is readable',
 'The configuration, the alert count before and after triage, and the false positives dismissed with their reason.', 3),

('qa-cyber', 'A static analysis rule for an in-house defect',
 'Write a Semgrep or CodeQL rule that catches a defect that keeps recurring in a given project',
 'The rule, the real occurrences it found, and evidence that it does not fire on correct code.', 4),

-- ── qa-design (4) ──────────────────────────────────────────────────
('qa-design', 'Five sessions, one protocol',
 'Run a usability study with five participants over one journey of an existing product',
 'The protocol, the consents, the raw quotes, and the findings kept apart from the inferences.', 3),

('qa-design', 'An accessibility audit against a named standard',
 'Audit a real page against WCAG 2.2 level AA, by hand as well as with a tool',
 'Each defect with its exact criterion, its proposed fix and its estimated cost.', 3),

('qa-design', 'The gap between the mockup and what shipped',
 'Compare an implementation to its design specifications and report the gaps',
 'The list of gaps, ranked by what they change for the user rather than by their size.', 2),

('qa-design', 'Where people give up',
 'Map a real user journey and identify the friction points, measured rather than assumed',
 'The map, the measurement behind each friction point, and the first one to address with the reason.', 3),

-- ── qa-game (5) ────────────────────────────────────────────────────
('qa-game', 'Five sessions on a jam game',
 'Facilitate five structured playtests on a game jam entry and turn them into decisions its team can take',
 'The protocol, identical across sessions, what the players did, and the trade-offs proposed.', 2),

('qa-game', 'A balance analysis that comes with its volume',
 'Analyse the balance of a card or character game over real matches',
 'The win rates with the number of matches, the dominant strategy if there is one, and what follows from it.', 3),

('qa-game', 'Four hours without a script',
 'Run a four-hour exploratory testing session on an indie game, with its author''s agreement',
 'The session charter, reproducible defect reports, and what the session did not cover.', 3),

('qa-game', 'A protocol for one genre',
 'Design a playtest protocol suited to a specific genre — roguelike, puzzle, role-playing — and explain what the genre changes',
 'The protocol, and the trace of at least one session that put it to the test.', 3),

('qa-game', 'Playing against the rules, with permission',
 'On a game whose author has opened it to testing, look for the paths the rules did not anticipate and document what they allow',
 'The author''s written agreement, each path reproducible, and what it does or does not break.', 4),

-- ── qa-lead (3) ────────────────────────────────────────────────────
('qa-lead', 'What a team tests, and what it gives up testing',
 'Write the test strategy of a distributed team of ten: levels, tooling, ownership, cost',
 'The strategy, the list of owned omissions, and the indicator that will say whether it works.', 4),

('qa-lead', 'An initiative that survives its launch',
 'Design a quality culture initiative — defect hunts, quality champions, blameless post-mortems — and write what keeps it alive at six months',
 'The scheme, what is being asked of people, and what makes the new path easier than the old one.', 3),

('qa-lead', 'The handbook of a team growing fast',
 'Write the quality handbook of a young technical company in fast growth: hiring, tooling, rituals, and what is deliberately not done yet',
 'The handbook, and what will have to be revisited once the team has doubled.', 4)

) AS c(orientation_slug, title, description, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug;
