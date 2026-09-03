-- One published onboarding challenge per domain, and a column that says which
-- one it is.
--
-- ## The hole
--
-- `GET /api/challenges/onboarding?domain=X` runs
--
--     SELECT * FROM challenge_templates
--     WHERE is_onboarding AND skill_domain = $1 AND status = 'published'
--
-- and `/auth/register` sends every new account straight at it. Four domains
-- answered — `code`, `design`, `game` and `security`, from the four rows
-- migration 0003 seeded in 2024 — and eight did not: `ops`, `ai`,
-- `soft_skills`, `audio`, `quality`, `leadership`, `communication` and
-- `education` each returned "No onboarding challenge found for domain". The
-- first screen after signing up was an error for two thirds of the platform.
--
-- SKI-360 reads this as ten domains of eleven. It is eight of twelve: the four
-- 2024 seeds do answer, and `soft_skills` is an active row of `skill_domains`
-- and a value `validate_skill_domain` accepts, so somebody can register into
-- it and be sent to the same 404. Seeding eleven and excluding `soft_skills`
-- by hand would have left that one open, and would have put a hand-written
-- list back exactly where the ticket asks for a loop over the constant.
-- Twelve rites, no exclusion list.
--
-- ## Why the four 2024 seeds are retired rather than kept
--
-- They ask, in French, for "minimum 100 mots", which is not a coincidence:
-- `evaluate_basic` passed any non-code submission of 100 characters, and those
-- four briefs were written against that scoring. SKI-361 removes the scoring;
-- keeping a brief that asks for a word count nothing measures any more would
-- leave the platform teaching people to pad.
--
-- Archived rather than deleted: `challenge_submissions` and `deliverables`
-- reference them, and somebody who did the 2024 Hello World keeps their proof.
-- `is_onboarding = FALSE` plus `status = 'archived'` is what removes them from
-- every surface — the onboarding lookup, and the public catalogue, which
-- already filters on `is_onboarding = FALSE`.
--
-- ## Why a column and not just a row
--
-- `is_onboarding` marks fifteen rows in the `code` domain alone: the fifteen
-- "Bonjour Skilluv — <starter>" variants of the fork gesture, one per starter
-- template. They belong there — the flag is also what keeps them out of the
-- public listing — but it means `LIMIT 1` picks an arbitrary one of fifteen,
-- and a rite whose brief changes between two page loads is not a rite.
--
-- `is_domain_rite` names *the* first gesture of a domain, one per domain,
-- enforced by a partial unique index. The starter variants stay
-- `is_onboarding` and stop being the answer to "what does a new designer do
-- first".
--
-- ## The gestures
--
-- One per domain, each producing a real artifact on day one, in the shape of
-- the trade, landing in the review loop that domain already has. The table is
-- SKI-362's. The two rows that ticket does not carry are `code`, whose gesture
-- was already built, and `soft_skills`, a trade practised on other people's
-- work and whose first gesture is therefore a review.
--
-- Every one of them is read by a person. After SKI-361 none of them can be
-- anything else: a non-code submission lands in the human review queue.

-- ═══════════════════════════════════════════════════════════════════
-- 1. The column that names a domain's rite
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE challenge_templates
    ADD COLUMN is_domain_rite BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN challenge_templates.is_domain_rite IS
    'TRUE on the single template that is this domain''s Bonjour Skilluv rite — '
    'the first gesture a new account is asked for. At most one published row '
    'per domain carries it (challenge_templates_one_rite_per_domain). '
    '`is_onboarding` is broader: it also marks the fifteen per-starter variants '
    'of the code fork gesture, which is why the onboarding lookup needs this '
    'column to be deterministic.';

CREATE UNIQUE INDEX challenge_templates_one_rite_per_domain
    ON challenge_templates (skill_domain)
    WHERE is_domain_rite AND status = 'published';

-- ═══════════════════════════════════════════════════════════════════
-- 2. Retire the 2024 seeds
-- ═══════════════════════════════════════════════════════════════════

UPDATE challenge_templates
SET is_onboarding = FALSE,
    status = 'archived'
WHERE title IN (
    'Premier pas — Hello World',
    'Premier pas — Ton premier design',
    'Premier pas — Game Concept',
    'Premier pas — Trouve la faille'
);

