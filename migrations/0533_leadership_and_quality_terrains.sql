-- Leadership and quality get terrains, in the table the other domains use.
--
-- ## The mistake this corrects
--
-- Migrations 0459 and 0469 put these two domains' "where can I practise"
-- answers into `external_resources`, arguing that a second table would have
-- meant a second curation workflow. That was wrong on the facts: 0418 had
-- already built the table, generic by design — audio, communication and
-- education all seed into it — and it carries the one thing
-- `external_resources` does not, which is the adoption workflow. A terrain
-- proposal is a shortlist entry with `adopted_project_id` waiting to be
-- filled; a curated resource is a link. Leadership and quality therefore had
-- no adoptable terrain at all, and nothing said so because nothing looked.
--
-- ## What moves and what stays
--
-- Only the rows that are actually upstream repositories move. The leadership
-- `governance` category held six rows: four are repositories where the
-- coordination work of a project happens in public and a newcomer can write
-- a proposal, and two — the Python PEP index and the Open Source Guides
-- chapter — are reading, which is what `learning` is for. They are recategorised
-- rather than moved.
--
-- Quality's `practice_target` rows do not move at all, and this is the part
-- worth being precise about: Juice Shop and DVWA are applications built to be
-- attacked, not projects asking for contributions. Practising on them is the
-- point and contributing to them is not, so they stay in the toolkit where
-- the category description already says what they are. Quality's real
-- terrains — projects that welcome test and accessibility work — did not
-- exist anywhere, and are seeded below.
--
-- ## About `ingestion_labels`
--
-- These are the labels the polling worker of 0480 watches once a terrain is
-- adopted. They are a researched starting point, not a guarantee: label sets
-- get renamed upstream, and a steward confirms them at adoption. Where a
-- project's exact taxonomy was not certain the list stays to the labels that
-- are stable across the ecosystem rather than guessing a specific one, since
-- a wrong label produces silence and a broad one produces noise a steward can
-- narrow.

-- ═══════════════════════════════════════════════════════════════════
-- Leadership — where coordination happens in public
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO terrain_proposals
    (slug, name, skill_domain, kind, upstream_url, ingestion_labels, why_md, sort_order)
VALUES

('rust-rfcs', 'Rust — the RFC process', 'leadership', 'oss_repo',
 'https://github.com/rust-lang/rfcs',
 ARRAY['T-lang', 'T-libs-api', 'T-compiler'],
 'The clearest public record of how a large project decides anything. An RFC '
 'is written by whoever wants the change, argued in the open, and either '
 'merged or closed with the reasoning attached — which means the whole '
 'artefact of a technical decision, alternatives included, is readable before '
 'you ever write one. Contributing does not require write access to anything: '
 'shepherding somebody else''s stalled RFC is real coordination work and is '
 'chronically short of people.', 10),

('bevy-rfcs', 'Bevy — the RFC process', 'leadership', 'oss_repo',
 'https://github.com/bevyengine/rfcs',
 ARRAY['S-Needs-Review', 'S-Needs-Champion'],
 'Smaller than Rust''s and correspondingly easier to matter in. The process '
 'names a champion for each proposal, which is the role this domain is about '
 'and which the project openly asks for. A first contribution can be taking '
 'over a proposal whose author stopped answering.', 20),

('godot-proposals', 'Godot — the proposal repository', 'leadership', 'oss_repo',
 'https://github.com/godotengine/godot-proposals',
 ARRAY['discussion', 'needs professional review'],
 'A repository that holds only proposals, separate from the engine. The '
 'discussion is the work, and the volume is the problem: proposals go stale '
 'for want of somebody willing to summarise a hundred comments into a '
 'decision. That summarising is the trade.', 30),

('kubernetes-community', 'Kubernetes — SIG governance', 'leadership', 'oss_repo',
 'https://github.com/kubernetes/community',
 ARRAY['good first issue', 'help wanted', 'sig/contributor-experience'],
 'The largest published governance model in open source: charters, election '
 'procedures, escalation paths, all versioned in one repository. Heavy, and '
 'the reason it is here anyway — a contributor-experience SIG that explicitly '
 'takes newcomers is a place to do real organisational work at a scale no '
 'small project can offer.', 40),

('opentofu', 'OpenTofu — a project built out of a fork', 'leadership', 'oss_repo',
 'https://github.com/opentofu/opentofu',
 ARRAY['good first issue', 'help wanted', 'rfc'],
 'A project that had to invent its governance in public and under time '
 'pressure, and wrote down what it decided. Its RFC directory is young enough '
 'that a proposal still changes the shape of things, and the technical '
 'steering committee publishes how it works.', 50),

