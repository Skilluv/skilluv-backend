-- The AI toolkit, as rows rather than a page.
--
-- ## Why not a markdown page
--
-- The backlog asked for a published toolkit document. Migration 0188 already
-- answered that question for language ecosystems and the reasoning is the
-- same: a page is written once and rots quietly, cannot be filtered by what
-- somebody is learning, and cannot be joined against anything. An operator
-- edits rows in the admin panel; nobody redeploys to fix a dead link.
--
-- ## Why this is not a `content_guides` toolkit page
--
-- Migration 0199 has a `toolkit` kind, and the code domain uses it: one
-- markdown page describing the landscape. That is the right shape for a page
-- somebody reads once.
--
-- This is a catalogue, not a page. Each row carries what it costs to reach
-- the thing and which trades it serves, and both are filters — `GET
-- /api/ai/toolkit?orientation=nlp-engineer` answers a question a document
-- cannot. If the two ever merge, this is the shape that survives, because a
-- page can be generated from rows and rows cannot be extracted from a page.
--
-- ## Why one table for tools, communities and courses
--
-- They are the same row: something outside Skilluv, with an address, a
-- category, and a sentence saying why it is worth the reader's time. Three
-- tables would mean three endpoints and three admin screens for one question
-- — "where do I go next".
--
-- ## `access_note`
--
-- The column that makes this useful to the audience Skilluv is for. Most
-- lists of AI tooling assume a credit card and a fast connection. Whether a
-- thing has a free tier, whether it needs a GPU, whether the course is
-- audit-free — that is what decides if a person in Cotonou can actually use
-- it, and no upstream list writes it down.

CREATE TABLE external_ai_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE
        CHECK (slug ~ '^[a-z0-9-]+$'),
    display_name VARCHAR(80) NOT NULL,
    category VARCHAR(20) NOT NULL
        CHECK (category IN (
            'framework',    -- PyTorch, JAX, Candle
            'llm_tooling',  -- vLLM, LangChain, DSPy
            'mlops',        -- MLflow, Evidently, ZenML
            'data_stack',   -- dbt, Airflow, DuckDB
            'compute',      -- Colab, RunPod, cloud
            'safety',       -- HELM, lm-eval-harness, Detoxify
            'hub',          -- HuggingFace, Kaggle, Papers with Code
            'community',    -- forums, Discords, paper clubs
            'learning'      -- courses and books
        )),
    url TEXT NOT NULL CHECK (url ~ '^https://'),
    -- Said plainly, in the platform's own words: what it is good at and who
    -- it suits. Not marketing copy lifted from the project's own site.
    summary TEXT NOT NULL CHECK (length(btrim(summary)) > 0),
    -- What it costs to actually reach it: free tier, GPU required, audit
    -- without paying. The half of the answer no upstream list writes down.
    access_note TEXT NOT NULL DEFAULT '',
    -- Which trades it serves. Empty means "everyone in the domain".
    orientation_slugs TEXT[] NOT NULL DEFAULT '{}',
    is_curated BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE external_ai_resources IS
    'Curated tooling, communities and courses for the AI trades. Curated '
    'because the value is in the selection: a link dump is what somebody '
    'already failed to navigate before arriving here.';

COMMENT ON COLUMN external_ai_resources.access_note IS
    'What it takes to actually reach this — free tier, GPU needed, course '
    'auditable without paying. Most upstream lists assume a card and a fast '
    'connection, and that assumption is the barrier.';

CREATE INDEX idx_external_ai_resources_category
    ON external_ai_resources (category, sort_order)
    WHERE is_curated = TRUE;

CREATE INDEX idx_external_ai_resources_orientations
    ON external_ai_resources USING gin (orientation_slugs);

CREATE OR REPLACE FUNCTION touch_external_ai_resources_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_external_ai_resources_updated_at
    BEFORE UPDATE ON external_ai_resources
    FOR EACH ROW EXECUTE FUNCTION touch_external_ai_resources_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The initial curation
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO external_ai_resources
    (slug, display_name, category, url, summary, access_note, orientation_slugs, sort_order)
VALUES

-- ── Frameworks ─────────────────────────────────────────────────────
('pytorch', 'PyTorch', 'framework', 'https://pytorch.org',
 'Le cadre par défaut de la recherche et, désormais, de la production. Presque tout ce qui se publie est en PyTorch.',
 'Libre. Fonctionne sur processeur, lentement mais réellement : on peut apprendre sans GPU.',
 ARRAY['ml-engineer','computer-vision-engineer','nlp-engineer','llm-engineer'], 10),

('jax', 'JAX', 'framework', 'https://jax.readthedocs.io',
 'Différentiation et compilation. Plus exigeant que PyTorch, et plus rapide quand le calcul est le goulot.',
 'Libre. Le vrai gain suppose des TPU ou des GPU récents.',
 ARRAY['ml-engineer'], 20),

