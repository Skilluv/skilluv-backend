-- Ten pieces of communication the platform needs for itself.
--
-- Ticket T-03. Every other terrain in this domain belongs to somebody else —
-- an engine, a framework, a documentation site — and reaching a first
-- accepted contribution through one of them means waiting on a maintainer who
-- has a queue. This is the terrain where the platform is the client, the need
-- is real, and the work is read by everybody who opens the site.
--
-- ## Why the attribution matters more here than the challenge
--
-- The briefs below are ordinary. What makes this terrain worth having is the
-- second half of the ticket: the author's name appears, in clear, on the page
-- the work ships on. A platform that asks its community to write its
-- documentation and lists no names is asking for free work, and would deserve
-- the reading.
--
-- 0423 built `work_credits` over `attestations.evidence_url`. It answers
-- "who is credited here" for one basis only — `audio_project_credited` — and
-- this migration widens it to every basis that carries an evidence URL, so a
-- documentation page can ask the same question of the same record.
--
-- ## Why they are drafts, again
--
-- Same reason as 0417, 0423 and 0512: a brief nobody has reviewed must not be
-- offered to somebody learning. These need whoever owns each surface to say
-- what it should contain.

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
    'This work is meant to be published on skill-uv.com, with your name in ' ||
    'clear on the page it ships on. The credit is recorded as an attestation, ' ||
    'and it points at where it appears.' || E'\n\n' ||
    'In every case: every claim is verified, every example has been run, and ' ||
    'whatever comes from elsewhere is cited.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid for the family applies, and it is public.',
    'communication', c.difficulty, NULL,
    'draft', TRUE, c.ai_policy,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'communication' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'communication' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

('tech-writer', 'Skilluv — contributor getting-started guide',
 'Write the page a new contributor reads first: install, run the tests, open a first contribution',
 'The page, replayed on a clean machine, and the list of everything that blocked during that attempt.', 3, 'disclosure_required'),

('tech-writer', 'Skilluv — public API reference',
 'Document the public API endpoints: parameters, returns, errors, rate limits',
 'The reference entries, each with a runnable example call and its real response.', 4, 'disclosure_required'),

('tech-writer', 'Skilluv — explaining the proof model',
 'Explain what an attestation is, what it proves and what it does not',
 'Three linked explanation pages, readable by somebody who is not a developer.', 3, 'disclosure_required'),

('tech-writer', 'Skilluv — release notes people can read',
 'Rework the release notes so they say what changes for a user rather than what was merged',
 'Two past releases rewritten, plus the format to follow for the ones after.', 2, 'disclosure_required'),

('developer-advocate', 'Skilluv — a talk introducing the platform',
 'Prepare and give a twenty-minute talk explaining what Skilluv is trying to do, without a sales deck',
 'The recording, the slides, and the five-minute version.', 4, 'disclosure_required'),

('developer-advocate', 'Skilluv — "my first contribution" workshop',
 'Run a workshop where beginners open their first upstream contribution, end to end',
 'The run sheet, the prepared environment, and a write-up of what blocked the participants.', 4, 'disclosure_required'),

('content-creator-tech', 'Skilluv — video series on the contributor path',
 'Three videos following a real path: arriving, choosing a challenge, getting a deliverable validated',
 'The three videos, their captions, and the thread that connects them.', 4, 'disclosure_required'),

('content-creator-tech', 'Skilluv — podcast episode with a member',
 'Interview a member about what they built and what the platform did or did not change for them',
 'The episode, its transcript, and show notes linking the artefacts discussed.', 3, 'disclosure_required'),

('technical-translator', 'Skilluv — interface in Portuguese and Arabic',
 'Carry the interface into two languages that open the platform to whole communities',
 'The translated files, the glossary, and the handling of reading direction for Arabic.', 4, 'disclosure_required'),

('research-writer-tech', 'Skilluv — the state of proving technical skill',
 'Write the state of what exists for proving a technical skill, and of what does not work',
 'The document, its sources, and the declaration of interest: it is commissioned by a platform selling an answer to this problem.', 5, 'human_verified')

) AS c(orientation_slug, title, description, expected, difficulty, ai_policy)
JOIN orientations o ON o.slug = c.orientation_slug;

-- ═══════════════════════════════════════════════════════════════════
-- Credits stop being an audio feature
-- ═══════════════════════════════════════════════════════════════════
--
-- `work_credits` (0423) answers "who is credited on this project" and reads
-- one basis: `audio_project_credited`. The question is not an audio question.
-- A documentation page, a translated interface and a tutorial video all
-- carry a name, and all three record it the same way — an attestation whose
-- `evidence_url` points at where the credit appears.
--
-- The view is widened rather than copied. A second view for communication
-- would mean a page has to know which one to read, and the answer would be
-- the domain — which is exactly the thing a credits list should not have to
-- care about.
--
-- The `audio_subtype` column is kept so nothing reading the view breaks, and
-- joined by a generic `subtype` that carries whichever the artefact has.

DROP VIEW work_credits;

CREATE VIEW work_credits AS
SELECT p.id            AS project_id,
       p.slug          AS project_slug,
       u.id            AS user_id,
       u.username,
       u.display_name,
       a.basis,
       a.title         AS credit_title,
       a.evidence_url,
       a.verification_code,
       a.issued_at,
       ps.audio_subtype,
       -- Whichever subtype the artefact carries, so a credits page prints
       -- "documentation" or "voice_reel" without knowing the domain.
       COALESCE(ps.audio_subtype, ps.communication_subtype,
                ps.design_subtype, ps.code_subtype,
                ps.ai_subtype, ps.ops_subtype) AS subtype,
       ps.primary_domain
  FROM attestations a
  JOIN users u ON u.id = a.user_id
  JOIN deliverables d ON d.id = ANY (a.linked_deliverable_ids)
  JOIN project_slices ps ON ps.id = d.slice_id
  JOIN projects p ON p.id = ps.project_id
 WHERE a.evidence_url IS NOT NULL
   AND a.revoked_at IS NULL
   AND d.revoked_at IS NULL;

COMMENT ON VIEW work_credits IS
    'Who is credited on which project, from the attestations that carry an '
    'evidence URL. A view rather than a table: a second copy of these facts '
    'is one somebody has to remember to update when a credit is revoked, and '
    'a retracted credit has to leave the page it was printed on. Widened from '
    'the audio-only version of 0423 — a documentation page carries a name too.';
