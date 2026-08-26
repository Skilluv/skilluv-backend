-- The leadership domain: score, tiers, missions, contests, awards, grids.
--
-- ## Four tickets that needed no table
--
-- `leadership_missions` (leadership/M-01) is `missions` with
-- `skill_domain = 'leadership'`. The backlog lists `retainer_duration_months`
-- as something new; `missions.retainer_monthly` has existed since 0192, and
-- the retainer is the payment model rather than a second kind of engagement.
--
-- The hackathon organiser contest (C-01) is a `tournament_kinds` row with
-- `allows_community_vote`, which the format table has carried since 0438. The
-- awards (C-02) are `award_categories`, and their seventy-thirty community-jury
-- split is already the default on `award_editions`.

-- ═══════════════════════════════════════════════════════════════════
-- What a leadership score counts
-- ═══════════════════════════════════════════════════════════════════
--
-- Four departures from the backlog's formula.
--
-- `decisions_recorded` is new and sits below `rfcs_accepted`. The backlog only
-- scored accepted proposals, and a domain that only rewards proposals which
-- passed teaches people to propose what will pass. A well-argued rejected RFC
-- is leadership work; it is worth less, and it is worth something.
--
-- A retrospective is worth 45 rather than 30. Facilitating the hour is easy.
-- What the attestation actually rests on is seventy per cent of the action
-- items being resolved within ninety days, which is three months of somebody
-- chasing people, and it is the rarest thing in this domain.
--
-- `mentees_graduated` is new. `cohorts_completed` alone scores a cohort of
-- three and a cohort of twenty identically, which says something false about
-- both.
--
-- `commitments_acknowledged` is new, and it is the term this domain most
-- needed. Everything else here can be produced alone at a desk. A commitment
-- another project's steward has read and accepted cannot, and it is the
-- closest thing leadership has to a merged pull request.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('leadership', 'attestations_leadership', 10, 'count', NULL,
     'Each leadership attestation issued.', 10),
    ('leadership', 'roadmaps_validated', 40, 'count', NULL,
     'Each validated roadmap or delivery plan — commitments, dependencies, '
     'and what it deliberately leaves out.', 20),
    ('leadership', 'decisions_recorded', 25, 'count', NULL,
     'Each technical decision written down with its alternatives, whether or '
     'not it was adopted. A well-argued rejected proposal is leadership work.', 30),
    ('leadership', 'rfcs_accepted', 50, 'count', NULL,
     'Each written proposal an organisation adopted.', 40),
    ('leadership', 'retrospectives_followed_through', 45, 'count', NULL,
     'Each retrospective whose action items were mostly resolved within the '
     'quarter. Facilitating the hour is easy; the three months of chasing '
     'afterwards is what this counts.', 50),
    ('leadership', 'cohorts_completed', 200, 'count', NULL,
     'Each cohort led to its end with most of the people who joined '
     'finishing it.', 60),
    ('leadership', 'mentees_graduated', 30, 'count', NULL,
     'Each person who finished a cohort this person led. A cohort of three '
     'and a cohort of twenty are not the same undertaking.', 70),
    ('leadership', 'people_frameworks_validated', 45, 'count', NULL,
     'Each validated career ladder, hiring process or team health audit — a '
     'structure other people are assessed or grown inside.', 75),
    ('leadership', 'playbooks_published', 60, 'count', NULL,
     'Each playbook a team kept using after its author left.', 80),
    ('leadership', 'community_initiatives_impact', 80, 'count', NULL,
     'Each community initiative that moved a number somebody can name.', 90),
    ('leadership', 'commitments_acknowledged', 20, 'count', NULL,
     'Each commitment another project''s steward read and accepted. The one '
     'term here that cannot be produced alone at a desk.', 100),
    ('leadership', 'missions_completed', 150, 'count', NULL,
     'Each paid leadership engagement carried through.', 110),
    ('leadership', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'The average of the review grids received, counted from 3 out of 5.', 120),
    ('leadership', 'years_active', 30, 'count', NULL,
     'Each year since the first validated artefact.', 130),
    ('leadership', 'featured_times', 200, 'count', NULL,
     'Each featuring by the community.', 140)
