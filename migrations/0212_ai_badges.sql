-- Twelve AI distinctions.
--
-- ## Eleven are counted, one is not
--
-- Migration 0177 had to mark six code badges manual because nothing in the
-- schema could count what they described. Half of that gap closed with
-- `attestations.basis` in 0178: an attestation that rests on a published
-- model is a row with a value in it, so the rules below say so instead of
-- asking an operator to decide.
--
-- The exception is `ai-mission-veteran`. Paid AI missions have no table yet,
-- so the honest options were to invent a rule that counts something else or
-- to say a human decides. It says a human decides.
--
-- ## Why the thresholds are lower than the code ones
--
-- `code-craft-master` is thirty verified deliverables. A model shipped to
-- production, a dataset published with a card, a benchmark somebody else can
-- re-run — these cost weeks each, and thirty of them is a career rather than
-- a badge. The counts here follow what the work actually takes.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('ai-first-artifact', 'medal',
 'Premier artefact IA',
 'Un premier livrable IA vérifié. Le moment où le profil cesse d''être déclaratif.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "ai", "min_count": 1}', 'common'),

('ai-craft-master', 'medal',
 'Maître d''œuvre IA',
 'Vingt livrables IA vérifiés. La régularité, pas le coup d''éclat.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "ai", "min_count": 20}', 'epic'),

('ai-craft-legend', 'medal',
 'Légende de l''atelier IA',
 'Soixante livrables IA vérifiés.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "ai", "min_count": 60}', 'legendary'),

('ai-model-shipped', 'medal',
 'Modèle en production',
 'Un modèle mis en service, avec une adresse où il répond.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ai_model_shipped", "min_count": 1}', 'rare'),

('ai-dataset-published', 'medal',
 'Jeu de données publié',
 'Un jeu de données publié avec sa fiche : provenance, licence, limites.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ai_dataset_published", "min_count": 1}', 'rare'),

('ai-agent-builder', 'medal',
 'Constructeur d''agents',
 'Un système d''agents déployé, avec ses évaluations et ses garde-fous.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ai_agent_system_deployed", "min_count": 1}', 'rare'),

('ai-benchmark-crushed', 'medal',
 'Référence battue',
 'Un résultat de banc public qu''un tiers a rejoué et retrouvé.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ai_benchmark_result", "min_count": 1}', 'epic'),

('ai-safety-contributor', 'medal',
 'Contributeur sûreté',
 'Une trouvaille de sûreté validée, divulguée dans les règles.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ai_safety_finding_validated", "min_count": 1}', 'epic'),

('ai-paper-author', 'medal',
 'Auteur publié',
 'Un article publié — préprint ou conférence — avec le code qui le soutient.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "ai_paper_published", "min_count": 1}', 'legendary'),

('ai-multi-modal', 'medal',
 'Polyvalent IA',
 'Du travail vérifié dans trois orientations IA différentes.',
 '{"distinct_over": "orientation", "skill_domain": "ai", "min_count": 3}', 'epic'),

('ai-oss-model-contributor', 'medal',
 'Contributeur IA open source',
 'Une contribution IA acceptée en amont, dans un dépôt qu''on ne contrôle pas.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "ai", "min_count": 1}', 'rare'),

('ai-featured', 'medal',
 'Mis en avant',
 'Un travail IA retenu par la rédaction pour son exemplarité.',
 '{"proof_types": ["deliverable_featured"], "skill_domain": "ai", "min_count": 1}', 'rare');

-- ═══════════════════════════════════════════════════════════════════
-- The one a human decides
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('ai-mission-veteran', 'medal',
 'Vétéran des missions IA',
 'Dix missions IA rémunérées menées à terme.',
 -- Manual until paid missions exist as rows. A rule counting something else
 -- would award this to people who never did the thing it names.
 '{"manual": true}', 'legendary');
