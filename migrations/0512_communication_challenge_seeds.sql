-- Twenty challenges, one set per communication trade.
--
-- ## Why they are drafts
--
-- Same reason as 0185, 0219 and 0417: the title and the intent come from the
-- backlog, and the full brief — the target project, the exact deliverable,
-- what is out of scope — needs an author who knows the trade. A challenge
-- nobody has reviewed must not be offered to somebody learning, and `draft`
-- is the state the workflow already has.
--
-- Seeding them anyway is the point. Five trades with an empty catalogue are
-- five trades the platform claims to support and cannot.
--
-- ## Why the instructions are built rather than written out
--
-- 0185 wrote a hundred and thirty-eight briefs by hand and every one repeats
-- the same headings. The variable part — what to do, and what comes out — is
-- what the rows carry.
--
-- ## The paragraph every communication brief ends on
--
-- Two things, and they are the two reasons a submission in this domain comes
-- back: an unverified claim, and an unattributed borrowing. Both are things
-- the author can check before a reviewer has to, and both are in the common
-- review grid of 0503.
--
-- ## `ai_policy`
--
-- `disclosure_required` on seventeen of the twenty — the platform default,
-- meaning a generative tool is allowed and has to be declared.
--
-- `human_verified` on the three research briefs, and for a reason specific to
-- them: a fabricated citation is the classic failure of a language model, it
-- is invisible to a reader who trusts the document, and the whole value of a
-- research text is that its sources can be followed. Elsewhere in this domain
-- a wrong sentence is a wrong sentence; here it is the artefact pretending to
-- be something it is not.
--
-- This is the second place in the catalogue where the stricter policy
-- protects the reader rather than the platform — 0417 wrote the first, for
-- voice.

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
    'In every case: every claim is verified and every example has been run — ' ||
    'this is the domain where a confident error gets copied and the author ' ||
    'never finds out. And whatever comes from elsewhere is cited with a ' ||
    'reachable link: text, screenshot, data, code excerpt.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid for the family applies, and it is public: you can read ' ||
    'it before you submit.',
    'communication', c.difficulty, NULL,
    'draft', TRUE, c.ai_policy,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'communication' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'communication' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── tech-writer (5) ────────────────────────────────────────────────
('tech-writer', 'Documentation contribution accepted upstream',
 'Get a documentation change accepted by an open-source project you do not control',
 'The link to the merged contribution, and three lines saying what was missing and for whom.', 2, 'disclosure_required'),

('tech-writer', 'Complete tutorial for one feature',
 'Write a step-by-step tutorial that carries a beginner from the first prerequisite to a result they can see',
 'The published tutorial, the list of prerequisites, and proof it was replayed end to end on a clean machine.', 3, 'disclosure_required'),

('tech-writer', 'Missing API reference',
 'Document a part of a public API that has nothing but its signatures: parameters, returns, errors, edge cases',
 'The reference contributed upstream, with at least one runnable example per documented entry.', 3, 'disclosure_required'),

('tech-writer', 'Changelog and migration guide',
 'Write the changelog for a release that breaks something, plus the guide that says how to get from one version to the other',
 'Both texts, the list of breaks with who they affect, and the migration path for each.', 4, 'disclosure_required'),

('tech-writer', 'README rebuild for a community project',
 'Rework a community project README so a stranger understands in a minute what it is and how to start',
 'The contribution proposed upstream, with before and after, and the reason for every cut.', 2, 'disclosure_required'),

-- ── developer-advocate (4) ─────────────────────────────────────────
('developer-advocate', 'A twenty-minute talk delivered',
 'Propose, get accepted and deliver a twenty to thirty minute talk at a conference or a meetup',
 'The recording, the slides, and the proposal exactly as it was sent to the committee.', 4, 'disclosure_required'),

('developer-advocate', 'Live demonstration',
 'Run something in front of an audience, live, with a prepared fallback path',
 'The recording, the demo repository, and the written plan B — what was going to happen if the connection died.', 3, 'disclosure_required'),

