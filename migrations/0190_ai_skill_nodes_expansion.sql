-- The skills the AI trades are made of.
--
-- ## What the catalogue held before this
--
-- Twenty-three nodes, all of them about *using* a language model: prompting,
-- integration, working alongside an assistant. That is one trade out of ten.
-- Nothing described a pipeline, a training loop, a serving stack, a detection
-- model or a red-team protocol — so six of the trades named in migration 0189
-- would have had nothing to attach to, and two of the four that already
-- existed had an empty skill map since 0088.
--
-- ## Naming
--
-- Each node names a technique or a tool, never a level. "Advanced ML" is a
-- label nobody can claim honestly; "détection de dérive (Evidently)" is
-- something a person has either done or not.
--
-- ## Where the tree deliberately stays shallow
--
-- Two levels, like the rest of the catalogue. A third would let us describe
-- `qlora` under `lora-fine-tuning` under `ai-integration`, and nobody claims
-- a skill at that depth — they claim the technique.

-- ═══════════════════════════════════════════════════════════════════
-- Roots
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('data-engineering',  'Ingénierie des données',      'Orchestration, flux, entrepôts. Faire arriver la donnée telle qu''elle est partie.', 'ai'),
('data-analysis',     'Analyse de données',           'SQL analytique, cohortes, tests. Transformer une table en décision.', 'ai'),
('machine-learning',  'Apprentissage automatique',    'Entraînement, évaluation, généralisation. Ce qui distingue un modèle d''une courbe apprise par cœur.', 'ai'),
('ml-operations',     'Exploitation des modèles',     'Servir, surveiller, redéployer. Un modèle en production vieillit sans prévenir.', 'ai'),
('computer-vision',   'Vision par ordinateur',        'Détection, segmentation, inférence embarquée. Et ce que le jeu de données ne montre pas.', 'ai'),
('natural-language-processing', 'Traitement du langage', 'Segmentation, entités, traduction, résumé. Le langage comme structure.', 'ai'),
('ai-safety',         'Sûreté des IA',                'Red-teaming, biais, alignement. Chercher activement l''échec plutôt que l''attendre.', 'ai'),
('generative-media',  'Média génératif',              'Diffusion, conditionnement, séries cohérentes. Une direction tenue, pas une image réussie.', 'ai');

