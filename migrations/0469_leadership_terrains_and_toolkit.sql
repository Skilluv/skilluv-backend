-- Where a leadership contributor practises, and what with.
--
-- ## Three tickets, one table
--
-- The backlog asks for a toolkit page (leadership/G-02), a seed of
-- open-source projects that welcome leadership contributions (T-02), and a
-- curated feed of external mentorship programmes (T-03) — the last one with a
-- table of its own, `external_mentorship_programs`.
--
-- All three are `external_resources` rows with a category, the same call
-- migration 0426 made for ops and 0459 for quality. A second table would have
-- meant a second curation workflow, a second admin screen, and a second place
-- for a dead link to survive.
--
-- ## `access_note` matters more here than anywhere except quality
--
-- Every conventional route into these trades runs through already having a
-- team. The note on each row below says what it costs to get in without one:
-- an RFC repository anybody can propose to, a programme with an open
-- application, a community that will let somebody help run it. That is the
-- column this domain is curated around.

INSERT INTO external_resource_categories
    (slug, skill_domain, name, description, sort_order)
VALUES
    ('governance', 'leadership', 'Governance and decision records',
     'Projects whose decisions are made in public, which is where somebody '
     'with no organisation can read how it is actually done — and propose.', 410),
    ('planning_tooling', 'leadership', 'Planning tooling',
     'Where the plan lives. The note says what the free tier holds, because '
     'the ceiling is usually the number of people rather than the number of '
     'items.', 420),
    ('people_tooling', 'leadership', 'People tooling',
     'Ladders, feedback, engagement surveys. Almost all of it is priced per '
     'seat, and the note says what is readable without buying.', 430),
    ('mentorship_programme', 'leadership', 'Mentorship programmes',
     'Structured programmes somebody can lead in, run by organisations other '
     'than this one. Each has an application window, and the note says when.', 440),
    ('community_tooling', 'leadership', 'Community tooling',
     'Running a space: moderation, events, membership.', 450)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Where decisions are made in public