('candle', 'Candle', 'framework', 'https://github.com/huggingface/candle',
 'Inférence en Rust, sans Python à l''exécution. Utile pour servir un modèle dans un binaire.',
 'Libre. Écosystème jeune : moins de modèles portés qu''en PyTorch.',
 ARRAY['mlops-engineer','llm-engineer'], 30),

('scikit-learn', 'scikit-learn', 'framework', 'https://scikit-learn.org',
 'Le tabulaire, qui reste la majorité du travail réel. Battre une régression logistique bien réglée est plus dur qu''il n''y paraît.',
 'Libre, tourne sur n''importe quelle machine.',
 ARRAY['ml-engineer','data-analyst'], 40),

-- ── Outillage LLM ──────────────────────────────────────────────────
('vllm', 'vLLM', 'llm_tooling', 'https://docs.vllm.ai',
 'Servir un modèle de langage avec un débit sérieux. La référence pour l''inférence en lot.',
 'Libre. Nécessite un GPU avec assez de mémoire pour le modèle visé.',
 ARRAY['mlops-engineer','llm-engineer'], 50),

('llama-cpp', 'llama.cpp', 'llm_tooling', 'https://github.com/ggml-org/llama.cpp',
 'Faire tourner un modèle quantifié sur un ordinateur ordinaire. La porte d''entrée quand il n''y a pas de GPU.',
 'Libre. Un modèle 7B quantifié tient sur 8 Go de RAM.',
 ARRAY['llm-engineer','prompt-engineer'], 60),

('dspy', 'DSPy', 'llm_tooling', 'https://dspy.ai',
 'Optimiser des invites par mesure plutôt que par intuition. Change la manière de travailler, pas seulement l''outil.',
 'Libre. Le coût est celui des appels au modèle pendant l''optimisation.',
 ARRAY['prompt-engineer','llm-engineer'], 70),

('langgraph', 'LangGraph', 'llm_tooling', 'https://langchain-ai.github.io/langgraph',
 'Orchestrer des agents comme un graphe d''états. Rend visible la terminaison, que les chaînes cachent.',
 'Libre.',
 ARRAY['llm-engineer'], 80),

-- ── MLOps ──────────────────────────────────────────────────────────
('mlflow', 'MLflow', 'mlops', 'https://mlflow.org',
 'Suivi d''expériences et registre de modèles. Le minimum pour retrouver ce qu''on a lancé la semaine dernière.',
 'Libre, s''auto-héberge sur une petite machine.',
 ARRAY['ml-engineer','mlops-engineer'], 90),

('evidently', 'Evidently', 'mlops', 'https://www.evidentlyai.com',
 'Détection de dérive et rapports de qualité. Répond à « le modèle a-t-il vieilli » sans écrire le calcul soi-même.',
 'Libre en bibliothèque.',
 ARRAY['mlops-engineer'], 100),

('weights-and-biases', 'Weights & Biases', 'mlops', 'https://wandb.ai',
 'Suivi d''expériences hébergé, avec des rapports partageables — utile quand la preuve doit être montrable.',
 'Offre gratuite pour les projets personnels et académiques.',
 ARRAY['ml-engineer','computer-vision-engineer'], 110),

-- ── Données ────────────────────────────────────────────────────────
('dbt', 'dbt', 'data_stack', 'https://www.getdbt.com',
 'Transformations versionnées et testées dans l''entrepôt. Le standard de fait de la modélisation analytique.',
 'Cœur libre. La version hébergée est payante au-delà d''un seul développeur.',
 ARRAY['data-engineer','data-analyst'], 120),

('dagster', 'Dagster', 'data_stack', 'https://dagster.io',
 'Orchestration pensée par actifs plutôt que par tâches. Plus facile à raisonner qu''Airflow sur un pipeline neuf.',
 'Libre en auto-hébergement.',
 ARRAY['data-engineer'], 130),

('duckdb', 'DuckDB', 'data_stack', 'https://duckdb.org',
 'Un entrepôt analytique dans un fichier. Permet d''apprendre le métier sans louer de Snowflake.',
 'Libre, tourne sur un portable. Le meilleur rapport apprentissage/coût de la liste.',
 ARRAY['data-engineer','data-analyst'], 140),

-- ── Calcul ─────────────────────────────────────────────────────────
('google-colab', 'Google Colab', 'compute', 'https://colab.research.google.com',
 'Un GPU gratuit dans un navigateur. C''est là que commence la plupart des parcours sans matériel.',
 'Gratuit avec des sessions interrompues et un GPU non garanti. Suffisant pour apprendre, pas pour entraîner longtemps.',
 ARRAY['ml-engineer','computer-vision-engineer','nlp-engineer'], 150),

('kaggle-notebooks', 'Kaggle Notebooks', 'compute', 'https://www.kaggle.com/code',
 'Trente heures de GPU par semaine, gratuites, avec les jeux de données à côté.',
 'Gratuit avec un compte vérifié par téléphone.',
 ARRAY['ml-engineer','data-analyst'], 160),