('nodejs-node', 'Node.js — a collaborator model in the open', 'leadership', 'oss_repo',
 'https://github.com/nodejs/node',
 ARRAY['good first issue', 'help wanted', 'meta'],
 'Consensus-seeking with a documented objection procedure, a technical '
 'steering committee whose minutes are public, and a working-group structure '
 'somebody can join without being nominated. The `meta` label is where the '
 'coordination questions live rather than the code ones.', 60);

-- The two that are reading rather than terrain.
UPDATE external_resources
   SET category = 'learning'
 WHERE domain = 'leadership'
   AND slug IN ('python-peps', 'oss-governance-models');

-- ═══════════════════════════════════════════════════════════════════
-- Quality — projects that ask for test and accessibility work
-- ═══════════════════════════════════════════════════════════════════
--
-- Chosen for one property above all: the project has an existing test suite
-- and treats a failing test as a contribution. A project with no suite turns
-- "write a test" into "argue for testing", which is a fine thing to do and a
-- terrible first contribution.
--
-- Three of the six are the repositories this platform already contributes to
-- elsewhere, which is deliberate: a steward who is already known upstream is
-- worth more to a newcomer than a better repository with nobody in it.

INSERT INTO terrain_proposals
    (slug, name, skill_domain, kind, upstream_url, ingestion_labels, why_md, sort_order)
VALUES

('cal-com', 'Cal.com — end-to-end coverage', 'quality', 'oss_repo',
 'https://github.com/calcom/cal.com',
 ARRAY['bug', 'help wanted', 'good first issue'],
 'A booking product, which means time zones, recurrence and calendar '
 'integrations — the three areas where a bug report with a reproduction is '
 'worth more than a patch. The Playwright suite is real and runs in CI, so a '
 'failing test attached to an issue is a contribution the maintainers can act '
 'on immediately.', 10),

('excalidraw', 'Excalidraw — a canvas that has to survive real input',
 'quality', 'oss_repo',
 'https://github.com/excalidraw/excalidraw',
 ARRAY['bug', 'good first issue', 'help wanted'],
 'Direct manipulation on a canvas is where usability testing finds things no '
 'unit test can: pointer behaviour under a trackpad, selection that does the '
 'wrong thing at the edge of a shape, keyboard access to something built for '
 'a mouse. Small enough to hold in your head, popular enough that a finding '
 'matters.', 20),

('home-assistant-core', 'Home Assistant — the suite that asks for help',
 'quality', 'oss_repo',
 'https://github.com/home-assistant/core',
 ARRAY['needs tests', 'help wanted', 'good first issue'],
 'One of the few projects large enough to carry a standing label for missing '
 'coverage, which removes the hardest part of starting: knowing what is worth '
 'testing. Thousands of integrations, each small and self-contained, so the '
 'first contribution is scoped whether or not anybody scopes it for you.', 30),

('godot-engine', 'Godot — regression and playtest reporting',
 'quality', 'oss_repo',
 'https://github.com/godotengine/godot',
 ARRAY['needs testing', 'regression', 'confirmed'],
 'An engine with a release cycle that depends on people testing pre-releases '
 'and saying what broke. `needs testing` is an open request: somebody has '
 'described a problem and nobody has reproduced it. Reproducing it, on '
 'hardware the maintainers do not have, is a whole contribution.', 40),

('gutenberg', 'Gutenberg — accessibility that is actually triaged',
 'quality', 'oss_repo',
 'https://github.com/WordPress/gutenberg',
 ARRAY['Accessibility (a11y)', 'Needs Testing', 'good first issue'],
 'An editor used by people who did not choose it, inside a platform with a '
 'stated accessibility commitment and a team that triages against it. That '
 'combination is rare: most projects accept accessibility findings and few '
 'have anybody whose job is to act on them.', 50),

('owasp-wstg', 'OWASP Web Security Testing Guide', 'quality', 'oss_repo',
 'https://github.com/OWASP/wstg',
 ARRAY['good first issue', 'help wanted', 'content'],
 'The document a security tester works from, maintained as a repository. '
 'Contributing means writing or correcting a test procedure — the artefact '
 'this domain produces anyway — and it is reviewed by people who run those '
 'procedures for a living. The rare terrain where the deliverable and the '
 'contribution are the same object.', 60);