-- ═══════════════════════════════════════════════════════════════════
--
-- The single most useful thing for somebody entering this domain, and the one
-- nobody points them at. A project with a public RFC process is a corpus of
-- real technical decisions, with their alternatives, their objections and
-- their outcomes — and it accepts proposals from people it has never met.

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('rust-rfcs', 'Rust RFCs', 'governance', 'leadership',
     'https://github.com/rust-lang/rfcs',
     'Fifteen years of technical decisions with their alternatives, their '
     'objections and what happened next. The best-documented decision corpus '
     'in open source.',
     'Free to read, and open to proposals from anybody. Read twenty accepted '
     'and five rejected before writing one.',
     ARRAY['lead-tech'], 10),

    ('bevy-rfcs', 'Bevy RFCs', 'governance', 'leadership',
     'https://github.com/bevyengine/rfcs',
     'A younger process on a fast-moving engine, which means the decisions '
     'are still being taken rather than already settled.',
     'Free, and the maintainers actively want proposals. Lower barrier than '
     'Rust''s, and the reviews are unusually generous.',
     ARRAY['lead-tech'], 20),

    ('godot-proposals', 'Godot proposals', 'governance', 'leadership',
     'https://github.com/godotengine/godot-proposals',
     'Feature and direction proposals for an engine with a large community. '
     'Where product framing and technical framing meet in public.',
     'Free. The rejected ones with long discussions are the most instructive.',
     ARRAY['lead-tech', 'lead-product'], 30),

    ('kubernetes-sig-governance', 'Kubernetes SIG governance', 'governance', 'leadership',
     'https://github.com/kubernetes/community',
     'How a very large project divides responsibility: special interest '
     'groups, charters, and what happens when two of them disagree.',
     'Free. The charter documents are the useful part, not the meeting '
     'notes.',
     ARRAY['lead-project', 'lead-community'], 40),

    ('python-peps', 'Python enhancement proposals', 'governance', 'leadership',
     'https://peps.python.org/',
     'The longest-running public decision process in the industry, with a '
     'template that has survived thirty years of use.',
     'Free. PEP 1 is the process; the rejected PEPs are where the reasoning '
     'is.',
     ARRAY['lead-tech'], 50),

    ('oss-governance-models', 'Open source governance models', 'governance', 'leadership',
     'https://opensource.guide/leadership-and-governance/',
     'BDFL, meritocracy, liberal contribution — what each one optimises for '
     'and what each one breaks under.',
     'Free.',
     ARRAY['lead-community', 'lead-project'], 60)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Planning, people and community tooling
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
    ('linear-app', 'Linear', 'planning_tooling', 'leadership',
     'https://linear.app/',
     'Issue tracking with a delivery model rather than a board: cycles, '
     'projects, and a roadmap that is derived from the work instead of drawn '
     'beside it.',
     'Free tier caps at 250 issues, which is a real ceiling on a live '
     'project and enough to learn the model.',
     ARRAY['lead-project', 'lead-product'], 110),

    ('github-projects', 'GitHub Projects', 'planning_tooling', 'leadership',
     'https://docs.github.com/en/issues/planning-and-tracking-with-projects',
     'Planning attached to the repository the work is in, which removes the '
     'synchronisation problem every other tool creates.',
     'Free on public repositories, including the views and the automation.',
     ARRAY['lead-project', 'lead-tech'], 120),

    ('excalidraw', 'Excalidraw', 'planning_tooling', 'leadership',
     'https://excalidraw.com/',
     'Diagrams for a decision record. Deliberately rough, which stops a '
     'sketch reading as a finished architecture.',
     'Free, open source, works without an account.',
     ARRAY['lead-tech', 'lead-project'], 130),

    ('adr-tools', 'Architecture decision record tools', 'planning_tooling', 'leadership',
     'https://adr.github.io/',
     'The templates and the tooling for keeping decision records in the '
     'repository they concern.',
     'Free. Start with Michael Nygard''s one-page template and add nothing '
     'until it hurts.',
     ARRAY['lead-tech'], 140),

    ('rework-basecamp', 'Shape Up', 'planning_tooling', 'leadership',
     'https://basecamp.com/shapeup',
     'An argument for fixed time and variable scope, written by people who '
     'run their company on it. Disagree with it after reading it, not before.',
     'Free to read in full online.',
     ARRAY['lead-product', 'lead-project'], 150),

    ('career-ladders-collection', 'progression.fyi', 'people_tooling', 'leadership',
     'https://progression.fyi/',
     'Dozens of real published career ladders, side by side. The fastest way '
     'to see the difference between one written in behaviours and one written '
     'in adjectives.',
     'Free, no account.',
     ARRAY['lead-people'], 210),

    ('rands-in-repose', 'Rands in Repose', 'people_tooling', 'leadership',
     'https://randsinrepose.com/archives/',
     'Twenty years of writing about managing engineers, by somebody who has '
     'done it badly and said so.',
     'Free. The archive is the value; the recent posts are not the entry '
     'point.',
     ARRAY['lead-people'], 220),

    ('interviewing-rubrics', 'Structured interviewing guidance', 'people_tooling', 'leadership',
     'https://rework.withgoogle.com/en/guides/hiring-use-structured-interviewing',
     'Why the same questions in the same order with the same rubric beats a '
     'good interviewer''s judgement, with the research behind it.',
     'Free.',
     ARRAY['lead-people'], 230),

    ('discourse-forum', 'Discourse', 'community_tooling', 'leadership',
     'https://www.discourse.org/',
     'Forum software built around the idea that a conversation worth having '
     'is worth finding again — which chat is bad at.',
     'Open source and self-hostable; the hosted plan is not free. The '
     'moderation model is worth reading whether or not you run one.',
     ARRAY['lead-community'], 310),

    ('community-canvas', 'Community Canvas', 'community_tooling', 'leadership',
     'https://community-canvas.org/',
     'A structured way to answer who a community is for, what brings people '
     'back, and what it costs to run.',
     'Free, and it is a worksheet rather than a book.',
     ARRAY['lead-community'], 320),

    ('outreachy', 'Outreachy', 'mentorship_programme', 'leadership',
     'https://www.outreachy.org/',
     'Paid internships in open source for people underrepresented in tech, '
     'with mentors drawn from the projects. Leading a project here is a real '
     'cohort with real outcomes.',
     'Two rounds a year. Mentor applications open roughly two months before '
     'each round; no employer required to mentor.',
     ARRAY['lead-mentor'], 410),

    ('google-summer-of-code', 'Google Summer of Code', 'mentorship_programme', 'leadership',
     'https://summerofcode.withgoogle.com/',
     'Students paired with open-source projects for a summer. Mentoring here '
     'produces a documented outcome somebody else confirms.',
     'Annual. Mentors join through a participating organisation rather than '
     'individually — find the project first.',
     ARRAY['lead-mentor'], 420),

    ('season-of-docs', 'Google Season of Docs', 'mentorship_programme', 'leadership',
     'https://developers.google.com/season-of-docs',
     'The documentation equivalent, and an easier first mentoring engagement '
     'because the deliverable is unambiguous.',
     'Annual, smaller, and less contested than Summer of Code.',
     ARRAY['lead-mentor', 'lead-community'], 430)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Skilluv as a leadership terrain
