-- Seeded leadership challenges, one set per trade.
--
-- Drafts, as every seeded challenge is: a human publishes them after reading
-- them.
--
-- ## The problem this domain has that the others do not
--
-- Most leadership challenges as normally written require an employer. "Write
-- your team's career ladder" is not attemptable by somebody who has no team,
-- and a catalogue full of those filters on employment — which is the filter
-- this platform exists to get around.
--
-- Each challenge below is attemptable by somebody with no organisation behind
-- them. Three routes make that possible, and every challenge uses one:
--
--   * a **real public system** whose decisions can be read and argued with —
--     an open-source project's RFC repository, a public roadmap;
--   * a **stated hypothetical**, where the assumptions are part of the
--     deliverable and a reviewer judges the reasoning rather than the outcome;
--   * a **real small group** the person is already in — a jam team, a study
--     cohort, a community. Three people is a team.
--
-- The one exception is the cohort challenge, which genuinely cannot be
-- simulated. It is on this platform's own cohorts, which anybody can start.
--
-- ## Redaction
--
-- Anything submitted here that came from a real organisation goes through the
-- redaction state on the slice. A challenge answer is not an exemption from
-- migration 0460's rule, and the seeded instructions say so.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## What there is to do' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## How to attempt it without an employer' || E'\n\n' ||
    c.route || E'\n\n' ||
    '## What is expected' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'In every case: the document names what it gives up, and any claim about ' ||
    'people comes with what was measured. If it draws on a real ' ||
    'organisation, it is submitted anonymised and nobody in it is ' ||
    'identifiable — including by a detail only they would have.' || E'\n\n' ||
    '## What will be looked at' || E'\n\n' ||
    'The review grid of the family applies, and it is public: you can read ' ||
    'it before you submit.',
    'leadership', c.difficulty,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'leadership' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'leadership' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── lead-product (5) ───────────────────────────────────────────────
('lead-product', 'A problem statement before a feature',
 'Take a feature request from a real open-source project and rewrite it as the problem behind it: who has it, how often, and what they do today instead',
 'Any public issue tracker. The request already exists; the work is finding what it is actually asking for.',
 'The problem statement, the evidence behind it, and at least one solution that is not the one requested.', 2),

('lead-product', 'A quarter, and what it costs',
 'Write a quarterly roadmap for a real open-source project, with the priority order defended in terms of what is learned or unblocked',
 'A project with a public backlog. You are proposing, not deciding — say so in the document.',
 'The roadmap, the order defended, and the list of what is deliberately not in it.', 3),

('lead-product', 'Five conversations, one synthesis',
 'Talk to five people who use a product you did not build, and turn what they said into a prioritised set of findings',
 'Any product with a community. Five users of an open-source tool are reachable in a week.',
 'What was asked, what was said in their words, and what you conclude — in three separate sections.', 3),

('lead-product', 'The feature that should be removed',
 'Find a feature in a real product that is costing more than it returns, and write the case for deprecating it including the migration path for the people using it',
 'A public product. The evidence is usage figures if they exist and reasoning if they do not — say which.',
 'The case, the migration path, and what would change your mind.', 4),

('lead-product', 'Twelve months, three bets',
 'Write a twelve-month product direction as three bets, each with what it assumes and what would falsify it',
 'A real product or a stated hypothetical. If hypothetical, the assumptions are part of the deliverable.',
 'The three bets, their assumptions, and the non-goals that make them a direction rather than a list.', 4),

-- ── lead-tech (5) ──────────────────────────────────────────────────
('lead-tech', 'A decision, with its alternatives',
 'Write a decision record for a technical choice a real project has already made, reconstructing the alternatives that were available at the time',
 'Any open-source project whose history you can read. The commit that made the choice is the starting point.',
 'The decision, at least two alternatives described so their advocates would recognise them, and what would make it wrong.', 2),

('lead-tech', 'A migration with a state in between',
 'Propose a schema or storage migration for a real project, including the period where both shapes exist',
 'A public codebase. The in-between state is the part most proposals skip and the part that actually happens.',
 'The proposal, the intermediate state, the rollback, and what it costs to undo once it is done.', 4),

('lead-tech', 'Scaling something that is not yet slow',
 'Take a real system and write what would break first at ten times its current load, with the numbers behind the claim',
 'A public system, or one whose architecture is documented. Measure or estimate, and say which.',
 'What breaks first, at what figure, the options, and their costs.', 4),

('lead-tech', 'A post-incident decision record',
 'Take a published post-mortem from a real company and write the decision record that should have followed it',
 'Public post-mortems are plentiful. The work is turning "we will improve monitoring" into a decision somebody can act on.',
 'The decision, what it costs, its alternatives, and how it would be verified a year later.', 3),

('lead-tech', 'A direction for a team, in writing',
 'Write a twelve-month technical direction: what is deprecated, what is migrated, and what skills the team has to gain',
 'A real project, or a stated hypothetical whose team and constraints are described.',
 'The direction, the sequence, and what is deliberately left alone.', 4),