ON CONFLICT (skill_domain, term) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The tiers get this domain's words
-- ═══════════════════════════════════════════════════════════════════
--
-- Slugs and thresholds unchanged, for the reason migration 0452 gives:
-- `craft_scores.tier_slug` is what the recruiter search filters on, and a
-- domain whose second tier is called something else is a domain the "Senior
-- and above" filter silently reads differently.

UPDATE craft_score_tiers SET name = v.name, description = v.description
  FROM (VALUES
    ('apprentice',  'Aspiring lead',
     'The first documents. Nobody has planned their quarter around one yet.'),
    ('contributor', 'Contributor',
     'Writes decisions down, and the people they affect have read them.'),
    ('engineer',    'Lead',
     'Holds a direction other people work inside, and can say what it gives up.'),
    ('senior',      'Senior lead',
     'Runs things that outlast their attention: a playbook still used, a '
     'cohort that graduated without them in the room.'),
    ('staff',       'Principal lead',
     'The way an organisation decides carries this person''s mark.'),
    ('principal',   'Executive',
     'A practice that changed how other organisations decide, not just this '
     'one.')
  ) AS v(slug, name, description)
 WHERE craft_score_tiers.skill_domain = 'leadership'
   AND craft_score_tiers.slug = v.slug;

-- ═══════════════════════════════════════════════════════════════════
-- Leadership missions
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('leadership_product_strategy', 'leadership', 'Product strategy',
     'Decide what a product is for and what it will stop doing. Fixed price '
     'on a document, or a monthly retainer while it is being applied.', 10),
    ('leadership_tech_lead_fractional', 'leadership', 'Fractional tech lead',
     'Part-time technical direction for a team without one: decisions '
     'recorded, reviews led, and somebody accountable for them. Monthly.', 20),
    ('leadership_delivery_recovery', 'leadership', 'Delivery recovery',
     'Take a project that is late and produce a plan somebody outside the '
     'team can rely on. Fixed price, and the first deliverable is usually the '
     'honest date.', 30),
    ('leadership_pm_fractional', 'leadership', 'Fractional product manager',
     'Part-time product direction: discovery, prioritisation, and the '
     'roadmap that follows. Monthly.', 40),
    ('leadership_team_health_audit', 'leadership', 'Team health audit',
     'Find out what a team actually thinks, and hand back a plan rather than '
     'a survey. Fixed price, with confidentiality that survives the '
     'engagement.', 50),
    ('leadership_hiring_design', 'leadership', 'Hiring process design',
     'Job description, interview loop, rubrics and calibration, for a role '
     'the client keeps failing to fill. Fixed price.', 60),
    ('leadership_mentoring_engagement', 'leadership', 'Mentoring engagement',
     'Run a cohort or accompany named people to a stated outcome. Priced per '
     'cohort, and the outcome is written before it starts.', 70),
    ('leadership_community_strategy', 'leadership', 'Community strategy',
     'Decide who a community is for and what brings them back. Fixed price '
     'on the strategy, monthly while it is being run.', 80)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO mission_deliverable_formats
    (slug, skill_domain, name, description, sort_order)
VALUES
    ('written_strategy', 'leadership', 'Written strategy',
     'A document that says what is being pursued, what is being given up, '
     'and how the client will know it worked.', 510),
    ('decision_record', 'leadership', 'Decision record',
     'One or more decisions written down with their alternatives and '
     'trade-offs, in a form the client''s own repository can hold.', 520),
    ('facilitated_workshops', 'leadership', 'Facilitated workshops',
     'Sessions run with the client''s people, and the written output that '
     'came out of them.', 530),
    ('cohort_led', 'leadership', 'Cohort led',
     'A group accompanied from start to a stated outcome, with the '
     'attendance and the outcomes recorded.', 540),
    ('recurring_engagement', 'leadership', 'Recurring engagement',
     'Standing availability with an agreed cadence — the retainer shape. '
     'What is handed over is a period, not a file.', 550)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Leadership contests
