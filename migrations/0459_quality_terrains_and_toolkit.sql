-- Where a quality contributor practises, and what they practise with.
--
-- ## Three tickets, one table
--
-- The backlog asks for a toolkit page (quality/G-02), a seed of open-source
-- projects that welcome test contributions (T-01), and a curated feed of
-- projects asking for QA help (T-03) — the last one with a table of its own,
-- `external_qa_requests`.
--
-- All three are `external_resources` rows with a category. A second table
-- would have meant a second curation workflow, a second admin screen, and a
-- second place for a dead link to survive. Migration 0426 made exactly this
-- call for the ops toolkit and `external_cloud_programs`, and 0458 turned the
-- category list into a table so this one costs an INSERT.
--
-- ## Why `projects` rows are not seeded here
--
-- `projects.owner_id` is NOT NULL and points at a user or a guild. A
-- migration has neither, and inventing a system account to own thirty
-- upstream repositories would have put rows on the platform that claim
-- somebody stewards them when nobody does. A curated resource is an honest
-- pointer; a project row is a commitment, and it is made when a steward takes
-- it.
--
-- ## `access_note` is the column that matters here
--
-- More than in any other domain. Two of the five quality trades cannot
-- practise without other people's systems or other people's time, and the
-- difference between a tool with a real free tier and one with a trial is the
-- difference between a first month that happens and one that does not.

