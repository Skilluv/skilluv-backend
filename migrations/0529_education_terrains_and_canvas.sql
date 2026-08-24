-- Where education work can be done, and the ten cohorts the platform needs
-- for itself.
--
-- ## T-02: open educational projects — `terrain_proposals`
--
-- The table 0418 built. A proposal names an upstream repository, the labels
-- its ingestion should watch, and why it is worth somebody's first
-- contribution. A steward adopts it or declines it with a reason.
--
-- The five below have one property in common: their curriculum is in a public
-- repository and contributions to it are reviewed by people who teach. That
-- is rarer than it sounds — most educational organisations publish the
-- output and keep the programme.
--
-- ## T-03: `external_education_platforms` already exists and is called
-- `external_opportunities`
--
-- Migration 0513 wrote out the argument: a bootcamp hiring trainers and a
-- conference calling for papers are the same three facts — an organisation, a
-- deadline and a link — and two tables would mean two curation flows and two
-- answers to what is open. The `teaching_position` and `curriculum_call`
-- kinds are there for this ticket, and nothing is seeded into them for the
-- reason 0513 gave: a hiring page is true for about two months, and a closed
-- one looks exactly like an open one until somebody applies.
--
-- ## T-01: the platform teaches its own
--
-- Ten cohorts the community can lead. This is the terrain where the platform
-- is the client and the learners are its own newcomers, which makes it the
-- only place an educator can run a full cohort here without first finding
-- twenty people themselves.
--
-- They are seeded as curriculum challenges rather than as cohorts: a cohort
-- has dates and a teacher, and inventing both in a migration would create ten
-- programmes with nobody running them and a start date in the past. What is
-- seeded is the brief; a cohort is what an educator creates when they take
-- one on.

-- ═══════════════════════════════════════════════════════════════════
-- Projects whose curriculum is public
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO terrain_proposals
    (slug, name, skill_domain, kind, upstream_url, ingestion_labels, why_md, sort_order) VALUES

('freecodecamp-curriculum', 'freeCodeCamp — curriculum', 'education', 'oss_repo',
 'https://github.com/freeCodeCamp/freeCodeCamp',
 ARRAY['scope: curriculum', 'help wanted', 'good first issue'],
 'The largest open curriculum there is, and one of the few whose lesson content is reviewed in public by people who teach. A contributor sees their exercise reach hundreds of thousands of learners, which is a scale no other terrain here offers.',
 410),

('the-odin-project', 'The Odin Project — curriculum', 'education', 'oss_repo',
 'https://github.com/TheOdinProject/curriculum',
 ARRAY['Type: Content', 'Status: Needs Review', 'good first issue'],
 'A curriculum built entirely by contributors, with a review process that argues about pedagogy rather than about formatting. The best terrain for somebody learning to design a learning path: the reasoning happens in the pull request.',
 420),

('mdn-content', 'MDN Web Docs — learning area', 'education', 'oss_repo',
 'https://github.com/mdn/content',
 ARRAY['Content:Learn', 'good first issue'],
 'MDN''s learning area is a curriculum inside a reference, and it is the part that gets least attention. Contributions there reach every self-taught web developer who ever searched for how something works.',
 430),

('rust-by-example', 'Rust by Example', 'education', 'oss_repo',
 'https://github.com/rust-lang/rust-by-example',
 ARRAY['E-easy', 'C-enhancement'],
 'A whole book built on the principle this domain argues for — an example before the abstraction — and small enough that one contributor can improve one chapter properly.',
 440),

('exercism-tracks', 'Exercism — tracks and mentoring notes', 'education', 'oss_repo',
 'https://github.com/exercism/exercism',
 ARRAY['good first issue', 'help wanted'],
 'Exercises plus the notes mentors use when responding to them, which is a rare public record of teaching decisions. Contributing a mentoring note is contributing pedagogy, not content.',
 450);

-- ═══════════════════════════════════════════════════════════════════
-- Ten cohorts the platform needs
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, ai_policy, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## What there is to do' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## What is expected' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'This cohort runs on skill-uv.com, for its own newcomers, with your name ' ||
    'on it. The learners are members: their outcome records are theirs, they ' ||
    'consent to what leaves them, and nothing identifiable goes into what ' ||
    'you publish about it.' || E'\n\n' ||
    'There is evidence that somebody learned something — a before and after, ' ||
    'a finished project, a measured assessment.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid for the family applies, and it is public.',
    'education', c.difficulty, NULL,
    'draft', TRUE, c.ai_policy,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'education' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'education' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

('curriculum-designer', 'Skilluv — the first-contribution path',
 'Design the path a newcomer follows from arriving to a first accepted upstream contribution',
 'The path, its prerequisites, the artefacts at each step, and an honest estimate of how long it takes somebody with a job.', 4, 'disclosure_required'),

('curriculum-designer', 'Skilluv — Rust backend, an introduction',
 'Design an eight-week programme taking somebody who writes code to somebody who can open a pull request on this backend',
 'The programme, its assessment, and the facilitator notes another trainer would need.', 4, 'disclosure_required'),

('curriculum-designer', 'Skilluv — reading a codebase you did not write',
 'Design a short programme on the skill nobody teaches: finding your way around a hundred thousand lines somebody else wrote',
 'The programme, the exercises, and the codebases they run on.', 3, 'disclosure_required'),

('technical-trainer', 'Skilluv — the first-contribution cohort',
 'Run the first-contribution path as a cohort of ten to twenty newcomers, end to end',
 'The cohort delivered, its completion rate, the learners'' outcome records, and what you changed mid-way.', 5, 'disclosure_required'),

('technical-trainer', 'Skilluv — a workshop on the review grids',
 'Run a session teaching members to review each other''s work against a public grid',
 'The session, its exercises, and the reviews the participants produced afterwards.', 3, 'disclosure_required'),

('technical-trainer', 'Skilluv — a workshop on writing a contribution',
 'Three hours from a chosen issue to an opened pull request, on real repositories',
 'The run sheet, the prepared environments, and what blocked whom.', 3, 'disclosure_required'),

('coding-teacher', 'Skilluv — the beginner track, taught live',
 'Teach the beginner track to a group who have never programmed, over a term',
 'The programme as delivered, the learners'' outcome records, and the moments the group got stuck.', 5, 'disclosure_required'),

('coding-teacher', 'Skilluv — office hours, documented',
 'Run open office hours for a term and write up what people actually get stuck on',
 'The record of what was asked, grouped by cause, and what it says the curriculum is missing. Anonymised at source.', 3, 'disclosure_required'),

('curriculum-designer', 'Skilluv — the reviewer''s programme',
 'Design what somebody has to know before they are granted review rights in a domain',
 'The programme, the assessment, and the argument for where the bar sits.', 4, 'disclosure_required'),

('technical-trainer', 'Skilluv — a train-the-trainer session',
 'Teach the members who want to lead cohorts here how to run one',
 'The session, the materials, and the first cohort each participant ran afterwards.', 4, 'disclosure_required')

) AS c(orientation_slug, title, description, expected, difficulty, ai_policy)
JOIN orientations o ON o.slug = c.orientation_slug;