-- ── lead-project (4) ───────────────────────────────────────────────
('lead-project', 'An honest date',
 'Take a real project that is behind and produce a plan with a date somebody outside the team could rely on',
 'Any public project with a slipped milestone. The evidence is the tracker.',
 'The plan, the assumptions the date rests on, and the first one that would break it.', 3),

('lead-project', 'A risk register with responses',
 'Build a risk register for a real project, each risk with an owner, a response, and the signal that it is happening',
 'A public project, or a group you are in. Three people is a project.',
 'The register, and the two risks you decided to accept rather than mitigate, with the reason.', 3),

('lead-project', 'Coordinating four groups',
 'Write the delivery plan for something that needs four groups to move — a jam, a release, an event — with dependencies and the order they unblock each other in',
 'A real event or a stated hypothetical. If real, the acknowledgement of each group is part of the deliverable.',
 'The plan, the dependency map, and the communication cadence with who is in each loop.', 4),

('lead-project', 'The recovery plan',
 'Take a project that has already missed twice and write what changes — including what is being cut',
 'A public project, anonymised if it is one you were in.',
 'What is cut, what is kept, the new date, and why this one is different.', 4),

-- ── lead-people (4) ────────────────────────────────────────────────
('lead-people', 'A ladder described by what people do',
 'Write a career ladder for one track with five levels, each described by observable behaviour rather than adjectives',
 'A stated hypothetical team, described in the document: size, product, stage. The reasoning is what is judged.',
 'The ladder, and for each level one example of somebody at it and one of somebody not yet there.', 3),

('lead-people', 'A hiring loop somebody can fail fairly',
 'Design the interview process for one role: stages, questions, rubric, and how two interviewers are calibrated against each other',
 'A hypothetical role, fully described. Or a real one, anonymised.',
 'The loop, the rubric, and what a rejection tells the candidate.', 3),

('lead-people', 'One-to-ones that are not status updates',
 'Write the framework for one-to-ones in a team: structure, cadence, and what goes in them that does not go in a stand-up',
 'A real small group works: a jam team, a study cohort, a community you help run.',
 'The framework, and what it does when somebody has nothing to say.', 2),

('lead-people', 'Asking a team what it thinks',
 'Design and run a team health check on a real group you are part of, and write the plan that follows from it',
 'Any group of three or more. Anonymity of the responses is part of the design.',
 'What was asked, how anonymity was preserved, what came back, and the plan — with somebody''s time attached.', 4),

-- ── lead-community (4) ─────────────────────────────────────────────
('lead-community', 'Who it is for, and who it is not',
 'Write a twelve-month strategy for a real community, naming who it serves and who it deliberately does not',
 'Any community with a public presence, including one you are in.',
 'The strategy, the second-visit mechanism, and what is being given up to make it work.', 3),

('lead-community', 'A programme that runs on people',
 'Design an ambassador or maintainer programme: recruitment, what is asked, what is given back, and what happens when somebody stops',
 'A real community or a stated hypothetical.',
 'The programme, and an honest account of what it costs the people in it.', 3),

('lead-community', 'The moderation rules, written before the incident',
 'Write the operating playbook for a community space: what is out of bounds, who decides, how an appeal works, and how a moderator hands over',
 'A real space you help run, or one you are designing.',
 'The playbook, and the three cases it does not cover, named.', 3),

('lead-community', 'Six months of content, aimed at something',
 'Build a content plan for a community, each item tied to a thing the community is trying to become rather than to a channel',
 'A real community. The plan is judged on the link between the items and the goal.',
 'The plan, the goal it serves, and the measure that will say whether it worked.', 2),

-- ── lead-mentor (4) ────────────────────────────────────────────────
('lead-mentor', 'A curriculum with an entry condition',
 'Design a six-month curriculum for one trade, stating what somebody needs to already be able to do before starting',
 'Any trade in the Skilluv catalogue. The entry condition is the part most curricula skip.',
 'The curriculum, what each step produces, and what happens to somebody who falls behind.', 3),

('lead-mentor', 'The first ninety days',
 'Write a structured onboarding for somebody joining a technical team, in three thirty-day segments each ending in something they produced',
 'A stated hypothetical team, described. Or a real one, anonymised.',
 'The three segments, what each produces, and how the person can tell they are on track without asking.', 3),

('lead-mentor', 'A cohort, run to the end',
 'Start a cohort on this platform, run it, and bring it to a conclusion with most of the people who joined finishing',
 'Skilluv''s own cohorts. Anybody can start one, and this is the one challenge here that cannot be simulated.',
 'The cohort record, the graduation rate over everybody who joined, and what you would change.', 5),

('lead-mentor', 'What happened to the people',
 'Take a cohort or a mentoring relationship that has finished and report the outcomes with the denominator included',
 'Any cohort you led, here or elsewhere. Anonymised if elsewhere.',
 'How many started, how many finished, where they got to, and what the number does not show.', 3)

) AS c(orientation_slug, title, description, route, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug;
