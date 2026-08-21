-- The "worked in three trades" badge stops being a judgement.
--
-- Migration 0177 marked it manual, and said why: a deliverable pointed at a
-- challenge, a challenge carried a domain, and nothing carried a trade. That
-- was true then. Migration 0186 put the trade on the slice, so the badge is
-- now a count like the others.
--
-- Manual grants already made stay: somebody looked at the work and decided,
-- and the engine has no business taking that back because it can now count.
-- The rule simply starts awarding it to everyone else who qualifies.

UPDATE badge_rules
   SET conditions = '{"distinct_over": "orientation", "skill_domain": "code", "min_count": 3}',
       description = 'Du travail vérifié dans trois orientations code différentes.',
       updated_at = NOW()
 WHERE slug = 'code-multi-domain';