-- ═══════════════════════════════════════════════════════════════════
-- Practice targets — systems built to be tested against
-- ═══════════════════════════════════════════════════════════════════
--
-- The category exists because "where do I practise" is this domain's first
-- question and its hardest one. A penetration tester with no authorised
-- target has no trade; a usability researcher with no participants has a
-- protocol and nothing else.

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('owasp-juice-shop', 'OWASP Juice Shop', 'practice_target', 'quality',
     'https://owasp.org/www-project-juice-shop/',
     'The standard first target: a deliberately vulnerable web application '
     'covering the whole OWASP top ten and a good deal more.',
     'Free. Runs in one container, no account, no network exposure needed.',
     ARRAY['qa-cyber'], 10),

    ('dvwa', 'DVWA', 'practice_target', 'quality',
     'https://github.com/digininja/DVWA',
     'Older and narrower than Juice Shop, and still the clearest place to '
     'see one injection class at a time at four difficulty levels.',
     'Free, self-hosted. Never expose it to a network you do not own.',
     ARRAY['qa-cyber'], 20),

    ('hack-the-box', 'Hack The Box', 'practice_target', 'quality',
     'https://www.hackthebox.com/',
     'Structured targets with a progression, and a community that writes up '
     'its solutions.',
     'Free tier is enough to start; retired machines need a subscription.',
     ARRAY['qa-cyber'], 30),

    ('itch-io-jams', 'itch.io game jams', 'practice_target', 'quality',
     'https://itch.io/jams',
     'Jams end every weekend, and end with dozens of games nobody has ever '
     'watched a stranger play. Their authors almost always say yes.',
     'Free. Ask the author before running sessions, and say what they get '
     'back.',
     ARRAY['qa-game'], 40),

    ('a11y-supports', 'a11ysupport.io', 'practice_target', 'quality',
     'https://a11ysupport.io/',
     'What each assistive technology actually announces for a given pattern. '
     'The reference for the two thirds of accessibility defects no automated '
     'tool finds.',
     'Free, no account.',
     ARRAY['qa-design'], 50)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Test runners and the tooling around them
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('playwright', 'Playwright', 'test_runner', 'quality',
     'https://playwright.dev/',
     'End-to-end browser testing with the auto-waiting that removes most of '
     'the flakiness people blame on end-to-end testing.',
     'Free, open source. Browsers download on first install, about 400 MB.',
     ARRAY['qa-code', 'qa-design'], 110),

    ('cypress', 'Cypress', 'test_runner', 'quality',
     'https://www.cypress.io/',
     'End-to-end testing with a runner you watch. Easier to start with than '
     'Playwright, harder to run in parallel.',
     'The runner is free and open source; the dashboard is a paid service '
     'you do not need.',
     ARRAY['qa-code'], 120),

    ('vitest', 'Vitest', 'test_runner', 'quality',
     'https://vitest.dev/',
     'Unit and integration testing for anything built with Vite. Fast enough '
     'that people actually run it while writing.',
     'Free, open source.',
     ARRAY['qa-code'], 130),

    ('pytest', 'pytest', 'test_runner', 'quality',
     'https://docs.pytest.org/',
     'The Python default, and the one whose fixture model is worth learning '
     'even if you write another language.',
     'Free, open source.',
     ARRAY['qa-code'], 140),

    ('cargo-nextest', 'cargo-nextest', 'test_runner', 'quality',
     'https://nexte.st/',
     'A Rust test runner that isolates each test in its own process, which '
     'is what makes order-dependence visible instead of intermittent.',
     'Free, open source.',
     ARRAY['qa-code'], 150),

    ('hypothesis', 'Hypothesis', 'test_tooling', 'quality',
     'https://hypothesis.readthedocs.io/',
     'Property-based testing for Python: state what should always be true '
     'and let it hunt for the counter-example.',
     'Free, open source. `proptest` is the Rust equivalent, `fast-check` the '
     'JavaScript one.',
     ARRAY['qa-code'], 160),

    ('stryker-mutator', 'Stryker Mutator', 'test_tooling', 'quality',
     'https://stryker-mutator.io/',
     'Mutation testing: breaks the code on purpose and reports which tests '
     'did not notice. The fastest way to find the tests that prove nothing.',
     'Free, open source. Slow — run it on one module, not the repository.',
     ARRAY['qa-code'], 170),

    ('pact', 'Pact', 'test_tooling', 'quality',
     'https://pact.io/',
     'Contract testing: verify that two services still agree without '
     'deploying them together.',
     'The libraries are free and open source; the broker can be self-hosted.',
     ARRAY['qa-code'], 180),

    ('codecov', 'Codecov', 'test_tooling', 'quality',
     'https://about.codecov.io/',
     'Coverage reporting with a per-pull-request diff, which is the only '
     'coverage view that changes anybody''s behaviour.',
     'Free for public repositories.',
     ARRAY['qa-code', 'qa-lead'], 190),

    ('postman', 'Postman', 'test_tooling', 'quality',
     'https://www.postman.com/',
     'API request collections that double as a test suite, runnable in a '
     'pipeline through Newman.',
     'Free tier is generous; the collection format is the part worth '
     'learning and it is portable.',
     ARRAY['qa-code', 'qa-cyber'], 200)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Security scanners
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('owasp-zap', 'OWASP ZAP', 'security_scanner', 'quality',
     'https://www.zaproxy.org/',
     'The dynamic scanner that runs in a pipeline. Its baseline mode is the '
     'one worth adding first: fast, and quiet enough to keep.',
     'Free, open source. Fully featured — there is no paid edition holding '
     'anything back.',
     ARRAY['qa-cyber'], 310),

    ('semgrep', 'Semgrep', 'security_scanner', 'quality',
     'https://semgrep.dev/',
     'Static analysis where writing your own rule takes minutes, which is '
     'what makes it useful for in-house defects rather than generic ones.',
     'The engine and community rules are free; some rule packs are paid.',
     ARRAY['qa-cyber', 'qa-code'], 320),

    ('codeql', 'CodeQL', 'security_scanner', 'quality',
     'https://codeql.github.com/',
     'Query a codebase as if it were a database. Steeper than Semgrep, and '
     'reaches defects that need data flow to see.',
     'Free for public repositories on GitHub Actions.',
     ARRAY['qa-cyber'], 330),

    ('nuclei', 'Nuclei', 'security_scanner', 'quality',
     'https://github.com/projectdiscovery/nuclei',
     'Template-driven scanning. Fast, noisy, and only as good as the triage '
     'that follows it.',
     'Free, open source. Rate-limit it — the defaults are aggressive.',
     ARRAY['qa-cyber'], 340),

    ('trivy', 'Trivy', 'security_scanner', 'quality',
     'https://trivy.dev/',
     'Dependency, container and configuration scanning in one binary.',
     'Free, open source.',
     ARRAY['qa-cyber', 'qa-code'], 350)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Accessibility and research tooling
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('axe-devtools', 'axe DevTools', 'a11y_tooling', 'quality',
     'https://www.deque.com/axe/devtools/',
     'The automated checker most other tools wrap. Finds roughly a third of '
     'real issues, and is honest about which third.',
     'The browser extension is free; the pro features are not, and are not '
     'needed to audit.',
     ARRAY['qa-design'], 410),

    ('nvda', 'NVDA', 'a11y_tooling', 'quality',
     'https://www.nvaccess.org/',
     'A free screen reader for Windows. Twenty minutes with it finds things '
     'no automated pass will.',
     'Free, open source. On macOS, VoiceOver is built in; on Linux, Orca.',
     ARRAY['qa-design'], 420),

    ('wave', 'WAVE', 'a11y_tooling', 'quality',
     'https://wave.webaim.org/',
     'An in-page visual overlay of accessibility issues. Useful precisely '
     'because it shows them in place rather than in a list.',
     'Free, browser extension or hosted.',
     ARRAY['qa-design'], 430),

    ('wcag-quickref', 'WCAG 2.2 Quick Reference', 'a11y_tooling', 'quality',
     'https://www.w3.org/WAI/WCAG22/quickref/',
     'The criteria themselves, filterable by level. This is where a finding '
     'gets its number, and a finding with no number is not a finding.',
     'Free, no account.',
     ARRAY['qa-design'], 440),

    ('maze', 'Maze', 'research_tooling', 'quality',
     'https://maze.co/',
     'Unmoderated remote testing: tasks, recordings, and aggregated paths.',
     'Free tier caps the number of responses per study — enough for a first '
     'study, not for a series.',
     ARRAY['qa-design'], 450),

    ('obs-studio', 'OBS Studio', 'research_tooling', 'quality',
     'https://obsproject.com/',
     'Session recording with no per-seat cost and no third party holding '
     'the footage — which matters when the consent form says where it goes.',
     'Free, open source. The honest default when a client will not allow a '
     'hosted research tool.',
     ARRAY['qa-design', 'qa-game'], 460)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Where this trade is practised on somebody else's code
