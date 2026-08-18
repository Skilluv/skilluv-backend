-- Forty-one challenges, one set per AI trade.
--
-- ## The constraint that would have refused every one of them
--
-- `challenge_templates` still carried the CHECK written in migration 0003,
-- back when the platform had four domains. `orientations` was widened long
-- ago (0088 lists 'ai' and 'ops'); this table was not, so a fresh database
-- stopped here. A CHECK cannot be extended, only replaced, so every value is
-- restated — dropping one silently is how two tournament kinds disappeared in
-- migration 0223.
--
-- ## Why they are drafts
--
-- The title and the intent come from the backlog; the full brief — the
-- dataset, the numbers to beat, what is out of scope — needs an author who
-- knows the trade. A challenge nobody has reviewed must not be offered to
-- somebody learning, and `draft` is the state the workflow already has.
--
-- Seeding them anyway is the point: ten trades with an empty catalogue are
-- ten trades the platform claims to support and cannot.
--
-- ## Why the instructions are built here rather than written out
--
-- Migration 0185 wrote all hundred and thirty-eight briefs in full, and every
-- one repeats the same three headings and the same closing paragraph. Writing
-- them again by hand would be a hundred chances to let one drift. The
-- variable part — what to do, and what artefact comes out — is what the rows
-- carry.
--
-- ## The paragraph every AI brief ends on
--
-- Measured on data the model has not seen, and obtainable again. Those two
-- sentences are the difference between a result and a claim, and they are the
-- two most common reasons an AI submission comes back.

ALTER TABLE challenge_templates
    DROP CONSTRAINT IF EXISTS challenges_skill_domain_check;

ALTER TABLE challenge_templates
    DROP CONSTRAINT IF EXISTS challenge_templates_skill_domain_check;

ALTER TABLE challenge_templates
    ADD CONSTRAINT challenge_templates_skill_domain_check
    CHECK (skill_domain IN (
        -- Migration 0003, the original four.
        'code', 'design', 'game', 'security',
        -- Migration 0088 already listed these on `orientations`.
        'soft_skills', 'ai', 'ops'
    ));

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## Ce qu''il y a à faire' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Ce qui est attendu' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'Dans tous les cas : les chiffres annoncés sont mesurés sur des données ' ||
    'que le modèle n''a pas vues, et un lecteur peut les obtenir à nouveau — ' ||
    'graines, versions et données figées. Ce sur quoi le travail échoue est ' ||
    'écrit par son auteur. Un travail sans documentation est refusé.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de revue de la famille s''applique, et elle est publique : ' ||
    'tu peux la lire avant de soumettre.',
    'ai', c.difficulty, c.language,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ai' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'ai' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── data-engineer (5) ──────────────────────────────────────────────
('data-engineer', 'Pipeline batch de bout en bout',
 'Orchestrer un ETL quotidien avec Airflow, Prefect ou Dagster, avec reprise sur échec et rattrapage d''historique',
 'Un dépôt avec les DAG, les tests, et une exécution de rattrapage démontrée sur au moins sept jours.', 3, 'python'),

('data-engineer', 'Pipeline de flux temps réel',
 'Ingérer un flux Kafka ou Redpanda et énoncer la sémantique de livraison réellement obtenue',
 'Un dépôt et une démonstration de ce qui se passe quand un consommateur redémarre au mauvais moment.', 4, 'python'),

('data-engineer', 'Migration d''entrepôt',
 'Migrer un jeu de tables d''un entrepôt vers un autre sans perte ni doublon, avec réconciliation',
 'Le code de migration et le rapport de réconciliation ligne à ligne entre source et cible.', 4, 'python'),

('data-engineer', 'Contrôles de qualité bloquants',
 'Poser des contrôles de qualité (Great Expectations, tests dbt) qui arrêtent le pipeline au lieu de casser le tableau de bord',
 'Un dépôt, les contrôles, et un cas où le pipeline s''arrête volontairement.', 3, 'python'),

('data-engineer', 'Magasin de caractéristiques',
 'Servir les mêmes caractéristiques à l''entraînement et à l''inférence, avec des jointures à date',
 'Un dépôt et la démonstration qu''une fuite temporelle est impossible par construction.', 4, 'python'),

-- ── data-analyst (4) ───────────────────────────────────────────────
('data-analyst', 'Tableau de bord de six indicateurs',
 'Construire un tableau de bord de six indicateurs métier, chacun avec sa définition écrite',
 'Un tableau de bord public ou une capture reproductible, et les requêtes qui le nourrissent.', 2, 'sql'),

('data-analyst', 'Analyse de cohortes',
 'Mener une analyse de rétention par cohortes et en tirer une recommandation',
 'Un rapport avec les requêtes, les courbes et ce qu''il faudrait faire.', 3, 'sql'),

