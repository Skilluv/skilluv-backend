-- The leadership skill tree, and which trade needs which part of it.
--
-- ## What is deliberately not created here
--
-- `soft_skills` already holds most of the vocabulary this domain would have
-- invented: `adr-writing`, `roadmap-thinking`, `technical-decision-making`,
-- `leadership-technical`, `mentoring-junior`, `technical-1on1`,
-- `giving-feedback`, `receiving-feedback`, `stakeholder-communication`,
-- `project-scoping`, `scope-negotiation`, `incident-postmortem`,
-- `written-communication`, `async-communication`. Ops holds
-- `postmortem-facilitation` and `incident-communication`.
--
-- Fourteen nodes that would have been re-created under a `lead-` prefix if
-- this migration had followed the backlog's "~50 relations" literally. Two
-- nodes meaning one competence is worse than a missing one: the tree
-- deduplicates nothing, the recommender shows both, and a profile claims two
-- skills for one thing somebody can do.
--
-- ## Where the boundary between `soft_skills` and `leadership` falls
--
-- `soft_skills` holds what somebody does *with* another person: giving
-- feedback, running a one-to-one, writing clearly. Leadership holds what
-- somebody produces *for* other people to act on: a plan, a decision record, a
-- ladder, a curriculum.
--
-- That line is why `mentoring-junior` stays in soft_skills and
-- `curriculum-design` is created here. Mentoring one person is a
-- relationship; designing the path twenty people will take is a document, and
-- it is reviewed as one.

-- ═══════════════════════════════════════════════════════════════════
-- The families
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('direction-setting', 'Direction setting',
 'Deciding what is pursued and what is given up, and writing the reason down so the decision can be argued with later.', 'leadership'),
('written-decisions', 'Written decisions',
 'Recording a choice before it is taken: the alternatives, the trade-offs, and what would make it the wrong one.', 'leadership'),
('delivery-leadership', 'Delivery leadership',
 'Holding a plan that survives contact with reality — dependencies, risks, and a date somebody outside the team can rely on.', 'leadership'),
('people-frameworks', 'People frameworks',
 'The structures other people grow inside: expectations, hiring, progression. Documents, not conversations.', 'leadership'),
('community-building', 'Community building',
 'Building a place people come back to, and being able to say why they came back.', 'leadership'),
('curriculum-and-cohorts', 'Curriculum and cohorts',
 'Designing how somebody gets from where they are to where the work is, and running it with a group.', 'leadership'),
