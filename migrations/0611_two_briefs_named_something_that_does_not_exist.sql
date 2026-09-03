-- Two rite briefs pointed at objects nobody can open.
--
-- ## design
--
-- 0607 wrote "Take the entry brief of your trade. It is short on purpose, and
-- it does not say what the screen looks like." There is no such object. The
-- rite belongs to the domain, not to the trade — that is SKI-362's decision,
-- one rite per domain — so `challenge_templates.orientation_id` is NULL on all
-- twelve and nothing serves a per-trade entry brief. The sentence also implies
-- an orientation has already been chosen, and nothing in the signup path
-- requires one.
--
-- The brief *is* the brief. It says what to design, inline, at domain level.
--
-- ## leadership
--
-- 0607 wrote "Pick a published Skilluv incident and read it end to end,
-- timeline included." `GET /api/ops/incidents` returns the caller's own
-- practice incidents and nothing else; there is no catalogue of published
-- Skilluv incidents, and a person arriving at the leadership domain on their
-- first day has zero. The rite asked them to start from a list that is empty
-- for exactly the population it is written for.
--
-- Rewritten to an incident they can actually reach: one they lived, or any
-- public post-mortem. What the rite measures — causes rather than people, an
-- owner, a change somebody can tell worked — does not depend on whose incident
-- it was.
--
-- ## The other ten
--
-- Checked against what the API serves. `security` reads
-- `GET /api/security/scope`, which is public and needs no account.
-- `communication` reads a published `content_guides` row, `education` a
-- `skill_nodes` row, `game` a published slice, `ai` a mission, `soft_skills` a
-- public deliverable, `ops` the terrain of migration 0430, `quality` the
-- canvas. `audio` and `code` name nothing they do not carry themselves. Those
-- ten stand.

UPDATE challenge_templates
SET instructions =
    E'The gesture: one screen, finished enough to be argued with.\n\n'
    '1. The brief, and it is the whole brief: a screen somebody uses once and '
    'should never need twice — a sign-in, a first run, a confirmation. Pick '
    'which one. It does not say what the screen looks like; that part is yours.\n'
    '2. Design one screen against it. One. A flow of six half-screens is not '
    'this rite.\n'
    '3. Upload it, with the two or three sentences that say what decision each '
    'choice serves.\n\n'
    'What is read: the fit between the brief and the screen. A beautiful screen '
    'answering a different brief does not pass, and that is the whole lesson.'
WHERE is_domain_rite AND skill_domain = 'design';

UPDATE challenge_templates
SET instructions =
    E'The gesture: a retro that names causes, not people.\n\n'
    '1. Pick an incident you can read end to end — one you lived through, or '
    'any public post-mortem. Name it, and link it if it is public.\n'
    '2. Write the retro: what happened, what it cost, what made it possible, '
    'and what made it stop.\n'
    '3. Propose one change, with an owner and a way to tell whether it worked.\n\n'
    'What is read: whether the change you propose would actually have prevented '
    'this incident. Blame is not a cause, and "be more careful" is not a change.'
WHERE is_domain_rite AND skill_domain = 'leadership';