('data-analyst', 'Analyse d''un test A/B',
 'Analyser un test A/B avec puissance, significativité et taille d''effet, et conclure honnêtement',
 'Un rapport qui dit aussi ce que le test ne permet pas de conclure.', 3, 'sql'),

('data-analyst', 'Rapport narratif',
 'Écrire un rapport qui va de la question à la recommandation, graphiques à l''appui',
 'Un rapport publié, lisible par quelqu''un qui ne connaît pas les données.', 2, 'sql'),

-- ── ml-engineer (5) ────────────────────────────────────────────────
('ml-engineer', 'Modèle de classification en production',
 'Entraîner un modèle de classification et le servir derrière une API, avec sa référence de comparaison',
 'Un dépôt, un modèle publié, et une adresse où il répond.', 3, 'python'),

('ml-engineer', 'Système de recommandation',
 'Construire un système de recommandation évalué hors ligne, et dire comment il serait évalué en ligne',
 'Un dépôt, le rapport d''évaluation, et la référence naïve qu''il faut battre.', 4, 'python'),

('ml-engineer', 'Prévision de série temporelle',
 'Prévoir une série temporelle avec une validation glissante et un horizon assumé',
 'Un dépôt et un rapport comparant le modèle à une prévision naïve.', 3, 'python'),

('ml-engineer', 'Affinage d''un modèle pré-entraîné',
 'Affiner un modèle pré-entraîné sur une tâche cible et mesurer ce que l''affinage a coûté ailleurs',
 'Un modèle publié avec sa fiche, et le rapport avant/après.', 4, 'python'),

('ml-engineer', 'Déploiement fantôme',
 'Déployer un modèle en parallèle de la production sans l''exposer, et comparer sur trafic réel',
 'Le code de déploiement et le rapport de comparaison sur au moins une semaine.', 4, 'python'),

-- ── prompt-engineer (4) ────────────────────────────────────────────
('prompt-engineer', 'RAG sur un corpus documentaire',
 'Construire une recherche augmentée sur un corpus donné et l''évaluer sur des questions écrites à l''avance',
 'Un dépôt, le jeu d''évaluation, et les cas où le système répond mal.', 3, 'python'),

('prompt-engineer', 'Bibliothèque de vingt invites',
 'Calibrer vingt invites pour des tâches précises, versionnées et évaluées une par une',
 'Un dépôt avec les invites, leurs évaluations, et l''historique des modifications.', 2, 'python'),

('prompt-engineer', 'Agent conversationnel multi-tours',
 'Construire un agent qui garde un état sur plusieurs tours et sait passer la main à un humain',
 'Un dépôt, une démonstration, et la règle d''escalade écrite.', 3, 'python'),

('prompt-engineer', 'Défenses contre l''injection d''invite',
 'Mettre en place des défenses contre l''injection d''invite et mesurer leur efficacité par l''attaque',
 'Un dépôt, le banc d''attaque, et le taux de réussite avant et après.', 4, 'python'),

-- ── llm-engineer (5) ───────────────────────────────────────────────
('llm-engineer', 'Affinage LoRA d''un modèle ouvert',
 'Affiner un modèle ouvert en LoRA sur une tâche cible et le publier sur HuggingFace',
 'Un modèle publié avec sa fiche, le code d''entraînement et le rapport d''évaluation.', 4, 'python'),

('llm-engineer', 'Recherche hybride avec reclassement',
 'Combiner BM25 et vecteurs, ajouter un reclasseur, et mesurer ce que chaque étage apporte',
 'Un dépôt et une ablation qui montre l''apport de chaque composant.', 4, 'python'),

('llm-engineer', 'Système multi-agents',
 'Coordonner plusieurs agents sur une tâche composée, avec une condition d''arrêt et un budget',
 'Un dépôt, une trace d''exécution complète, et le coût par exécution.', 5, 'python'),

('llm-engineer', 'Agent à outils sous bac à sable',
 'Donner des outils réels à un agent — API, base, exécution de code — sans lui donner les clés',
 'Un dépôt, le modèle de permissions, et ce que l''agent ne peut pas faire.', 5, 'python'),

('llm-engineer', 'Distillation vers un modèle plus petit',
 'Distiller un grand modèle vers un plus petit et mesurer précisément ce qui a été perdu',
 'Le modèle distillé publié, et le rapport de ce qui se dégrade.', 5, 'python'),

-- ── mlops-engineer (4) ─────────────────────────────────────────────
('mlops-engineer', 'Service de modèle à l''échelle',
 'Servir un modèle derrière KServe, Triton ou vLLM avec autoscaling, et mesurer la latence de queue',
 'Le manifeste de déploiement et un rapport de charge avec les percentiles hauts.', 4, 'python'),

('mlops-engineer', 'Surveillance et dérive',
 'Instrumenter un modèle en production pour détecter la dérive, avec un seuil qui déclenche une action',
 'La configuration de surveillance, et la démonstration d''une alerte sur dérive injectée.', 4, 'python'),