-- ═══════════════════════════════════════════════════════════════════
--
-- Neither is measured, and only one is juried. An event pitch is decided by
-- the people who would attend, which is the only opinion that predicts
-- whether the event happens; a written-decision contest is decided by a jury,
-- because "which of these two architectures is better argued" is not a
-- popularity question.

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, required_rule_keys, sort_order, is_juried,
     allows_community_vote)
VALUES
    ('event_pitch', 'leadership', 'Event pitch',
     'Propose an event the community would actually come to: format, theme, '
     'what it costs to run, and who runs it. The community votes, because '
     'the people who would attend are the only ones whose opinion predicts '
     'whether it happens.',
     TRUE, FALSE, FALSE, '{event_format,budget_ceiling}', 210,
     FALSE, TRUE),
    ('decision_defence', 'leadership', 'Decision defence',
     'One brief, several written decisions, and a jury reading which of them '
     'holds up. Judged on the alternatives explored and on what the author '
     'says would make them wrong — not on which option they picked.',
     TRUE, FALSE, FALSE, '{brief_url}', 220,
     TRUE, FALSE)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Leadership awards
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO award_categories (slug, name, description, subject_type, sort_order)
VALUES
    ('leadership_vision_of_the_year', 'Product vision of the year',
     'The direction that was still the right one twelve months later.',
     'deliverable', 410),
    ('leadership_decision_of_the_year', 'Decision of the year',
     'The written decision whose alternatives section taught the most people '
     'the most.',
     'deliverable', 420),
    ('leadership_cohort_of_the_year', 'Cohort of the year',
     'The run whose people are furthest from where they started.',
     'user', 430),
    ('leadership_initiative_of_the_year', 'Community initiative of the year',
     'The thing that changed a number, in a direction somebody wanted.',
     'deliverable', 440),
    ('leadership_culture_of_the_year', 'Team culture of the year',
     'The team other people describe as the reason they stayed.',
     'user', 450),
    ('leadership_rookie_of_the_year', 'First-year lead of the year',
     'Somebody in their first two years of leading, who did something the '
     'people around them are still using.',
     'user', 460)
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- What a leadership reviewer looks at
-- ═══════════════════════════════════════════════════════════════════
--
-- Five families, five grids, plus the domain default.
--
-- The three refusals at the top are not criteria to score. A document that
-- still identifies somebody who did not agree to be identified, a plan with
-- nothing given up, and a claim about people with no way of checking it: none
-- is compensated by the quality of the rest.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('leadership', NULL, 'Leadership — shared criteria', '[
  {"criterion": "Nobody is identifiable who did not agree to be", "looks_like": "No organisation, team or person named in an anonymised document, including by a detail only they would have. Refused outright."},
  {"criterion": "Something is given up", "looks_like": "The document names what it is not doing and what that costs. A plan that pursues everything has decided nothing. Refused outright."},
  {"criterion": "A claim about people is checkable", "looks_like": "\"The team was happier\" comes with what was measured and when. Refused outright otherwise — this is the domain where unfalsifiable claims are easiest to make."},
  {"criterion": "Somebody outside the room could act on it", "looks_like": "A reader who was not present knows what to do next without asking the author."},
  {"criterion": "The reasoning survives the conclusion", "looks_like": "A reader who disagrees with the decision can still see how it was reached, and argue with the step rather than the outcome."},
  {"criterion": "AI use is declared", "looks_like": "Using an assistant is declared. It is allowed; hiding it is not."}
]'),

('leadership', 'delivery', 'Product and delivery — review grid', '[
  {"criterion": "The problem comes before the solution", "looks_like": "Who has the problem, how it was found out, and what they do today instead. A roadmap that opens on features is a wish list."},
  {"criterion": "The order is defended", "looks_like": "Why this before that, in terms of what is learned or unblocked — not in terms of what is ready."},
  {"criterion": "Dependencies are named with their owners", "looks_like": "Including the ones outside the team, and whether those owners have agreed."},
  {"criterion": "The risks have responses", "looks_like": "A risk register with no mitigations is a list of things to be sad about later."},
  {"criterion": "Success can fail", "looks_like": "The measure would move down if the plan does not work. A metric that can only improve is a scoreboard."},
  {"criterion": "The date is honest", "looks_like": "A date with its assumptions attached, and the first one that breaks it named."}
]'),