-- ═══════════════════════════════════════════════════════════════════
--
-- The backlog names five projects (quality/T-01). They are here as curated
-- resources rather than as `projects` rows, for the reason at the top of this
-- file, and the summary says what kind of contribution each one actually
-- takes — which is the part a beginner cannot find out without reading a
-- year of issues.

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('vitest-contributing', 'Vitest — contributing tests', 'hub', 'quality',
     'https://github.com/vitest-dev/vitest/blob/main/CONTRIBUTING.md',
     'A test framework whose own suite is readable, which makes it an '
     'unusually good first place to add a case.',
     'Look for issues labelled `p2-to-be-discussed` with a reproduction '
     'attached: they usually need a failing test before anything else.',
     ARRAY['qa-code'], 510),

    ('prettier-tests', 'Prettier — language test cases', 'hub', 'quality',
     'https://github.com/prettier/prettier/blob/main/CONTRIBUTING.md',
     'A formatter tested almost entirely by snapshot cases per language. '
     'Adding a case for an unhandled construct is a self-contained first '
     'contribution.',
     'The test format is documented and the review is fast.',
     ARRAY['qa-code'], 520),

    ('godot-testing', 'Godot Engine — issue reproduction', 'hub', 'quality',
     'https://github.com/godotengine/godot/blob/master/CONTRIBUTING.md',
     'An engine whose issue tracker is full of reports that need a minimal '
     'reproduction project. Producing one is a quality contribution the '
     'maintainers explicitly ask for.',
     'The `needs testing` and `needs more info` labels are the entry point.',
     ARRAY['qa-game', 'qa-code'], 530),

    ('bevy-regression', 'Bevy Engine — regression tests', 'hub', 'quality',
     'https://github.com/bevyengine/bevy/blob/main/CONTRIBUTING.md',
     'A fast-moving engine where a regression test attached to a fix is '
     'welcome and rarely offered.',
     'Rust. The examples directory doubles as the manual test suite.',
     ARRAY['qa-code', 'qa-game'], 540),

    ('home-assistant-qa', 'Home Assistant — integration testing', 'hub', 'quality',
     'https://developers.home-assistant.io/docs/development_testing',
     'Thousands of community integrations, most of them under-tested, and a '
     'documented harness for testing them.',
     'Python. A single integration is a scope one person can hold.',
     ARRAY['qa-code'], 550)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Skilluv as a quality terrain