('mlops-engineer', 'Chaîne de réentraînement',
 'Automatiser le réentraînement et le redéploiement d''un modèle, tests compris',
 'Un dépôt, la chaîne qui s''exécute seule, et la procédure de retour arrière.', 4, 'python'),

('mlops-engineer', 'Coût d''inférence divisé par deux',
 'Réduire de moitié le coût d''inférence d''un modèle servi sans dégrader la qualité au-delà d''un seuil annoncé',
 'Le rapport avant/après, avec le coût, la latence et la qualité mesurés ensemble.', 4, 'python'),

-- ── computer-vision-engineer (4) ───────────────────────────────────
('computer-vision-engineer', 'Détection d''objets déployée',
 'Affiner un modèle de détection sur un jeu propre et le déployer, mAP à l''appui',
 'Le modèle publié, le jeu d''évaluation, et une adresse où il répond.', 4, 'python'),

('computer-vision-engineer', 'Segmentation sémantique',
 'Entraîner un modèle de segmentation sur une tâche précise et rapporter le mIoU par classe',
 'Le modèle publié et le rapport par classe, pas seulement la moyenne.', 4, 'python'),

('computer-vision-engineer', 'Reconnaissance faciale et biais',
 'Construire un système de reconnaissance faciale, mesurer sa performance par sous-population et énoncer ses garde-fous',
 'Un dépôt, le rapport de biais, et l''usage prévu écrit noir sur blanc.', 5, 'python'),

('computer-vision-engineer', 'Vision embarquée temps réel',
 'Faire tourner un modèle de vision en temps réel sur une carte embarquée, quantification comprise',
 'Le modèle optimisé, les images par seconde mesurées sur la carte, et ce que la quantification a coûté.', 5, 'python'),

-- ── nlp-engineer (4) ───────────────────────────────────────────────
('nlp-engineer', 'Reconnaissance et liaison d''entités',
 'Extraire des entités d''un corpus et les rattacher à une base de connaissances, ambiguïtés comprises',
 'Un dépôt, le jeu d''évaluation annoté, et le taux d''erreur par type d''entité.', 4, 'python'),

('nlp-engineer', 'Sentiment multilingue',
 'Analyser le sentiment sur au moins trois langues et rapporter la performance langue par langue',
 'Un dépôt, le rapport par langue, et l''écart assumé entre elles.', 3, 'python'),

('nlp-engineer', 'Résumé abstractif',
 'Affiner un modèle de résumé et mesurer la fidélité autant que le ROUGE',
 'Le modèle publié, et une évaluation manuelle des hallucinations sur un échantillon.', 4, 'python'),

('nlp-engineer', 'Traduction en langue peu dotée',
 'Construire une traduction pour une langue peu dotée — wolof, yoruba, bambara — et documenter la collecte des données',
 'Le modèle publié, le rapport d''évaluation, et la provenance des données avec les consentements.', 5, 'python'),

-- ── ai-safety-researcher (3) ───────────────────────────────────────
('ai-safety-researcher', 'Red-team d''un modèle ouvert',
 'Attaquer un modèle ouvert selon un protocole écrit, mesurer un taux de réussite et proposer une atténuation',
 'Un rapport reproductible, une divulgation faite dans les règles, et l''atténuation proposée.', 5, 'python'),

('ai-safety-researcher', 'Évaluation de biais',
 'Mesurer un biais sur un modèle ou un jeu de données avec un protocole qu''un tiers peut rejouer',
 'Un rapport avec le protocole, les écarts mesurés et les recommandations.', 4, 'python'),

('ai-safety-researcher', 'Reproduction d''une technique d''alignement',
 'Reproduire une technique d''alignement publiée et dire où elle tient et où elle ne tient pas',
 'Un dépôt, les expériences, et l''écart avec les résultats publiés.', 5, 'python'),

-- ── generative-ai-artist (3) ───────────────────────────────────────
('generative-ai-artist', 'Chaîne de diffusion dirigée',
 'Construire une chaîne de diffusion avec ControlNet et LoRA qui produit un résultat voulu, pas trouvé',
 'La chaîne, les paramètres, et trois résultats obtenus depuis la même intention.', 3, 'python'),

('generative-ai-artist', 'Graphe ComfyUI réutilisable',
 'Publier un graphe ComfyUI documenté que quelqu''un d''autre peut reprendre et modifier',
 'Le graphe, sa documentation, et le rendu de référence qu''il doit reproduire.', 3, 'python'),

('generative-ai-artist', 'Série cohérente de dix pièces',
 'Produire dix pièces qui tiennent ensemble, et écrire la direction qui les tient',
 'La série publiée, la note d''intention, et les droits sur les modèles utilisés.', 4, 'python')

) AS c(orientation_slug, title, description, expected, difficulty, language)
JOIN orientations o ON o.slug = c.orientation_slug;