('leadership', 'technical', 'Technical decisions — review grid', '[
  {"criterion": "Alternatives were genuinely explored", "looks_like": "At least two others, each described well enough that its advocate would recognise it. A straw man is worse than no alternative."},
  {"criterion": "The trade-off is stated in both directions", "looks_like": "What the chosen option costs, not only what it gives."},
  {"criterion": "What would make this wrong", "looks_like": "The condition under which the decision should be revisited. The field that separates a decision from an opinion."},
  {"criterion": "The migration path exists", "looks_like": "How to get from what is there today to what is proposed, including the state in between where both exist."},
  {"criterion": "The reversal is costed", "looks_like": "What it would take to undo. Some decisions are one-way, and saying so is the point."},
  {"criterion": "Scale is named", "looks_like": "The load, the data volume, the team size the decision assumes. An architecture with no numbers fits every situation and suits none."}
]'),

('leadership', 'people', 'People — review grid', '[
  {"criterion": "Expectations are observable", "looks_like": "A level is described by things somebody does, not by adjectives. \"Senior engineers show ownership\" is not a ladder."},
  {"criterion": "The process is the same for everybody", "looks_like": "Same questions, same rubric, same order. A loop that varies by candidate is a loop that measures the interviewer."},
  {"criterion": "The uncomfortable part is written down", "looks_like": "What happens when somebody does not meet the expectations, described before anybody is in that situation."},
  {"criterion": "Claims about the team have evidence", "looks_like": "What was asked, of how many people, and how anonymity was preserved."},
  {"criterion": "The plan has a cost", "looks_like": "Somebody''s time. An initiative that costs nothing is one nobody is doing."},
  {"criterion": "It survives the author", "looks_like": "The ladder still works when the person who wrote it is no longer calibrating it."}
]'),

('leadership', 'community', 'Community — review grid', '[
  {"criterion": "It names who it is for", "looks_like": "And, harder, who it is not for. A community built for everybody retains nobody."},
  {"criterion": "Retention rather than arrival", "looks_like": "What brings somebody back a second time. Counting who showed up once measures the announcement."},
  {"criterion": "The volunteers are accounted for", "looks_like": "What is being asked of them, what they get, and what happens when they stop. A programme that runs on unpaid goodwill says so."},
  {"criterion": "It works without the author present", "looks_like": "A community that needs one person online is one person''s hobby."},
  {"criterion": "Moderation is decided in advance", "looks_like": "What is out of bounds, who decides, and what the appeal is — written before the first incident."},
  {"criterion": "Inclusion is a mechanism, not a sentence", "looks_like": "Something in the design that changes who can take part: the hour a session runs, the language, the cost of the tool."}
]'),

('leadership', 'teaching', 'Mentoring and curriculum — review grid', '[
  {"criterion": "The starting point is stated", "looks_like": "What somebody needs to already be able to do. A curriculum with no entry condition is one where half the cohort is lost by week two."},
  {"criterion": "Each step produces something", "looks_like": "The learner finishes each part holding an artefact, not a feeling of having understood."},
  {"criterion": "Progress is visible to the learner", "looks_like": "They can tell whether they are getting it without asking the mentor."},
  {"criterion": "The dropout path is designed", "looks_like": "What happens to somebody who falls behind. Most curricula assume nobody does."},
  {"criterion": "It scales past one mentor", "looks_like": "Another person could run it from what is written, or the document says honestly that it cannot be."},
  {"criterion": "Outcomes are reported with the denominator", "looks_like": "How many finished out of how many started. A graduation rate over survivors is not one."}
]')

ON CONFLICT DO NOTHING;