-- ═══════════════════════════════════════════════════════════════════
-- Children
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT c.slug, c.display_name, c.description, 'ai', p.id
FROM (VALUES

-- ── Ingénierie des données ─────────────────────────────────────────
('batch-orchestration',      'Orchestration batch',            'Airflow, Prefect, Dagster. Dépendances, reprises, rattrapage d''historique.', 'data-engineering'),
('streaming-ingestion',      'Ingestion en flux',              'Kafka, Redpanda. Partitions, décalages, sémantique de livraison.', 'data-engineering'),
('warehouse-modelling',      'Modélisation d''entrepôt',        'dbt, schéma en étoile, modèles incrémentaux.', 'data-engineering'),
('data-quality-testing',     'Tests de qualité des données',   'Great Expectations, contrats de données, tests dbt. Détecter la rupture en amont du tableau de bord.', 'data-engineering'),
('lakehouse-formats',        'Formats de lac de données',      'Parquet, Iceberg, Delta. Partitionnement, compaction, évolution de schéma.', 'data-engineering'),
('feature-store',            'Magasin de caractéristiques',    'Feast. Cohérence entre apprentissage et service, jointures à date.', 'data-engineering'),
('reverse-etl',              'Reverse ETL',                    'Renvoyer l''entrepôt vers les outils métier sans en faire une seconde source de vérité.', 'data-engineering'),
('pipeline-cost-control',    'Maîtrise du coût des pipelines', 'Octets scannés, fréquence, rétention. Une requête planifiée coûte tous les jours.', 'data-engineering'),

-- ── Analyse de données ─────────────────────────────────────────────
-- No SQL node here on purpose: `sql`, `sql-cte`, `sql-window-functions` and
-- `postgres-indexing` already exist under `code`, and the map below points at
-- them. A second SQL node under `ai` would give two answers to whether
-- somebody knows window functions.
('dashboard-design',         'Conception de tableaux de bord', 'Metabase, Superset, Looker. Une métrique par question, pas trente par écran.', 'data-analysis'),
('cohort-retention-analysis','Analyse de cohortes',            'Rétention, courbes de survie, biais de survivant.', 'data-analysis'),
('ab-test-analysis',         'Analyse de tests A/B',           'Puissance, significativité, taille d''effet. Arrêter avant la fin invalide le test.', 'data-analysis'),
('metric-definition',        'Définition de métriques',        'Écrire une définition que deux équipes calculent pareil.', 'data-analysis'),
('data-storytelling',        'Récit par les données',          'Ce qu''on a trouvé, ce qu''on en déduit, ce qu''il faut faire. Dans cet ordre.', 'data-analysis'),

-- ── Apprentissage automatique ──────────────────────────────────────
('supervised-modelling',     'Modélisation supervisée',        'Régression, arbres, gradient boosting sur données tabulaires.', 'machine-learning'),
('model-evaluation',         'Évaluation de modèles',          'Validation croisée, fuite de données, choix de métrique. La fuite est l''erreur la plus fréquente et la moins visible.', 'machine-learning'),
('feature-engineering',      'Ingénierie des caractéristiques','Encodage, normalisation, variables temporelles.', 'machine-learning'),
('deep-learning-training',   'Entraînement profond',           'PyTorch : boucle, optimiseurs, régularisation, points de reprise.', 'machine-learning'),
('transfer-learning',        'Apprentissage par transfert',    'Repartir d''un modèle pré-entraîné plutôt que du bruit.', 'machine-learning'),
('recommender-systems',      'Systèmes de recommandation',     'Filtrage collaboratif, démarrage à froid, évaluation hors ligne et en ligne.', 'machine-learning'),
('time-series-forecasting',  'Prévision de séries temporelles','Saisonnalité, validation glissante, horizon.', 'machine-learning'),
('experiment-tracking',      'Suivi d''expériences',            'MLflow, Weights & Biases. Une expérience qu''on ne peut pas retrouver n''a pas eu lieu.', 'machine-learning'),
('training-reproducibility', 'Reproductibilité d''entraînement','Graines, versions, données figées. Même entrée, même sortie, sur une autre machine.', 'machine-learning'),

-- ── Exploitation des modèles ───────────────────────────────────────
('model-serving',            'Service de modèles',             'KServe, Triton, vLLM. Lots, concurrence, latence de queue.', 'ml-operations'),
('drift-detection',          'Détection de dérive',            'Evidently. Dérive des entrées et des performances, et le seuil qui déclenche quelque chose.', 'ml-operations'),
('ml-ci-cd',                 'CI/CD pour modèles',             'Kubeflow, ZenML, Metaflow. Réentraîner sans intervention manuelle.', 'ml-operations'),
('model-registry-versioning','Registre et versions de modèles','Quel poids est en production, depuis quand, et comment revenir en arrière.', 'ml-operations'),
('inference-cost-optimization','Optimisation du coût d''inférence','Quantification, mise en lots, cache. Diviser la facture sans changer la réponse.', 'ml-operations'),
('gpu-capacity-planning',    'Dimensionnement GPU',            'Mémoire, débit, files d''attente. Ce qu''on peut réellement servir avec la carte qu''on a.', 'ml-operations'),

-- ── Vision par ordinateur ──────────────────────────────────────────
('object-detection',         'Détection d''objets',             'YOLO, DETR. mAP, seuils de confiance, ancrage.', 'computer-vision'),
('image-segmentation',       'Segmentation d''images',          'Masques, mIoU, segmentation sémantique et d''instances.', 'computer-vision'),
('vision-dataset-curation',  'Curation de jeux d''images',      'Annotation, augmentation, équilibrage. Le jeu de données décide plus que l''architecture.', 'computer-vision'),
('edge-vision-optimization', 'Vision embarquée',               'ONNX, TensorRT, quantification. Tenir sur une Jetson ou un Raspberry Pi.', 'computer-vision'),
('vision-bias-evaluation',   'Évaluation des biais visuels',   'Performance par sous-population. Un modèle de visages qui n''a pas été testé par teint n''a pas été testé.', 'computer-vision'),
('video-tracking',           'Suivi vidéo',                    'Association d''identités entre trames, occlusions, ré-identification.', 'computer-vision'),

-- ── Traitement du langage ──────────────────────────────────────────
('text-tokenization',        'Tokenisation',                   'BPE, SentencePiece. Ce que le modèle voit réellement.', 'natural-language-processing'),
('named-entity-recognition', 'Reconnaissance d''entités',       'spaCy, modèles affinés. Frontières d''entités et types imbriqués.', 'natural-language-processing'),
('entity-linking',           'Liaison d''entités',              'Rattacher une mention à une base de connaissances, et gérer l''ambiguïté.', 'natural-language-processing'),
('text-classification',      'Classification de texte',        'Sentiment, intention, thématique. Jeu de test qui ressemble à la production.', 'natural-language-processing'),
('machine-translation-eval', 'Évaluation de traduction',       'BLEU, COMET, et pourquoi un score seul ne suffit pas.', 'natural-language-processing'),
('summarization-eval',       'Évaluation de résumé',           'ROUGE, fidélité, hallucination. Un résumé faux est pire qu''un texte long.', 'natural-language-processing'),
('low-resource-nlp',         'TAL en langues peu dotées',      'Wolof, yoruba, bambara. Peu de données annotées, transfert multilingue, collecte éthique.', 'natural-language-processing'),

-- ── Sûreté des IA ──────────────────────────────────────────────────
('llm-red-teaming',          'Red-teaming de modèles',         'Protocole d''attaque, reproductibilité, gravité. Une trouvaille non reproductible n''est pas une trouvaille.', 'ai-safety'),
('jailbreak-taxonomy',       'Taxonomie des contournements',   'Injection, encodage, jeu de rôle, attaques multi-tours. Nommer pour pouvoir mesurer.', 'ai-safety'),
('bias-measurement',         'Mesure des biais',               'Protocole, sous-populations, écart mesuré. Distinguer un biais d''un échantillon trop petit.', 'ai-safety'),
('alignment-techniques',     'Techniques d''alignement',        'RLHF, DPO, IA constitutionnelle. Ce que chacune optimise réellement.', 'ai-safety'),
('eval-harness-design',      'Conception de bancs d''évaluation','HELM, lm-eval-harness. Un banc qu''un tiers peut relancer.', 'ai-safety'),
('responsible-disclosure-ai','Divulgation responsable',        'Prévenir l''éditeur, convenir d''un délai, publier. Dans cet ordre.', 'ai-safety'),
('dual-use-assessment',      'Évaluation du double usage',     'Décider ce qui se publie et ce qui se retient, et écrire pourquoi.', 'ai-safety'),

-- ── Média génératif ────────────────────────────────────────────────
('diffusion-pipelines',      'Chaînes de diffusion',           'Stable Diffusion, échantillonneurs, graines. Reproduire une image délibérément.', 'generative-media'),
('controlnet-conditioning',  'Conditionnement ControlNet',     'Pose, profondeur, contours. Diriger au lieu de relancer.', 'generative-media'),
('lora-training-visual',     'Entraînement de LoRA visuels',   'Constituer un jeu, entraîner un style, éviter le sur-apprentissage.', 'generative-media'),
('comfyui-workflows',        'Graphes ComfyUI',                'Un graphe réutilisable et documenté, pas une capture d''écran de nœuds.', 'generative-media'),
('generative-series-consistency','Cohérence de série',         'Dix pièces qui tiennent ensemble. C''est là que la direction artistique se voit.', 'generative-media'),
('generative-provenance',    'Provenance et droits',           'C2PA, licences des modèles, données d''entraînement. Ce qu''on a le droit de publier.', 'generative-media'),

-- ── Sous les racines existantes ────────────────────────────────────
-- `ai-integration` and `prompt-engineering` were seeded in 0057 and cover the
-- basics. What the LLM engineer does beyond them belongs under the same roots
-- rather than in a duplicate tree.
('hybrid-retrieval',         'Recherche hybride',              'BM25 et vecteurs combinés, puis reclassement. Presque toujours meilleur que le dense seul.', 'ai-integration'),
('query-decomposition',      'Décomposition de requêtes',      'Découper une question en sous-questions récupérables.', 'ai-integration'),
('lora-fine-tuning',         'Affinage LoRA',                  'PEFT, QLoRA. Adapter un modèle ouvert sans réentraîner ses milliards de paramètres.', 'ai-integration'),
('multi-agent-orchestration','Orchestration multi-agents',     'LangGraph, AutoGen. État partagé, terminaison, coût.', 'ai-integration'),
('model-distillation',       'Distillation de modèles',        'Transférer vers un modèle plus petit en mesurant ce qu''on perd.', 'ai-integration'),
('agent-tool-safety',        'Sûreté des outils d''agent',      'Bac à sable, permissions, confirmation. Un agent qui exécute du code exécute du code.', 'ai-integration'),
('prompt-injection-defense', 'Défense contre l''injection',     'Séparer instruction et donnée, filtrer, évaluer l''attaque.', 'prompt-engineering'),
('prompt-library-versioning','Versionnage d''invites',          'Une invite est du code : versionnée, testée, avec un historique.', 'prompt-engineering')

) AS c(slug, display_name, description, parent_slug)
JOIN skill_nodes p ON p.slug = c.parent_slug;