-- The two starters written for game developers were filed under `code` with
-- the other thirteen, so the domain they were made for had none of them. The
-- seed file is corrected in the same commit; this is for databases that have
-- already run it.
UPDATE challenge_templates
SET skill_domain = 'game'
WHERE title IN (
    'Bonjour Skilluv — Game Godot',
    'Bonjour Skilluv — Game Bevy'
)
AND skill_domain = 'code';

-- ═══════════════════════════════════════════════════════════════════
-- 3. The twelve rites
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO challenge_templates (
    title, description, instructions,
    skill_domain, difficulty, mode, tone, duration_minutes,
    reward_fragments, is_onboarding, is_training, is_domain_rite,
    is_capstone, status, ai_policy, evaluation_rubric
)
SELECT
    v.title, v.description, v.instructions,
    v.skill_domain, 1, 'solo', 'educational', 60,
    10, TRUE, TRUE, TRUE,
    FALSE, 'published', 'disclosure_required',
    -- The domain's own default review grid, the way migration 0417 attaches
    -- one to every audio seed. A rite is read by a person, and a person
    -- reading with no statement of what good means is how a first gesture
    -- turns into a matter of taste. NULL where a domain has published no
    -- default grid yet — `game`, `security` (whose grids are all
    -- group-scoped) and `soft_skills` — and that is a gap in those grids, not
    -- something for this migration to invent.
    (SELECT g.criteria FROM review_grids g
      WHERE g.domain = v.skill_domain AND g.reviewer_group IS NULL
      LIMIT 1)
