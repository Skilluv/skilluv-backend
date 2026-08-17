-- Paid AI work, on the mission table that already exists.
--
-- ## Why there is no `ai_missions`
--
-- The backlog asked for one. Migration 0192 built `missions` keyed by
-- `skill_domain`, with the application flow, the payment models, the IP terms
-- and the state machine already written. A second table would have meant a
-- second application flow, a second invoice path and two answers to "how many
-- missions has this person finished".
--
-- What AI needs is rows in `mission_types` and one more delivery format.
--
-- ## The IP clauses, and the one that is missing
--
-- The backlog listed four arrangements. Three of them are the ones 0192
-- already has, under different words:
--
--   * `full_transfer`               → `full_ownership_client`
--   * `model_open_source`           → `dual_license`
--   * `weights_client_code_creator` → `retain_reusable_components`, which is
--     exactly this shape: the client owns the domain-specific work — weights
--     trained on their data — and the creator keeps the training code they
--     would otherwise rewrite for the next client.
--
-- The fourth, a commercial licence granting use without ownership, has no
-- value here and does not get one. It is a licensing arrangement rather than
-- a split of ownership, it belongs in the contract, and adding an enum value
-- for something the platform cannot enforce would suggest that it can.

ALTER TABLE missions
    DROP CONSTRAINT IF EXISTS missions_deliverable_format_check;

ALTER TABLE missions
    ADD CONSTRAINT missions_deliverable_format_check
    CHECK (deliverable_format IN (
        -- Migration 0192
        'github_pr', 'repository_handover', 'library_published',
        'consulting_report',
        -- What an AI mission actually hands over. None of the four above
        -- describes a set of weights, and filing one as a repository handover
        -- would lose the thing the client is paying for.
        'model_weights',        -- weights and the card that makes them usable
        'dataset_delivered',    -- a dataset with its provenance documented
        'deployed_endpoint',    -- a running service with its API documentation
        'evaluation_report'     -- an audit or evaluation, with its protocol
    ));

COMMENT ON COLUMN missions.deliverable_format IS
    'What is handed over at the end. A set of weights is not a repository '
    'handover: the client is paying for the model, and the code that trained '
    'it may not even be theirs.';

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('ai_model_training', 'ai', 'Entraînement de modèle',
     'Un modèle entraîné sur les données du client, évalué honnêtement et livré avec sa fiche.', 10),
    ('ai_data_pipeline_build', 'ai', 'Construction de pipeline',
     'Faire arriver la donnée là où elle est utile, avec les contrôles qui arrêtent le pipeline quand la source ment.', 20),
    ('ai_rag_system', 'ai', 'Système de recherche augmentée',
     'Un RAG sur le corpus du client, avec le jeu d''évaluation qui dit quand il se trompe.', 30),
    ('ai_llm_fine_tune', 'ai', 'Affinage de modèle de langage',
     'Adapter un modèle ouvert à une tâche précise, licences amont respectées.', 40),
    ('ai_cv_application', 'ai', 'Application de vision',
     'Détection, segmentation ou lecture d''images, avec la performance mesurée sur le matériel visé.', 50),
    ('ai_mlops_consulting', 'ai', 'Conseil MLOps',
     'Servir, surveiller et redéployer. Le livrable est souvent un rapport et une chaîne qui tourne.', 60),
    ('ai_safety_audit', 'ai', 'Audit de sûreté',
     'Attaquer un système en place selon un protocole écrit, et proposer les atténuations.', 70),
    ('ai_ethics_review', 'ai', 'Revue éthique et conformité',
     'Biais, provenance des données, usage prévu. Ce qu''il faut avoir écrit avant de mettre en service.', 80);

-- ═══════════════════════════════════════════════════════════════════
-- The badge stops being a judgement
-- ═══════════════════════════════════════════════════════════════════
--
-- Migration 0212 marked `ai-mission-veteran` manual and said why: paid
-- missions had no table, and a rule counting something else would have
-- awarded it to people who never did the thing it names. They have one now.
--
-- Manual grants already made stay. Somebody looked at the work and decided,
-- and the engine has no business taking that back because it can now count.

UPDATE badge_rules
   SET conditions = '{"proof_types": ["mission_completed"], "skill_domain": "ai", "min_count": 10}',
       updated_at = NOW()
 WHERE slug = 'ai-mission-veteran';