-- ═══════════════════════════════════════════════════════════════════
--
-- Three roles the platform actually needs somebody to hold, opened to the
-- community as challenges rather than as appointments.
--
-- ## Why these are term-limited
--
-- A community role with no end date is a role somebody holds until they burn
-- out, and then nobody replaces them because the handover was never designed.
-- Every one of these states a term and ends with a handover document — which
-- is itself the leadership artefact.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## What there is to do' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Term' || E'\n\n' ||
    c.term || E'\n\n' ||
    'The role ends with a handover document written for whoever takes it '  ||
    'next. A community role with no end date is one somebody holds until '  ||
    'they burn out, after which nobody replaces them because the handover '  ||
    'was never designed.' || E'\n\n' ||
    '## What is expected' || E'\n\n' ||
    c.expected || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid of the family applies, and it is public. Work on our ' ||
    'own community is graded exactly as work on anybody else''s.',
    'leadership', c.difficulty,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'leadership' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'leadership' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

('lead-mentor', 'Skilluv cohort lead',
 'Design and run a Skilluv cohort for people entering one trade, from its curriculum to its graduation',
 'One cohort, start to conclusion. Typically three to six months.',
 'The curriculum, the run, and the outcome report with the denominator — how many joined, how many finished, and what you would change.', 4),

('lead-community', 'Skilluv community operations',
 'Hold the running of the Skilluv community spaces for a term: events, moderation, and the handover at the end',
 'Six months, renewable once. Two people may hold it together.',
 'The operating playbook, the record of what was run, and one number that moved with the evidence behind it.', 3),

('lead-product', 'Skilluv roadmap contributor',
 'Read what the community is actually asking for and turn it into a prioritised proposal the maintainers can argue with',
 'One quarter. The proposal is advisory — the maintainers decide.',
 'The synthesis of what was asked, the priority order defended, and the list of what you are proposing not to do.', 3)

) AS c(orientation_slug, title, description, term, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug;

-- ═══════════════════════════════════════════════════════════════════
-- The toolkit page
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary,
     body_md, sort_order)
VALUES

('toolkit-leadership', 'toolkit', 'leadership', NULL, 'en',
 'Leadership toolkit',
 'Almost none of this is software. What there is, and where the free tier actually stops.',
$md$
# Leadership toolkit

The uncomfortable answer first: **most of this trade needs no tooling at all.**
A decision record is a text file. A career ladder is a table. A retrospective
is an hour and a shared document.

What follows is what genuinely helps, and where the free tier stops.

## Everybody

| | |
|---|---|
| Somewhere to write | A repository. Not a wiki — a repository, so decisions have a history and a diff |
| **Excalidraw** | Diagrams rough enough that a sketch does not read as a finished architecture |
| A template | One page. Add fields only when their absence has cost you something |

## Where to read real decisions

This is the resource nobody points beginners at, and the most valuable one.

- **Rust RFCs** — fifteen years of decisions with their alternatives and
  objections. Read twenty accepted and five rejected before writing one.
- **Bevy RFCs** — younger, faster-moving, and the maintainers actively want
  proposals.
- **Godot proposals** — where product framing and technical framing meet in
  public.
- **Python PEPs** — the longest-running process in the industry. PEP 1 is the
  process; the rejected ones are where the reasoning is.

All free, all open to proposals from people they have never met.

## Product and delivery

| | |
|---|---|
| **Linear** | Free tier caps at 250 issues — a real ceiling on a live project, enough to learn the model |
| **GitHub Projects** | Free on public repositories, including views and automation |
| **Shape Up** | Free to read. Disagree with it after reading it, not before |

## People

| | |
|---|---|
| **progression.fyi** | Dozens of real published career ladders side by side. The fastest way to see behaviours versus adjectives |
| **re:Work on structured interviewing** | Why the same questions in the same order beat a good interviewer's judgement |
| **Rands in Repose** archive | Twenty years of managing engineers, by somebody who did it badly and said so |

Almost every people *tool* is priced per seat and none of them is necessary.
A ladder in a document and one-to-one notes in a file will take you further
than any of them.

## Community

| | |
|---|---|
| **Discourse** | Open source and self-hostable; the hosted plan is not. The moderation model is worth reading either way |
| **Community Canvas** | A worksheet for who it is for and what brings people back |

## Mentoring at scale

| | |
|---|---|
| **Outreachy** | Two rounds a year, paid, and no employer required to mentor |
| **Google Summer of Code** | Annual; mentors join through a participating project, so find the project first |
| **Season of Docs** | Smaller and less contested. An easier first engagement because the deliverable is unambiguous |
| **Skilluv cohorts** | Free, here, and startable this week |

## What to spend money on

Nothing, for the first year. If something eventually earns it, it will be the
planning tool your team is already fighting — and by then you will be able to
say exactly what it has to do.
$md$, 70),