-- ═══════════════════════════════════════════════════════════════════
--
-- The dogfooding argument, in a domain where it is unusually clean: a
-- contributor who finds a defect here can point at the fix and say "that
-- shipped because of me", and the reviewer is the person who maintains the
-- thing.
--
-- ## What this is honest about
--
-- These are functional and usability defects on our own surfaces. Security
-- findings are not in scope and do not belong here: the security domain has
-- its own disclosure path, and a defect that turns out to be exploitable
-- moves there rather than being written up in public. `projects.bug_bounty_
-- open` and `bug_bounty_scope` are what carry that distinction once a steward
-- registers our repositories; nothing below touches production data or
-- credentials.
--
-- ## Attribution
--
-- Anything confirmed here produces an attestation like any other artefact,
-- and the fact that Skilluv is the beneficiary changes nothing about how it
-- is reviewed. A platform that graded contributions to itself more
-- generously than contributions elsewhere would be worth nothing to a
-- recruiter.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## What there is to do' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Scope' || E'\n\n' ||
    c.scope || E'\n\n' ||
    'Functional, usability and accessibility defects only. Anything that ' ||
    'looks exploitable stops here and goes to the disclosure address in ' ||
    'SECURITY.md instead — writing it up in public is not the favour it ' ||
    'looks like.' || E'\n\n' ||
    '## What is expected' || E'\n\n' ||
    c.expected || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid of the family applies, and it is public. Work on our ' ||
    'own surfaces is graded exactly as work on anybody else''s.',
    'quality', c.difficulty,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'quality' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'quality' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

('qa-code', 'A failing test for an open Skilluv issue',
 'Take an open issue on a Skilluv repository and turn it into a failing test before anybody writes the fix',
 'A Skilluv repository, on your own clone. No production access is involved or required.',
 'The test, failing on the current code, and the issue it corresponds to.', 2),

('qa-code', 'A test plan for a Skilluv feature under discussion',
 'Pick a Skilluv feature that has been specified but not built, and write what should be put to the test and what should not',
 'Whatever is currently in the public roadmap or an open design issue.',
 'The plan, with its omissions and the risk each one accepts.', 3),

('qa-design', 'An accessibility audit of one Skilluv page',
 'Audit one page of the Skilluv application against WCAG 2.2 AA, by hand as well as with a tool',
 'Any public page. Keyboard, screen reader and zoom passes included.',
 'Each defect with its criterion, its proposed fix and its estimated cost.', 3),

('qa-design', 'A usability study of the Skilluv onboarding',
 'Run a usability study on the Skilluv sign-up and first-challenge journey with five participants who have never seen it',
 'The public journey. Written consent required before any recording.',
 'The protocol, the raw quotes, and the findings kept apart from the inferences.', 3),

('qa-game', 'Sessions on a Skilluv canvas game',
 'Facilitate structured playtests on one of the games published on the Skilluv canvas and turn them into decisions',
 'Any published canvas game, with its author informed.',
 'The protocol, what the players did, and the trade-offs proposed to the author.', 2),

