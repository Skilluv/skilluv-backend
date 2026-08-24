-- Thirteen leadership distinctions.
--
-- ## Why the thresholds are the lowest on the platform
--
-- `code-craft-master` is thirty verified deliverables. A quarterly roadmap
-- that survived its quarter, a decision record with real alternatives, a
-- cohort run to graduation — each is months, and several of them are a year.
-- Fifteen here is not a lower bar than thirty there; it is the same bar
-- measured in a unit that is four times larger.
--
-- ## The one nobody else awards
--
-- `lead-followed-through`. Every recognition scheme in this trade rewards the
-- document: the roadmap presented, the retro facilitated, the strategy
-- announced. Nothing rewards the three months afterwards of making the action
-- items actually happen, which is the part that decides whether any of it was
-- worth writing. It rests on retrospectives whose actions closed inside the
-- quarter, which is the closest a schema gets to "they did the boring half".
--
-- ## The one that is deliberately not about being right
--
-- `lead-decision-recorded` fires on a decision written down with its
-- alternatives, adopted or not. A badge that only counted accepted proposals
-- would teach people to propose what will pass, which is the opposite of what
-- the trade needs.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('lead-first-artifact', 'medal',
 'First leadership document',
 'A first verified leadership artefact. The moment the profile stops being a job title.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "leadership", "min_count": 1}', 'common'),

('lead-decision-recorded', 'medal',
 'Decision written down',
 'A technical decision recorded with its real alternatives — adopted or not. Being right is not what this counts.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_decision_recorded", "min_count": 1}', 'common'),

('lead-roadmap-shipper', 'medal',
 'Direction held',
 'Five validated roadmaps or delivery plans. Five quarters somebody else planned around.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_roadmap_validated", "min_count": 5}', 'rare'),

('lead-rfc-author', 'medal',
 'Proposals adopted',
 'Five written proposals an organisation took up.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_rfc_accepted", "min_count": 5}', 'epic'),

('lead-followed-through', 'medal',
 'Did the boring half',
 'Five retrospectives whose action items were owned, dated and mostly closed inside the quarter. Every scheme in this trade rewards the meeting; this one rewards the three months afterwards.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_retrospective_facilitated", "min_count": 5}', 'epic'),

('lead-playbook-author', 'medal',
 'Playbook that outlived you',
 'A playbook a team kept using after its author stopped enforcing it.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_playbook_published", "min_count": 1}', 'rare'),

('lead-cohort-mentor', 'medal',
 'Cohort led to the end',
 'A cohort run from start to graduation, with most of the people who joined finishing it.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_cohort_completed", "min_count": 1}', 'epic'),

('lead-cohort-veteran', 'medal',
 'Three cohorts',
 'Three cohorts led to their end. The second one is where somebody finds out whether the first was the group or the design.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_cohort_completed", "min_count": 3}', 'legendary'),

('lead-community-impact', 'medal',
 'A number that moved',
 'A community initiative that changed something somebody can name, in a direction somebody wanted.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "leadership_community_initiative_impact", "min_count": 1}', 'rare'),

('lead-craft-master', 'medal',
 'Leadership craft master',
 'Fifteen verified leadership artefacts. In this domain that is several years.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "leadership", "min_count": 15}', 'epic'),

('lead-craft-legend', 'medal',
 'Leadership craft legend',
 'Fifty verified leadership artefacts.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "leadership", "min_count": 50}', 'legendary'),

('lead-cross-domain', 'medal',
 'Led across three domains',
 'Verified leadership work aimed at three different domains. Holding a direction for a game team, a platform team and a design team are three different jobs.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "leadership", "distinct_over": "target_domain", "min_count": 3}', 'epic'),

('lead-featured', 'medal',
 'Featured',
 'Leadership work the community singled out as exemplary.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "featured_leader", "min_count": 1}', 'rare')

ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The one a human decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('lead-event-organiser', 'medal',
 'Ran the event',
 'Proposed an event the community voted for, and then actually ran it.',
 -- The engine can count the contest win. It cannot count the six weeks
 -- afterwards, and a rule counting only the vote would award this to
 -- somebody whose event was never held — which is the exact failure the
 -- distinction is meant to avoid.
 '{"manual": true}', 'epic')

ON CONFLICT (slug) DO NOTHING;
