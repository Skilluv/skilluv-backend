-- What each education trade is actually made of.
--
-- ## Core and recommended
--
-- Core is what the trade cannot exist without: remove it and the person is
-- doing something else. Three to five per orientation. A trade where
-- everything is core says nothing about what to learn first, which is the
-- only thing this map is read for.
--
-- ## Where the trainer and the teacher differ
--
-- They share a review family and most of a skill set, and the map is where
-- the difference is written down rather than argued about:
--
--   * the trainer's core is *delivery under time pressure* — a workshop is
--     three hours and does not come back;
--   * the teacher's core is *diagnosis* — the same twenty people for a
--     semester, and the whole job is noticing which one has stopped
--     following and why.
--
-- ## Why all three point outside the domain
--
-- Teaching is teaching something. A trainer who cannot do the thing they
-- teach is running a slide deck, and the rows reaching into `communication`
-- and `soft_skills` say the rest: a lesson is a written artefact before it is
-- a performance, and feedback is a conversation somebody can get wrong.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended, weight)
SELECT o.id, s.id, v.is_core, TRUE, v.weight
  FROM (VALUES

-- ── technical-trainer ──────────────────────────────────────────────
('technical-trainer', 'teaching-delivery',            TRUE,  1.0),
('technical-trainer', 'lesson-pacing',                TRUE,  1.0),
('technical-trainer', 'exercise-design',              TRUE,  1.0),
('technical-trainer', 'cohort-facilitation',          TRUE,  1.0),
('technical-trainer', 'question-handling',            TRUE,  1.0),
('technical-trainer', 'live-coding-teaching',         FALSE, 0.9),
('technical-trainer', 'demonstration-recovery',       FALSE, 0.8),
('technical-trainer', 'remote-teaching',              FALSE, 0.8),
('technical-trainer', 'learning-objectives',          FALSE, 0.9),
('technical-trainer', 'outcome-measurement',          FALSE, 0.9),
('technical-trainer', 'feedback-that-lands',          FALSE, 0.8),
('technical-trainer', 'teaching-adults',              FALSE, 0.9),
('technical-trainer', 'teaching-in-a-second-language', FALSE, 0.7),
-- Outside the domain: a workshop is a written artefact before it is a
-- performance.
('technical-trainer', 'talk-structure',               FALSE, 0.8),
('technical-trainer', 'slide-craft',                  FALSE, 0.7),

-- ── coding-teacher ─────────────────────────────────────────────────
('coding-teacher', 'teaching-delivery',               TRUE,  1.0),
('coding-teacher', 'learner-support',                 TRUE,  1.0),
('coding-teacher', 'diagnosing-a-stuck-learner',      TRUE,  1.0),
('coding-teacher', 'debugging-pedagogy',              TRUE,  1.0),
('coding-teacher', 'live-coding-teaching',            TRUE,  1.0),
('coding-teacher', 'scaffolding-and-fading',          FALSE, 0.9),
('coding-teacher', 'exercise-design',                 FALSE, 0.9),
('coding-teacher', 'feedback-that-lands',             FALSE, 0.9),
('coding-teacher', 'learner-motivation',              FALSE, 0.8),
('coding-teacher', 'imposter-syndrome-handling',      FALSE, 0.8),
('coding-teacher', 'formative-assessment',            FALSE, 0.8),
('coding-teacher', 'question-handling',               FALSE, 0.8),
('coding-teacher', 'cognitive-load-management',       FALSE, 0.9),
('coding-teacher', 'teaching-adults',                 FALSE, 0.7),
-- Outside the domain: reviewing a beginner's code is the same gesture as
-- reviewing a colleague's, done more slowly.
('coding-teacher', 'code-review-teaching',            FALSE, 0.8),
('coding-teacher', 'giving-feedback',                 FALSE, 0.7),

-- ── curriculum-designer ────────────────────────────────────────────
('curriculum-designer', 'learning-design',            TRUE,  1.0),
('curriculum-designer', 'learning-objectives',        TRUE,  1.0),
('curriculum-designer', 'progression-design',         TRUE,  1.0),
('curriculum-designer', 'prerequisite-mapping',       TRUE,  1.0),
('curriculum-designer', 'assessment-craft',           TRUE,  1.0),
('curriculum-designer', 'rubric-authoring',           FALSE, 0.9),
('curriculum-designer', 'cognitive-load-management',  FALSE, 0.9),
('curriculum-designer', 'activity-variety',           FALSE, 0.8),
('curriculum-designer', 'skill-matrix-authoring',     FALSE, 0.8),
('curriculum-designer', 'curriculum-maintenance',     FALSE, 0.8),
('curriculum-designer', 'accessibility-in-learning',  FALSE, 0.8),
('curriculum-designer', 'project-based-assessment',   FALSE, 0.8),
('curriculum-designer', 'outcome-measurement',        FALSE, 0.9),
('curriculum-designer', 'academic-integrity',         FALSE, 0.7),
-- Outside the domain: a curriculum is a document, and it is read by people
-- who were not in the room when it was decided.
('curriculum-designer', 'docs-information-architecture', FALSE, 0.8),
('curriculum-designer', 'written-communication',      FALSE, 0.8)

  ) AS v(orientation_slug, skill_slug, is_core, weight)
  JOIN orientations o ON o.slug = v.orientation_slug
  JOIN skill_nodes  s ON s.slug = v.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;

-- Every slug above has to exist. The JOIN would otherwise drop an unknown one
-- silently, and a skill map short of three edges reads exactly like a skill
-- map that was written that way.
DO $$
DECLARE
    expected INT := 47;
    actual   INT;
BEGIN
    SELECT count(*) INTO actual
      FROM orientation_skill_map m
      JOIN orientations o ON o.id = m.orientation_id
     WHERE o.primary_domain = 'education';

    IF actual <> expected THEN
        RAISE EXCEPTION
            'education skill map: % edges written, % expected — a skill slug above does not exist',
            actual, expected;
    END IF;
END $$;