('qa-lead', 'Read our test strategy back to us',
 'Read the Skilluv backend test suite and write what it says our strategy is — then what it does not cover and what risk that accepts',
 'The public repository. Nothing to run against production.',
 'The reconstructed strategy, the omissions found, and which of them look deliberate.', 4)

) AS c(orientation_slug, title, description, scope, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug;

-- ═══════════════════════════════════════════════════════════════════
-- The toolkit page
-- ═══════════════════════════════════════════════════════════════════
--
-- One row per locale, listing what the resources above are for. It exists
-- because a flat catalogue answers "what is there" and not "what do I install
-- on day one", and day one is the question beginners actually have.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

('toolkit-quality', 'toolkit', 'quality', NULL, 'en',
 'Quality toolkit',
 'What to install on day one, per trade, and what the free tier actually covers.',
$md$
# Quality toolkit

Everything here has a version you can use without paying and without an
employer. Where that stops being true, the note says exactly where.

## Everybody, whatever the trade

- **A note-taking habit with timestamps.** Not a tool recommendation. The
  single thing that separates a reproducible finding from a remembered one.
- **A screen recorder** — OBS Studio is free and keeps the footage on your
  machine, which matters when a consent form says where it goes.

## Software test engineer

| | |
|---|---|
| Browser end-to-end | **Playwright** — auto-waiting removes most flakiness |
| Unit / integration | **Vitest**, **pytest**, **cargo-nextest** by language |
| Property-based | **Hypothesis** (Python), **proptest** (Rust), **fast-check** (JS) |
| Mutation | **Stryker** — run it on one module, never the repository |
| Contracts | **Pact** |
| Coverage | **Codecov** — free for public repositories |
| API | **Postman** collections, run in CI with Newman |

Start with the runner the project already uses. Introducing a second one is a
contribution nobody asked for.

## Penetration tester

| | |
|---|---|
| Dynamic scanning | **OWASP ZAP** — baseline mode first |
| Static analysis | **Semgrep** for in-house rules, **CodeQL** for data flow |
| Templates | **Nuclei** — rate-limit it, the defaults are aggressive |
| Dependencies and images | **Trivy** |
| Targets | **Juice Shop**, **DVWA**, **HackTheBox** |

The scanners are the easy half. The triage is the trade.

## Usability and accessibility researcher

| | |
|---|---|
| Automated checking | **axe DevTools** — finds about a third |
| Screen reader | **NVDA** (Windows), VoiceOver (macOS), Orca (Linux) |
| In-page overlay | **WAVE** |
| The criteria | **WCAG 2.2 Quick Reference** |
| Remote studies | **Maze** — free tier caps responses per study |
| Recording | **OBS Studio** |

Budget for participant compensation before budgeting for tools. It is the
larger number and the one people forget.

## Playtest facilitator

| | |
|---|---|
| Games to test | **itch.io jam pages**, sorted by newest |
| Recording | **OBS Studio** |
| Balance data | A spreadsheet. Genuinely. |

The scarce resource here is players, not software.

## Test strategy lead

| | |
|---|---|
| Coverage trend | **Codecov** |
| Suite timing | Whatever your CI already reports — read it before adding anything |

The three numbers that matter — suite duration, share of failures that were
real, time from ready to merged — come from tools you already have. Nobody
needs to buy anything to start.
$md$, 60),

('toolkit-quality', 'toolkit', 'quality', NULL, 'fr',
 'Boîte à outils qualité',
 'Ce qu''on installe le premier jour, par métier, et ce que l''offre gratuite couvre réellement.',
$md$
# Boîte à outils qualité

Tout ce qui suit a une version utilisable sans payer et sans employeur. Là où
ça cesse d''être vrai, la note dit exactement où.

## Tout le monde, quel que soit le métier

- **Une habitude de prise de notes horodatées.** Ce n''est pas une
  recommandation d''outil. C''est la seule chose qui sépare un constat
  reproductible d''un constat dont on se souvient.
- **Un enregistreur d''écran** — OBS Studio est gratuit et garde les
  enregistrements sur ta machine, ce qui compte quand un formulaire de
  consentement dit où ils vont.

## Ingénieur de test logiciel

| | |
|---|---|
| Bout en bout navigateur | **Playwright** — l''attente automatique retire l''essentiel de l''instabilité |
| Unitaire / intégration | **Vitest**, **pytest**, **cargo-nextest** selon le langage |
| Par propriétés | **Hypothesis** (Python), **proptest** (Rust), **fast-check** (JS) |
| Mutation | **Stryker** — sur un module, jamais sur le dépôt |
| Contrats | **Pact** |
| Couverture | **Codecov** — gratuit pour les dépôts publics |
| API | Collections **Postman**, lancées en CI avec Newman |

Commence par le lanceur que le projet utilise déjà. En introduire un second
est une contribution que personne n''a demandée.

## Testeur d''intrusion

| | |
|---|---|
| Balayage dynamique | **OWASP ZAP** — mode baseline d''abord |
| Analyse statique | **Semgrep** pour les règles maison, **CodeQL** pour le flux de données |
| Modèles | **Nuclei** — limite le débit, les valeurs par défaut sont agressives |
| Dépendances et images | **Trivy** |
| Cibles | **Juice Shop**, **DVWA**, **HackTheBox** |

Les scanners sont la moitié facile. Le tri est le métier.

## Chercheur en utilisabilité et accessibilité

| | |
|---|---|
| Vérification automatique | **axe DevTools** — trouve environ un tiers |
| Lecteur d''écran | **NVDA** (Windows), VoiceOver (macOS), Orca (Linux) |
| Superposition dans la page | **WAVE** |
| Les critères | **Référence rapide WCAG 2.2** |
| Études à distance | **Maze** — l''offre gratuite plafonne les réponses par étude |
| Enregistrement | **OBS Studio** |

Prévois le défraiement des participants avant de prévoir les outils. C''est le
plus gros montant et celui qu''on oublie.

## Animateur de playtests

| | |
|---|---|
| Jeux à tester | Les pages de jam **itch.io**, triées par nouveauté |
| Enregistrement | **OBS Studio** |
| Données d''équilibrage | Un tableur. Vraiment. |

Ici la ressource rare, ce sont les joueurs, pas les logiciels.

## Responsable de la stratégie de test

| | |
|---|---|
| Tendance de couverture | **Codecov** |
| Durée de la suite | Ce que ta CI rapporte déjà — lis-le avant d''ajouter quoi que ce soit |

Les trois chiffres qui comptent — durée de la suite, part des échecs qui
étaient réels, temps entre « prêt » et « fusionné » — viennent d''outils que tu
as déjà. Personne n''a besoin d''acheter quoi que ce soit pour commencer.
$md$, 60);
