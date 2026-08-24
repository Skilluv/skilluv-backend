-- Fourteen quality distinctions.
--
-- ## The thresholds, and why they are not the code ones
--
-- `code-craft-master` is thirty verified deliverables. A usability study with
-- five recruited participants, a penetration test with signed rules of
-- engagement, a suite another team adopts — each of those is weeks of work
-- with somebody else's calendar in the middle of it. The counts below follow
-- what the work actually takes rather than copying a number from a domain
-- where a deliverable can be an afternoon.
--
-- ## The one that counts an outcome instead of an output
--
-- `quality-fix-shipped`. Every other domain's first badge fires on a verified
-- deliverable, which is a thing the author produced. This one fires on a
-- confirmed bug report — a defect somebody else fixed, and that the reporter
-- went back and re-checked. It is the only badge on the platform whose
-- condition cannot be satisfied by working harder alone, and it is the right
-- first badge for this trade.
--
-- ## The one that rests on the new dimension
--
-- `quality-cross-domain` counts distinct `target_domain` values on verified
-- work. The backlog phrased it as "validated in three or more
-- sub-orientations", which would have been satisfied by somebody holding
-- three quality orientations and testing the same product three ways. What is
-- worth recognising is having put three different kinds of system to the
-- test, and that is what the column records.
--
-- ## The one a human decides
--
-- `quality-bug-bash-champion`. Winning a defect hunt is a real thing to
-- recognise and the engine can count a contest win, but "champion" as the
-- community uses it also covers running one, and no row records that. It says
-- a human decides.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('quality-first-artifact', 'medal',
 'First quality artefact',
 'A first verified quality deliverable. The moment the profile stops being a claim.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "quality", "min_count": 1}', 'common'),

('quality-fix-shipped', 'medal',
 'A fix shipped because of you',
 'A defect reported, fixed by somebody else, and re-checked by the person who found it. The only distinction here that cannot be earned alone.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_bug_report_validated", "min_count": 1}', 'common'),

('quality-bug-hunter', 'medal',
 'Defect hunter',
 'Fifteen confirmed defect reports. Fifteen times a team changed something because of a description precise enough to act on.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_bug_report_validated", "min_count": 15}', 'rare'),

('quality-suite-shipped', 'medal',
 'Test suite adopted',
 'A suite another team runs in its own pipeline, without its author in the room.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_automation_shipped", "min_count": 1}', 'rare'),

('quality-coverage-champion', 'medal',
 'Coverage analyst',
 'Three accepted coverage analyses. Not a percentage: a ranking of the gaps that mattered.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_coverage_analysis_accepted", "min_count": 3}', 'rare'),

('quality-researcher', 'medal',
 'Study carried out',
 'A usability study with a protocol, real sessions, and findings kept apart from inferences.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_usability_study_completed", "min_count": 1}', 'rare'),

('quality-a11y-auditor', 'medal',
 'Accessibility audit delivered',
 'An audit against a named standard and level, every defect carrying its criterion and a costed fix.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_a11y_audit_delivered", "min_count": 1}', 'rare'),

('quality-strategist', 'medal',
 'Strategy owned',
 'A team test strategy validated: what is put to the test, what is given up, and who accepted the risk.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_test_strategy_validated", "min_count": 1}', 'epic'),

('quality-playtest-facilitator', 'medal',
 'Playtests facilitated',
 'Five validated playtest reports. Five times a game team was able to decide something from a session it did not run.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "quality_playtest_report_validated", "min_count": 5}', 'rare'),

('quality-craft-master', 'medal',
 'Quality craft master',
 'Fifteen verified quality deliverables. Regularity, not a single showpiece.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "quality", "min_count": 15}', 'epic'),

('quality-craft-legend', 'medal',
 'Quality craft legend',
 'Fifty verified quality deliverables.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "quality", "min_count": 50}', 'legendary'),

('quality-cross-domain', 'medal',
 'Tested across three domains',
 'Verified work aimed at three different domains. Putting a codebase, an interface and a game to the test are three trades, and holding all three is rare.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "quality", "distinct_over": "target_domain", "min_count": 3}', 'epic'),

('quality-featured', 'medal',
 'Featured',
 'Testing work the community singled out as exemplary.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "featured_quality_engineer", "min_count": 1}', 'rare')

ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The one a human decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('quality-bug-bash-champion', 'medal',
 'Defect hunt champion',
 'Ran or won a defect hunt that produced findings a team acted on.',
 -- The engine could count a contest win. It could not count having organised
 -- one, and the community uses the word for both. A rule that counted only
 -- the win would award this to half the people it is meant for and silently
 -- exclude the other half.
 '{"manual": true}', 'epic')

ON CONFLICT (slug) DO NOTHING;
