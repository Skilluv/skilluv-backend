-- The skills the education trades are made of.
--
-- ## What the catalogue held before this
--
-- Nothing. `mentoring-junior`, `technical-1on1` and `code-review-teaching`
-- exist under `soft_skills` and describe the one-to-one relationship, which
-- 0517 explained is a different trade. They stay where they are: moving them
-- would empty the mentoring vocabulary to fill this one, and the leadership
-- split will need them.
--
-- So this domain starts from an empty tree, which is unusual and worth
-- saying: every node below is new, and none of it is a rename of something
-- somebody already proved.
--
-- ## Naming
--
-- Each node names a technique or an artefact, never a level or a virtue.
-- "Patient" is not a skill anybody can be assessed on; "reading which of four
-- reasons a learner is stuck" is a thing a person has either learned to do or
-- not.
--
-- The temptation in this domain is to fill the tree with dispositions —
-- empathy, patience, presence. They matter and they are not skills the
-- platform can attest, so they are absent. What is here is what leaves a
-- trace: a lesson plan, a rubric, a measured outcome.
--
-- ## Where the tree deliberately stays shallow
--
-- Two levels, like the rest of the catalogue.

-- ═══════════════════════════════════════════════════════════════════
-- Roots
-- ═══════════════════════════════════════════════════════════════════

-- ## `ON CONFLICT (slug) DO NOTHING`, added when this branch met
-- `feat/leadership-quality-domains`
--
-- Both branches declared `cohort-facilitation`, and it is genuinely one
-- competence: keeping a group moving together when they are not moving at the
-- same speed. Migration 0466 gets there first, so the node lives in the
-- `leadership` domain and this domain reaches it through
-- `orientation_skill_map` — which is what that table is for, and what both
-- branches already argued about not duplicating nodes.
--
-- The clause is on every statement here rather than only the colliding one:
-- the next domain to arrive will overlap somewhere too, and a migration that
-- refuses to run because somebody else named a skill first is a merge
-- conflict discovered at deploy time.

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('teaching-delivery', 'Delivering to a room',
 'What happens between the plan and the learning: pace, attention, questions nobody prepared for.',
 'education'),
('learning-design', 'Learning design',
 'Deciding what is learned, in what order, and how anybody knows it worked.',
 'education'),
('assessment-craft', 'Assessment',
 'Finding out what somebody can actually do, and being able to defend the answer.',
 'education'),
('learner-support', 'Supporting a learner',
 'The part that decides whether somebody finishes: noticing, diagnosing, and intervening in time.',
 'education')
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Delivery
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'education', p.id
  FROM (VALUES
    ('lesson-pacing', 'Pacing a session',
     'Reading whether a room is ahead or behind, and changing what happens next rather than finishing the plan.'),
    ('live-coding-teaching', 'Teaching by writing code in front of people',
     'Typing slowly enough to be followed, making the mistakes on purpose, and narrating the decision rather than the syntax.'),
    ('question-handling', 'Handling the question you did not prepare for',
     'Answering, deferring or admitting you do not know — and knowing which of the three this one is.'),
    ('exercise-design', 'Designing an exercise',
     'A task that fails in an instructive way, is finishable in the time given, and cannot be completed by copying.'),
    ('cohort-facilitation', 'Running a cohort',
     'Twenty people over eight weeks: rhythm, group dynamics, and the ones who go quiet in week three.'),
    ('remote-teaching', 'Teaching people you cannot see',
     'Holding attention through a screen: check-ins, breakout structure, and knowing that silence means nothing online.'),
    ('demonstration-recovery', 'Recovering when the demonstration breaks',
     'The moment that teaches most, if the teacher debugs out loud instead of switching to slides.'),
    ('teaching-in-a-second-language', 'Teaching in a language that is not everybody''s first',
     'Vocabulary control, pace, and checking comprehension without asking whether everybody understood.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'teaching-delivery') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Learning design
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'education', p.id
  FROM (VALUES
    ('learning-objectives', 'Writing learning objectives',
     'What the learner will be able to do, stated as something observable. "Understands recursion" is not one.'),
    ('progression-design', 'Sequencing',
     'Ordering material so each step is reachable from the last. The single most common failure in a curriculum is a silent jump.'),
    ('prerequisite-mapping', 'Mapping prerequisites',
     'Making explicit what has to be true before a module starts, including the things experts forget they know.'),
    ('cognitive-load-management', 'Managing cognitive load',
     'Introducing one unfamiliar thing at a time. A tutorial that teaches a language, a framework and a tool at once teaches none.'),
    ('activity-variety', 'Varying the activity',
     'Reading, doing, explaining, breaking. A programme that is one mode from end to end loses most of the room.'),
    ('skill-matrix-authoring', 'Writing a skill matrix',
     'Describing what junior, mid and senior mean for a trade, in terms somebody could be assessed against.'),
    ('curriculum-maintenance', 'Keeping a curriculum alive',
     'Versioning it, dating what goes stale, and knowing which module breaks when a tool releases.'),
    ('accessibility-in-learning', 'Designing for the whole room',
     'Materials that work with a screen reader, on a bad connection, and for somebody who cannot attend live.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'learning-design') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Assessment
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'education', p.id
  FROM (VALUES
    ('rubric-authoring', 'Writing a rubric',
     'Criteria specific enough that two assessors reach the same grade, and public enough that a learner can aim at them.'),
    ('formative-assessment', 'Checking understanding as you go',
     'Finding out what is missing while there is still time to fix it, rather than at the end when there is not.'),
    ('project-based-assessment', 'Assessing a project',
     'Judging work that has no single right answer, against criteria written before it was submitted.'),
    ('feedback-that-lands', 'Feedback somebody can act on',
     'Specific, about the work, and paired with the next thing to try. A grade is not feedback.'),
    ('outcome-measurement', 'Measuring outcomes',
     'Before and after, on something observable. Satisfaction is a real signal and it is not learning.'),
    ('academic-integrity', 'Assessment somebody cannot fake',
     'Designing tasks where copying does not produce a pass, and handling it fairly when it happens anyway.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'assessment-craft') p
  ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Supporting a learner
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT v.slug, v.display_name, v.description, 'education', p.id
  FROM (VALUES
    ('diagnosing-a-stuck-learner', 'Diagnosing why somebody is stuck',
     'Missing prerequisite, misread instruction, environment broken, or afraid to ask. Four different problems that look identical.'),
    ('debugging-pedagogy', 'Teaching somebody to debug',
     'Handing over the method rather than the fix. The hardest thing to give away, and the thing that makes a learner independent.'),
    ('scaffolding-and-fading', 'Scaffolding, then removing it',
     'Support that is deliberately withdrawn. Help that never stops produces somebody who cannot work alone.'),
    ('learner-motivation', 'Keeping somebody in the room',
     'Week three of a cohort is where people leave. Knowing that, and building for it, is a skill.'),
    ('imposter-syndrome-handling', 'Working with somebody who thinks they do not belong',
     'Common enough among career changers to be a design constraint rather than an exception.'),
    ('teaching-adults', 'Teaching adults',
     'They arrive with experience, constraints and a reason. Ignoring any of the three loses them.')
  ) AS v(slug, display_name, description)
  CROSS JOIN (SELECT id FROM skill_nodes WHERE slug = 'learner-support') p
  ON CONFLICT (slug) DO NOTHING;