('developer-advocate', 'Technical deep dive article',
 'Write an article that goes to the end of a subject rather than over it: five thousand words, with code that runs',
 'The published article, the repository that goes with it, and the sources cited.', 4, 'disclosure_required'),

('developer-advocate', 'A meetup organised end to end',
 'Organise a meetup — venue, programme, speakers, announcement — and speak at it',
 'The announcement, the programme as it actually ran, the recordings or slides, and a write-up of what worked and what did not.', 4, 'disclosure_required'),

-- ── content-creator-tech (4) ───────────────────────────────────────
('content-creator-tech', 'Fifteen-minute video tutorial',
 'Write, shoot and edit a fifteen-minute technical tutorial somebody can follow without pausing',
 'The published video, the script, the captions, and the repository of the code shown.', 3, 'disclosure_required'),

('content-creator-tech', 'Technical podcast episode',
 'Prepare and publish a thirty-minute episode, interview or solo, on a subject that holds for that long',
 'The published episode, its show notes with every link cited, and the transcript.', 3, 'disclosure_required'),

('content-creator-tech', 'Four-hour development stream',
 'Hold a four-hour stream on a public project, taking chat questions as they come',
 'The retained recording, the project repository, and a summary of what actually moved forward.', 3, 'disclosure_required'),

('content-creator-tech', 'A coherent series of three videos',
 'Three videos that follow one another on one subject, where the third assumes the first two and says so',
 'The three videos, the thread that connects them written down, and the captions.', 4, 'disclosure_required'),

-- ── technical-translator (4) ───────────────────────────────────────
('technical-translator', 'Translating a section of open-source documentation',
 'Translate a whole section of an open-source project''s documentation, and get it accepted upstream',
 'The merged contribution, the glossary used, and the source version that was translated.', 3, 'disclosure_required'),

('technical-translator', 'Translating the Skilluv interface',
 'Carry the interface into a language it is missing: Portuguese, Arabic, Swahili, Wolof, Lingala',
 'The translated files, the glossary, and the list of what was left in English with the reason.', 3, 'disclosure_required'),

('technical-translator', 'A coherent technical glossary',
 'Build a technical glossary in a target language, usable across several projects, with the contested calls written down',
 'The published glossary, the justification for every debatable choice, and two translated texts that follow it.', 4, 'disclosure_required'),

('technical-translator', 'Improving a project''s i18n pipeline',
 'Make a project genuinely translatable: string extraction, plurals, no concatenated sentences',
 'The contribution proposed upstream, and a demonstration that a second language now lands without touching the code.', 4, 'disclosure_required'),

-- ── research-writer-tech (3) ───────────────────────────────────────
('research-writer-tech', 'Whitepaper on a technical subject',
 'Write a fifteen to twenty-five page whitepaper: one question, one method, results and their limits',
 'The document, the data behind it, and a protocol precise enough for a stranger to replay.', 5, 'human_verified'),

('research-writer-tech', 'State of a technical sector',
 'Produce an annual state of a sector: what exists, what is moving, and what is announced without evidence',
 'The report, its sources, and a declaration of the author''s interests with the actors named in it.', 5, 'human_verified'),

('research-writer-tech', 'An external specification proposed',
 'Draft a specification for a community or a standards body: constrained vocabulary, edge cases, compatibility',
 'The document, the trace of its submission, and the feedback received with what was done about it.', 5, 'human_verified')

) AS c(orientation_slug, title, description, expected, difficulty, ai_policy)
JOIN orientations o ON o.slug = c.orientation_slug;

-- Twenty rows, or a slug above is wrong and the JOIN dropped one silently.
DO $$
DECLARE
    seeded INT;
BEGIN
    SELECT count(*) INTO seeded
      FROM challenge_templates
     WHERE skill_domain = 'communication';

    IF seeded <> 20 THEN
        RAISE EXCEPTION
            'communication challenge seeds: % rows written, 20 expected', seeded;
    END IF;
END $$;