('toolkit-leadership', 'toolkit', 'leadership', NULL, 'fr',
 'Boîte à outils leadership',
 'Presque rien de tout ça n''est un logiciel. Ce qu''il y a, et où l''offre gratuite s''arrête vraiment.',
$md$
# Boîte à outils leadership

La réponse inconfortable d'abord : **l'essentiel de ce métier ne demande aucun
outil.** Une fiche de décision est un fichier texte. Une grille de progression
est un tableau. Une rétrospective, c'est une heure et un document partagé.

Ce qui suit est ce qui aide réellement, et où l'offre gratuite s'arrête.

## Tout le monde

| | |
|---|---|
| Un endroit où écrire | Un dépôt. Pas un wiki — un dépôt, pour que les décisions aient un historique et un diff |
| **Excalidraw** | Des schémas assez grossiers pour qu'un croquis ne se lise pas comme une architecture finie |
| Un modèle | Une page. N'ajoute un champ que quand son absence t'a coûté quelque chose |

## Où lire de vraies décisions

C'est la ressource vers laquelle personne n'oriente les débutants, et la plus
précieuse.

- **RFC Rust** — quinze ans de décisions avec leurs alternatives et leurs
  objections. Lis-en vingt acceptées et cinq rejetées avant d'en écrire une.
- **RFC Bevy** — plus jeunes, plus rapides, et les mainteneurs veulent
  activement des propositions.
- **Propositions Godot** — là où le cadrage produit et le cadrage technique se
  rencontrent en public.
- **PEP Python** — le plus ancien processus public de l'industrie. PEP 1 est le
  processus ; les rejetées sont là où est le raisonnement.

Tout est gratuit, et tout accepte des propositions de gens qu'ils n'ont jamais
rencontrés.

## Produit et livraison

| | |
|---|---|
| **Linear** | L'offre gratuite plafonne à 250 tickets — un vrai plafond sur un projet vivant, assez pour apprendre le modèle |
| **GitHub Projects** | Gratuit sur les dépôts publics, vues et automatisations comprises |
| **Shape Up** | Gratuit à lire. Sois en désaccord après l'avoir lu, pas avant |

## Personnes

| | |
|---|---|
| **progression.fyi** | Des dizaines de grilles de progression réelles côte à côte. La façon la plus rapide de voir la différence entre comportements et adjectifs |
| **re:Work sur l'entretien structuré** | Pourquoi les mêmes questions dans le même ordre battent le jugement d'un bon recruteur |
| Les archives de **Rands in Repose** | Vingt ans de management d'ingénieurs, par quelqu'un qui l'a mal fait et l'a dit |

Presque tous les *outils* RH sont facturés par siège et aucun n'est
nécessaire. Une grille dans un document et des notes de un-à-un dans un fichier
te mèneront plus loin.

## Communauté

| | |
|---|---|
| **Discourse** | Libre et auto-hébergeable ; l'offre hébergée ne l'est pas. Le modèle de modération vaut la lecture dans les deux cas |
| **Community Canvas** | Une fiche de travail pour « pour qui » et « qu'est-ce qui fait revenir » |

## Mentorat à l'échelle

| | |
|---|---|
| **Outreachy** | Deux sessions par an, rémunérées, et aucun employeur requis pour encadrer |
| **Google Summer of Code** | Annuel ; on encadre via un projet participant, donc trouve le projet d'abord |
| **Season of Docs** | Plus petit et moins disputé. Une première expérience plus simple parce que le livrable est sans ambiguïté |
| **Cohortes Skilluv** | Gratuites, ici, et démarrables cette semaine |

## Sur quoi dépenser

Rien, la première année. Si quelque chose finit par le mériter, ce sera l'outil
de planification contre lequel ton équipe se bat déjà — et à ce moment-là tu
sauras dire exactement ce qu'il doit faire.
$md$, 70);