('runpod', 'RunPod', 'compute', 'https://www.runpod.io',
 'GPU à l''heure, sans engagement. Ce qu''on prend quand Colab ne suffit plus et qu''un serveur dédié est hors budget.',
 'Payant à l''heure, facturable en petites sommes. Carte bancaire internationale requise.',
 ARRAY['llm-engineer','mlops-engineer'], 170),

-- ── Sûreté ─────────────────────────────────────────────────────────
('lm-evaluation-harness', 'lm-evaluation-harness', 'safety', 'https://github.com/EleutherAI/lm-evaluation-harness',
 'Le banc de référence pour évaluer un modèle de langage. Un résultat produit par cet outil est comparable ; un résultat maison ne l''est pas.',
 'Libre. Le coût est celui du calcul d''inférence.',
 ARRAY['ai-safety-researcher','llm-engineer'], 180),

('helm', 'HELM', 'safety', 'https://crfm.stanford.edu/helm',
 'Évaluation holistique : plusieurs axes, pas un score unique. Utile pour argumenter au-delà d''un classement.',
 'Libre, résultats publiés consultables sans rien lancer.',
 ARRAY['ai-safety-researcher'], 190),

('garak', 'garak', 'safety', 'https://github.com/NVIDIA/garak',
 'Scanner de vulnérabilités pour modèles de langage. Un point de départ pour un red-team méthodique.',
 'Libre.',
 ARRAY['ai-safety-researcher','prompt-engineer'], 200),

-- ── Hubs ───────────────────────────────────────────────────────────
('huggingface', 'HuggingFace', 'hub', 'https://huggingface.co',
 'Là où vivent les modèles et les jeux de données. Publier ici rend un travail trouvable ; le garder ailleurs le rend invisible.',
 'Gratuit pour publier, y compris des dépôts privés.',
 ARRAY[]::TEXT[], 210),

('huggingface-papers', 'HuggingFace Papers', 'hub', 'https://huggingface.co/papers',
 'Les articles du jour, reliés aux modèles et aux jeux de données qui les implémentent. Le chemin le plus court entre lire et lancer.',
 'Libre, sans compte.',
 ARRAY['ai-safety-researcher','llm-engineer'], 220),

('arxiv', 'arXiv', 'hub', 'https://arxiv.org',
 'Où paraissent les préprints. Lire les articles cités par un modèle qu''on utilise change la façon de s''en servir.',
 'Libre, sans compte.',
 ARRAY[]::TEXT[], 230),

-- ── Communautés ────────────────────────────────────────────────────
('masakhane', 'Masakhane', 'community', 'https://www.masakhane.io',
 'Recherche en TAL pour les langues africaines, menée depuis le continent. La communauté la plus proche du terrain de Skilluv.',
 'Ouverte, contributions bienvenues sans affiliation académique.',
 ARRAY['nlp-engineer'], 240),

('deep-learning-indaba', 'Deep Learning Indaba', 'community', 'https://deeplearningindaba.com',
 'La rencontre annuelle de l''IA africaine, avec des bourses de participation. Un des rares événements atteignables sans visa lointain.',
 'Candidature annuelle, bourses couvrant voyage et hébergement.',
 ARRAY[]::TEXT[], 250),

('eleutherai', 'EleutherAI', 'community', 'https://www.eleuther.ai',
 'Recherche ouverte sur les grands modèles, sur Discord, à ciel ouvert. On peut y lire les chercheurs travailler.',
 'Discord public.',
 ARRAY['llm-engineer','ai-safety-researcher'], 260),

('alignment-forum', 'Alignment Forum', 'community', 'https://www.alignmentforum.org',
 'Où se discute l''alignement. Exigeant, parfois insulaire, et incontournable pour qui vise la sûreté.',
 'Lecture libre.',
 ARRAY['ai-safety-researcher'], 270),

-- ── Apprentissage ──────────────────────────────────────────────────
('fast-ai', 'fast.ai', 'learning', 'https://course.fast.ai',
 'Le cours qui commence par entraîner un modèle et explique ensuite. Le meilleur premier pas sans mathématiques préalables.',
 'Gratuit, sans inscription. Conçu pour tourner sur Colab.',
 ARRAY['ml-engineer','computer-vision-engineer'], 280),

('huggingface-course', 'Cours HuggingFace', 'learning', 'https://huggingface.co/learn',
 'Du transformeur à l''agent, avec le code qu''on utilisera vraiment ensuite.',
 'Gratuit.',
 ARRAY['nlp-engineer','llm-engineer','prompt-engineer'], 290),

('made-with-ml', 'Made With ML', 'learning', 'https://madewithml.com',
 'Le passage du carnet Jupyter au service en production. Comble exactement le trou entre ml-engineer et mlops-engineer.',
 'Gratuit.',
 ARRAY['mlops-engineer','ml-engineer'], 300),

('spinning-up-safety', 'AI Safety Fundamentals', 'learning', 'https://aisafetyfundamentals.com',
 'Parcours de lecture structuré sur l''alignement, avec des cohortes encadrées.',
 'Gratuit, cohortes sur candidature.',
 ARRAY['ai-safety-researcher'], 310);
