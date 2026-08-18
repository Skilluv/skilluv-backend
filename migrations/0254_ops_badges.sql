-- Ten ops distinctions.
--
-- ## The thresholds are lower, and that is the position
--
-- `code-craft-master` is thirty verified deliverables. A Terraform module
-- another team runs, an operator with its tests, a migration executed without
-- a long lock — each of those is weeks, and thirty is a career. The counts
-- below follow what the work actually takes rather than copying a number
-- from a domain where a deliverable can be an afternoon.
--
-- ## The one nobody else awards
--
-- `ops-quiet-year`. Ops is judged almost entirely on visible failure: the
-- outage, the page, the post-mortem. The person whose service never fell over
-- generated no artefacts to point at, and every recognition scheme in this
-- trade misses them. It rests on objectives met over a year, which is the
-- closest a schema can get to "nothing happened, on purpose".
--
-- ## The one a human decides
--
-- `ops-incident-veteran`. Ten incidents commanded is a real thing to
-- recognise, and the engine counts attestations rather than incidents — so a
-- rule counting attestations would award it to somebody with ten published
-- post-mortems from three incidents. It says a human decides.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('ops-first-artifact', 'medal',
 'Premier artefact ops',
 'Un premier livrable ops vérifié. Le moment où le profil cesse d''être déclaratif.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "ops", "min_count": 1}', 'common'),

('ops-craft-master', 'medal',
 'Maître d''œuvre ops',
 'Quinze livrables ops vérifiés. La régularité, pas le coup d''éclat.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "ops", "min_count": 15}', 'epic'),

('ops-craft-legend', 'medal',
 'Légende de la salle machine',
 'Cinquante livrables ops vérifiés.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "ops", "min_count": 50}', 'legendary'),

('ops-infra-shipped', 'medal',
 'Infrastructure livrée',
 'Un module, un chart ou un pipeline qu''une autre équipe fait tourner sans son auteur.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ops_infra_shipped", "min_count": 1}', 'rare'),

('ops-uptime-hero', 'medal',
 'Objectif tenu',
 'Un objectif de service annoncé, mesuré et tenu sur sa fenêtre entière.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ops_uptime_achievement", "min_count": 1}', 'rare'),

('ops-cost-cutter', 'medal',
 'Facture allégée',
 'Une réduction de coûts documentée, avec la preuve que le service tient toujours.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ops_cost_optimization", "min_count": 1}', 'rare'),

('ops-migration-master', 'medal',
 'Migration menée',
 'Une migration majeure — base, nuage, orchestrateur — menée à son terme.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ops_migration_completed", "min_count": 1}', 'epic'),

('ops-observability-hero', 'medal',
 'Pile d''observabilité livrée',
 'Une pile d''observabilité livrée et adoptée par ceux qui exploitent le système.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ops_observability_stack_shipped", "min_count": 1}', 'rare'),

('ops-quiet-year', 'medal',
 'L''année tranquille',
 'Trois objectifs de service tenus. La distinction que ce métier mérite et que personne ne décerne : rien ne s''est passé, et c''était voulu.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ops_uptime_achievement", "min_count": 3}', 'epic'),

('ops-featured', 'medal',
 'Mis en avant',
 'Un travail ops retenu par la communauté pour son exemplarité.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "featured_ops_engineer", "min_count": 1}', 'rare')

ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- The one a human decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('ops-incident-veteran', 'medal',
 'Vétéran des incidents',
 'Dix incidents conduits, chacun avec son post-mortem publié et ses actions suivies.',
 -- The engine counts attestations, and one incident can produce several. A
 -- rule counting them would award this to somebody with ten post-mortems
 -- from three incidents, which is the opposite of what it names.
 '{"manual": true}', 'legendary')

ON CONFLICT (slug) DO NOTHING;