('leadership-integrity', 'Leadership integrity',
 'What holds when the document leaves the room: confidentiality, attribution, and claims that can be checked.', 'leadership')
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What each family contains
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT c.slug, c.display_name, c.description, 'leadership', p.id
FROM (VALUES

-- Direction setting
('problem-framing',        'Problem framing',           'Finding the problem behind a feature request. Who has it, how often, and what they do instead today.', 'direction-setting'),
('prioritisation-defence', 'Defending a priority order','Why this before that, in terms of what is learned or unblocked — not what is ready.', 'direction-setting'),
('non-goals',              'Naming non-goals',          'What is deliberately not being done. A direction that pursues everything has decided nothing.', 'direction-setting'),
('okr-design',             'Objective design',          'Targets that can move downwards. One that can only improve is a scoreboard.', 'direction-setting'),
('discovery-interviews',   'Discovery interviews',      'Talking to people who use the thing, and keeping what they said apart from what you concluded.', 'direction-setting'),
('deprecation-planning',   'Deprecation planning',      'Removing something people depend on, with the migration path and the notice period.', 'direction-setting'),

-- Written decisions
('alternatives-analysis',  'Analysing alternatives',    'Describing the options you did not choose well enough that their advocates would recognise them.', 'written-decisions'),
('falsifiability',         'Saying what would be wrong','The condition under which the decision should be revisited. What separates a decision from an opinion.', 'written-decisions'),
('migration-path-design',  'Designing a migration path','Including the state where both shapes exist, which is the part that actually happens.', 'written-decisions'),
('reversibility-analysis', 'Costing the reversal',      'What it takes to undo. Some decisions are one-way, and saying so is the point.', 'written-decisions'),
('scale-estimation',       'Estimating scale',          'The load, the volume, the team size a decision assumes. An architecture with no numbers fits everything and suits nothing.', 'written-decisions'),

-- Delivery leadership
('dependency-mapping',     'Mapping dependencies',      'Including the ones outside the team, and whether their owners have agreed.', 'delivery-leadership'),
('risk-registers',         'Risk registers',            'Each risk with an owner, a response, and the signal that it is happening. A list without responses is a list of regrets.', 'delivery-leadership'),
('honest-estimation',      'Honest estimation',         'A date with its assumptions attached, and the first one that would break it.', 'delivery-leadership'),
('cross-team-coordination','Cross-team coordination',   'Getting several groups to move in an order that unblocks each other, and telling them so.', 'delivery-leadership'),
('scope-cutting',          'Cutting scope',             'Deciding what comes out when the date will not move, and saying who decided.', 'delivery-leadership'),
('delivery-communication', 'Delivery communication',    'What outside the team gets told, how often, and what a bad week sounds like.', 'delivery-leadership'),

-- People frameworks
('career-ladder-design',   'Designing a career ladder', 'Levels described by things somebody does, not by adjectives.', 'people-frameworks'),
('hiring-loop-design',     'Designing a hiring loop',   'Stages, questions and a rubric that is the same for everybody.', 'people-frameworks'),
('interview-calibration',  'Calibrating interviewers',  'Two people scoring the same evidence the same way. Without it a loop measures the interviewer.', 'people-frameworks'),
('team-health-measurement','Measuring team health',     'What was asked, of how many, and how anonymity was preserved.', 'people-frameworks'),
('difficult-conversations','Difficult conversations',   'What happens when somebody is not meeting expectations, written down before anybody is.', 'people-frameworks'),
('onboarding-design',      'Designing onboarding',      'Thirty, sixty, ninety days, each ending in something the person produced.', 'people-frameworks'),

-- Community building
('community-positioning',  'Community positioning',     'Who it is for, and — harder — who it is not for. A community for everybody retains nobody.', 'community-building'),
('retention-mechanics',    'Retention mechanics',       'What brings somebody back a second time. Counting first arrivals measures the announcement.', 'community-building'),
('volunteer-programmes',   'Volunteer programmes',      'What is asked, what is given back, and what happens when somebody stops.', 'community-building'),
('moderation-policy',      'Moderation policy',         'What is out of bounds, who decides, and how an appeal works — written before the first incident.', 'community-building'),
('event-organisation',     'Organising events',         'Format, budget, the people who run it, and what happens if half as many come.', 'community-building'),

-- Curriculum and cohorts
('curriculum-design',      'Curriculum design',         'A sequence where each step produces an artefact, with a stated entry condition.', 'curriculum-and-cohorts'),
('cohort-facilitation',    'Facilitating a cohort',     'Keeping a group moving together when they are not moving at the same speed.', 'curriculum-and-cohorts'),
('learning-outcome-design','Designing learning outcomes','What somebody can do afterwards that they could not before, stated so it can be checked.', 'curriculum-and-cohorts'),
('dropout-design',         'Designing for falling behind','What happens to the person who misses two weeks. Most curricula assume nobody does.', 'curriculum-and-cohorts'),
('outcome-reporting',      'Reporting outcomes',        'With the denominator. A graduation rate over survivors is not one.', 'curriculum-and-cohorts'),

-- Integrity
('document-redaction',     'Redacting a document',      'Rewriting so nobody can be identified, including by a detail only they would have. Harder than it looks and never automatic.', 'leadership-integrity'),
('confidentiality-practice','Handling what you were told','What stays inside an engagement, and for how long after it ends.', 'leadership-integrity'),
('claim-substantiation',   'Substantiating a claim',    'Making a statement about people checkable. This is the domain where unfalsifiable claims are easiest to make.', 'leadership-integrity')

) AS c(slug, display_name, description, parent_slug)
JOIN skill_nodes p ON p.slug = c.parent_slug
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Which trade needs which
-- ═══════════════════════════════════════════════════════════════════
--
-- Core means somebody without it cannot do the job. `document-redaction` and
-- `claim-substantiation` are core to all six, which is unusual and correct:
-- they are the two things this domain refuses work over, and a trade where
-- they were merely recommended would be a trade where the refusal is a
-- surprise.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended)
SELECT o.id, s.id, m.is_core, NOT m.is_core
FROM (VALUES

-- ── lead-product ───────────────────────────────────────────────────
('lead-product', 'problem-framing',          TRUE),
('lead-product', 'prioritisation-defence',   TRUE),
('lead-product', 'non-goals',                TRUE),
('lead-product', 'discovery-interviews',     TRUE),
('lead-product', 'roadmap-thinking',         TRUE),
('lead-product', 'document-redaction',       TRUE),
('lead-product', 'claim-substantiation',     TRUE),
('lead-product', 'okr-design',               FALSE),
('lead-product', 'deprecation-planning',     FALSE),
('lead-product', 'stakeholder-communication',FALSE),
('lead-product', 'scope-negotiation',        FALSE),
('lead-product', 'written-communication',    FALSE),

-- ── lead-tech ──────────────────────────────────────────────────────
('lead-tech', 'alternatives-analysis',       TRUE),
('lead-tech', 'falsifiability',              TRUE),
('lead-tech', 'technical-decision-making',   TRUE),
('lead-tech', 'adr-writing',                 TRUE),
('lead-tech', 'document-redaction',          TRUE),
('lead-tech', 'claim-substantiation',        TRUE),
('lead-tech', 'migration-path-design',       FALSE),
('lead-tech', 'reversibility-analysis',      FALSE),
('lead-tech', 'scale-estimation',            FALSE),
('lead-tech', 'leadership-technical',        FALSE),
('lead-tech', 'code-review-teaching',        FALSE),
('lead-tech', 'incident-postmortem',         FALSE),
('lead-tech', 'rfc-implementation',          FALSE),

-- ── lead-project ───────────────────────────────────────────────────
('lead-project', 'dependency-mapping',       TRUE),
('lead-project', 'risk-registers',           TRUE),
('lead-project', 'honest-estimation',        TRUE),
('lead-project', 'scope-cutting',            TRUE),
('lead-project', 'document-redaction',       TRUE),
('lead-project', 'claim-substantiation',     TRUE),
('lead-project', 'cross-team-coordination',  FALSE),
('lead-project', 'delivery-communication',   FALSE),
('lead-project', 'project-scoping',          FALSE),
('lead-project', 'scope-negotiation',        FALSE),
('lead-project', 'stakeholder-communication',FALSE),
('lead-project', 'async-communication',      FALSE),

-- ── lead-people ────────────────────────────────────────────────────
('lead-people', 'career-ladder-design',      TRUE),
('lead-people', 'hiring-loop-design',        TRUE),
('lead-people', 'difficult-conversations',   TRUE),
('lead-people', 'team-health-measurement',   TRUE),
('lead-people', 'document-redaction',        TRUE),
('lead-people', 'claim-substantiation',      TRUE),
('lead-people', 'interview-calibration',     FALSE),
('lead-people', 'onboarding-design',         FALSE),
('lead-people', 'technical-1on1',            FALSE),
('lead-people', 'giving-feedback',           FALSE),
('lead-people', 'receiving-feedback',        FALSE),
('lead-people', 'confidentiality-practice',  FALSE),

-- ── lead-community ─────────────────────────────────────────────────
('lead-community', 'community-positioning',  TRUE),
('lead-community', 'retention-mechanics',    TRUE),
('lead-community', 'moderation-policy',      TRUE),
('lead-community', 'document-redaction',     TRUE),
('lead-community', 'claim-substantiation',   TRUE),
('lead-community', 'volunteer-programmes',   FALSE),
('lead-community', 'event-organisation',     FALSE),
('lead-community', 'async-communication',    FALSE),
('lead-community', 'written-communication',  FALSE),
('lead-community', 'giving-feedback',        FALSE),

-- ── lead-mentor ────────────────────────────────────────────────────
('lead-mentor', 'curriculum-design',         TRUE),
('lead-mentor', 'learning-outcome-design',   TRUE),
('lead-mentor', 'cohort-facilitation',       TRUE),
('lead-mentor', 'dropout-design',            TRUE),
('lead-mentor', 'outcome-reporting',         TRUE),
('lead-mentor', 'document-redaction',        TRUE),
('lead-mentor', 'claim-substantiation',      TRUE),
('lead-mentor', 'mentoring-junior',          FALSE),
('lead-mentor', 'onboarding-design',         FALSE),
('lead-mentor', 'giving-feedback',           FALSE),
('lead-mentor', 'code-review-teaching',      FALSE),
('lead-mentor', 'technical-1on1',            FALSE)

) AS m(orientation_slug, skill_slug, is_core)
JOIN orientations o ON o.slug = m.orientation_slug
JOIN skill_nodes s ON s.slug = m.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;
