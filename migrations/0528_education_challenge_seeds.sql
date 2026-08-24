-- Thirteen challenges, one set per education trade.
--
-- ## Why they are drafts
--
-- Same reason as 0185, 0219, 0417 and 0512: the title and the intent come
-- from the backlog, and the full brief — the audience, the exact deliverable,
-- what is out of scope — needs an author who knows the trade. A challenge
-- nobody has reviewed must not be offered to somebody learning, and `draft`
-- is the state the workflow already has.
--
-- ## The paragraph every education brief ends on
--
-- Two things, and they are the two reasons a submission in this domain comes
-- back: a delivery with no evidence that anybody learned, and a delivery that
-- exposes a learner. The second is the one people do not expect — a cohort
-- report with twenty names in it is refused however good the teaching was —
-- so it is in every brief rather than in a policy page nobody opens.
--
-- ## Where the backlog's list changed
--
-- Two of its briefs asked for something the platform cannot accept as
-- written:
--
--   * **"Semester teaching bootcamp — proof of employment + outcomes"**
--     became a term of teaching with *the learners' own outcome records*.
--     Proof of employment is a fact about a contract, this platform does not
--     verify employment, and asking for it would have collected payslips.
--   * **"Struggling student turnaround — case study"** is kept, and the brief
--     says the case study is anonymised at source. It is the best work in
--     this domain and the one most likely to arrive with a real person's
--     difficulties in it under their name.

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
    'In every case: there is evidence that somebody learned something — a ' ||
    'before and after, a finished project, a measured assessment. Enjoyment ' ||
    'is a real signal and it is not that evidence.' || E'\n\n' ||
    'And nothing identifiable about a learner appears in what you submit: no ' ||
    'names, no faces, no marks against a person, no messages. Anonymise at ' ||
    'source, or carry written consent. A delivery that exposes a learner is ' ||
    'refused whatever else it does.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid for the family applies, and it is public: you can read ' ||
    'it before you submit.',
    'education', c.difficulty, NULL,
    'draft', TRUE, c.ai_policy,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'education' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'education' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── technical-trainer (5) ──────────────────────────────────────────
('technical-trainer', 'A three-hour workshop, delivered',
 'Design and run a three-hour hands-on workshop where participants spend more time working than watching',
 'The recording or the run sheet, the slides, the exercises with their solutions, the environment setup, and what participants could do at the end that they could not at the start.', 3, 'disclosure_required'),

('technical-trainer', 'An eight-week cohort, end to end',
 'Run a cohort of eight weeks: sessions, follow-up, assessment, and the ones who go quiet in week three',
 'The programme as it was actually delivered, the completion rate, the measured outcomes, and a note on what you changed mid-way and why.', 5, 'disclosure_required'),

('technical-trainer', 'Two days of training inside an organisation',
 'Deliver a two-day training on a team''s own tooling and constraints, and evaluate it afterwards',
 'The materials, the adaptation you made to their context, and the post-training evaluation with what it changed.', 4, 'disclosure_required'),

('technical-trainer', 'A meetup with a hands-on half',
 'Speak at a meetup where the second half is the room doing the thing rather than watching it',
 'The talk, the exercise, the environment that worked on everybody''s machine, and what went wrong for whom.', 3, 'disclosure_required'),

('technical-trainer', 'A self-paced course in five modules',
 'Produce a recorded course of five modules that somebody can complete alone, with exercises that check they did',
 'The five modules, their exercises and solutions, and the point at which a learner alone gets stuck — you will only find it by watching one try.', 4, 'disclosure_required'),

-- ── curriculum-designer (4) ────────────────────────────────────────
('curriculum-designer', 'A twelve-week bootcamp curriculum',
 'Design a complete twelve-week programme: objectives, sequencing, projects, assessment',
 'The curriculum, its prerequisite map, its assessment plan, and the facilitator notes a second trainer would need to run it.', 5, 'disclosure_required'),

('curriculum-designer', 'A learning path from junior to senior',
 'Design a path through one trade: what is learned, in what order, and what proves each step',
 'The path, the skills at each stage, the artefacts that demonstrate them, and the honest estimate of how long it takes.', 4, 'disclosure_required'),

('curriculum-designer', 'A skill matrix for a technical team',
 'Write what junior, mid and senior mean for one trade, in terms somebody could be assessed against',
 'The matrix, the evidence expected at each level, and the review process that keeps it from becoming a promotion checklist.', 4, 'disclosure_required'),

('curriculum-designer', 'An assessment framework',
 'Build rubrics and project briefs that measure what a programme claims to teach',
 'The rubrics, a worked example of two assessors reaching the same grade with them, and the appeal process.', 4, 'disclosure_required'),

-- ── coding-teacher (4) ─────────────────────────────────────────────
('coding-teacher', 'Ten lesson plans for absolute beginners',
 'Write ten lesson plans covering the first steps of programming, in an order where nothing is assumed before it is taught',
 'The ten plans, their exercises, the misconceptions each one is designed to catch, and the prerequisite map.', 3, 'disclosure_required'),

('coding-teacher', 'A term of teaching, with its outcomes',
 'Teach a term in a school, a bootcamp or a community programme, and account for what changed for the learners',
 'The programme as delivered, the learners'' own outcome records, the completion rate, and what you would change. Anonymised at source.', 5, 'disclosure_required'),

('coding-teacher', 'A turnaround, written up',
 'Document one learner who was going to give up and did not: what was actually wrong, what you tried, what worked',
 'The case study, anonymised at source, with the intervention described precisely enough for another teacher to try it.', 4, 'disclosure_required'),

('coding-teacher', 'A series on how people learn to program',
 'Write about the pedagogy rather than the language: the misconceptions, the patterns, the moments people get stuck',
 'The series, with the claims sourced — either to research or to your own records — and the limits of what you can conclude from one classroom.', 4, 'disclosure_required')

) AS c(orientation_slug, title, description, expected, difficulty, ai_policy)
JOIN orientations o ON o.slug = c.orientation_slug;

-- Thirteen rows, or a slug above is wrong and the JOIN dropped one silently.
DO $$
DECLARE
    seeded INT;
BEGIN
    SELECT count(*) INTO seeded
      FROM challenge_templates
     WHERE skill_domain = 'education';

    IF seeded <> 13 THEN
        RAISE EXCEPTION
            'education challenge seeds: % rows written, 13 expected', seeded;
    END IF;
END $$;