FROM (VALUES
    (
        'code',
        'Bonjour Skilluv — the first commit',
        'Fork a Skilluv starter, introduce yourself in HELLO.md, and open the pull request.',
        E'The gesture: a pull request on a repository that is yours.\n\n1. Start the rite — the platform forks a `skilluv-community/starter-*` onto your GitHub account.\n2. Clone it and edit `HELLO.md`: who you are, what you want to build, what you already know.\n3. Commit, push, and open a pull request from `main` to `showcase` on your own fork.\n\nWhat is read: the pull request itself. Not its length — whether somebody arriving on your fork can tell what you are here to do.'
    ),
    (
        'design',
        'Bonjour Skilluv — the first screen',
        'One screen against a short brief, uploaded, and read by three reviewers.',
        E'The gesture: one screen, finished enough to be argued with.\n\n1. Take the entry brief of your trade. It is short on purpose, and it does not say what the screen looks like.\n2. Design one screen against it. One. A flow of six half-screens is not this rite.\n3. Upload it, with the two or three sentences that say what decision each choice serves.\n\nWhat is read: the fit between the brief and the screen. A beautiful screen answering a different brief does not pass, and that is the whole lesson.'
    ),
    (
        'game',
        'Bonjour Skilluv — the first playtest',
        'Play a slice of a Skilluv game and return a playtest verdict somebody can act on.',
        E'The gesture: a verdict, not an opinion.\n\n1. Play one published slice, start to finish, at least once.\n2. Write the verdict: what the slice taught you without telling you, where you got stuck and for how long, and the one change you would make first.\n3. Say what you did not test, so the next reader knows the edges of your report.\n\nWhat is read: whether the author of the slice can do something with your verdict tomorrow morning.'
    ),
    (
        'security',
        'Bonjour Skilluv — the first finding',
        'Read the public scope, test inside it, and report one finding on the Skilluv canvas.',
        E'The gesture: one finding, inside the scope, written to be reproduced.\n\n1. Read the published scope of the Skilluv disclosure programme. It says what is in, and more importantly what is not.\n2. Test only what the scope names. A finding against anything else is refused however real it is — that rule is the trade.\n3. Report it: what you did, what happened, why it matters, and what you would change.\n\nWhat is read: reproducibility. A finding somebody cannot reproduce from your text is not yet a finding.'
    ),
    (
        'ops',
        'Bonjour Skilluv — the first SLO read',
        'Read one service level objective of the Skilluv ops ground and propose one improvement.',
        E'The gesture: reading production before touching it.\n\n1. Open the ops ground and pick one SLO. Read what it promises, what it measures, and what its error budget is.\n2. Say what it does not catch. Every SLO misses something; naming it is the skill.\n3. Propose one change — to the objective, to the measurement, or to what happens when the budget burns — and say what it costs.\n\nWhat is read: whether the proposal survives its own trade-off. "Add more alerts" is not a proposal.'
    ),
    (
        'quality',
        'Bonjour Skilluv — the first defect report',
        'File one defect report on the Skilluv canvas that an engineer can reproduce without asking you anything.',
        E'The gesture: a report that needs no follow-up question.\n\n1. Use the Skilluv canvas as a real user would, and find one thing that is wrong.\n2. Write it down: what you did, step by step; what you expected; what happened instead; where, and on what.\n3. Say how sure you are, and what would prove you wrong.\n\nWhat is read: whether somebody who has never seen your screen can reproduce it from your text alone.'
    ),
    (
        'ai',
        'Bonjour Skilluv — the first workspace step',
        'Take one step of an entry mission in the workspace, and show what you checked.',
        E'The gesture: one step, and the evidence behind it.\n\n1. Open an entry mission and take its first workspace step.\n2. Do the work — and record what you verified: what you ran, what came back, what you rejected and why.\n3. State what you are not sure of. A step that claims certainty it does not have is the failure mode of this trade.\n\nWhat is read: the checking, not the output. An output nobody verified is not proof of anything.'
    ),
    (
        'audio',
        'Bonjour Skilluv — the first signature',
        'Twenty seconds of sound, every source declared.',
        E'The gesture: twenty seconds you can account for entirely.\n\n1. Make a twenty-second signature — an identity, a sting, a texture. Short on purpose: twenty seconds hides nothing.\n2. Declare every source: what you recorded, what you synthesised, what you sampled and under what licence.\n3. Say what you would fix with another hour.\n\nWhat is read: the sound, and the honesty of the source list. An undeclared sample ends the rite, whatever it sounds like.'
    ),
    (
        'communication',
        'Bonjour Skilluv — the first translation',
        'Translate one paragraph of a Skilluv guide, and defend the choices that are not literal.',
        E'The gesture: one paragraph, carried across whole.\n\n1. Pick a paragraph of a published guide, in a language pair you actually work in.\n2. Translate it so that a reader in the target language gets what the original reader gets — not the same words, the same understanding.\n3. Note the two or three places where you did not translate literally, and say why.\n\nWhat is read: the notes as much as the text. A translation whose author cannot explain their departures was not a translation.'
    ),
    (
        'education',
        'Bonjour Skilluv — the first explanation',
        'Explain one skill node in three beats, to somebody who does not have it yet.',
        E'The gesture: three beats, in order, for a real beginner.\n\n1. Pick one skill node from the tree — one you hold, and one you remember not holding.\n2. Explain it in three beats: what problem it solves, the smallest example that shows it, and the mistake everybody makes first.\n3. Write it for somebody who does not have the prerequisite you are about to use — or name the prerequisite.\n\nWhat is read: whether a beginner gets through it without stopping. Correct and unreadable does not pass.'
    ),
    (
        'leadership',
        'Bonjour Skilluv — the first retro',
        'Write a retro on a public Skilluv incident: what happened, what it cost, what changes.',
        E'The gesture: a retro that names causes, not people.\n\n1. Pick a published Skilluv incident and read it end to end, timeline included.\n2. Write the retro: what happened, what it cost, what made it possible, and what made it stop.\n3. Propose one change, with an owner and a way to tell whether it worked.\n\nWhat is read: whether the change you propose would actually have prevented this incident. Blame is not a cause, and "be more careful" is not a change.'
    ),
    (
        'soft_skills',
        'Bonjour Skilluv — the first review',
        'Review one public deliverable: what holds, what to change, and why.',
        E'The gesture: a review its author is glad to have received.\n\n1. Pick one public deliverable and read it properly — the whole thing, before writing a word.\n2. Say what holds, and why it holds. A review that only lists problems teaches nothing about what to keep.\n3. Say what you would change, in what order, and what you are unsure about.\n\nWhat is read: specificity and tone together. "Looks good" and "this is wrong" fail for the same reason — neither is actionable.'
    )
) AS v(skill_domain, title, description, instructions)
WHERE NOT EXISTS (
    SELECT 1 FROM challenge_templates ct WHERE ct.title = v.title
);
