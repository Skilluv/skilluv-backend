-- What each AI trade is actually made of.
--
-- ## Core and recommended
--
-- Core is what the trade cannot exist without: remove it and the person is
-- doing something else. Three to four per orientation. A trade where
-- everything is core says nothing about what to learn first, which is the
-- only thing this map is read for.
--
-- ## Why several rows point outside `ai`
--
-- `python`, `sql`, `containers`, `ci-cd` and `observability` live under `code`
-- and `ops`. Every AI trade rests on some of them, and duplicating the nodes
-- under `ai` would give two answers to whether somebody knows Python. The map
-- crosses domains for the same reason 0175 does.
--
-- ## The four trades that had nothing
--
-- `data-engineer`, `data-analyst`, `ml-engineer` and `prompt-engineer` were
-- seeded in 0088 with no skill map at all. They have looked supported for two
-- years and were not: nothing could be recommended, nothing verified. They
-- are mapped here alongside the six new ones.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended)
SELECT o.id, s.id, m.is_core, NOT m.is_core
FROM (VALUES

-- ── data-engineer ──────────────────────────────────────────────────
('data-engineer', 'batch-orchestration', TRUE),
('data-engineer', 'streaming-ingestion', TRUE),
('data-engineer', 'warehouse-modelling', TRUE),
('data-engineer', 'sql', TRUE),
('data-engineer', 'data-quality-testing', FALSE),
('data-engineer', 'lakehouse-formats', FALSE),
('data-engineer', 'pipeline-cost-control', FALSE),
('data-engineer', 'reverse-etl', FALSE),
('data-engineer', 'python', FALSE),
('data-engineer', 'containers', FALSE),
('data-engineer', 'postgres-indexing', FALSE),

-- ── data-analyst ───────────────────────────────────────────────────
('data-analyst', 'sql', TRUE),
('data-analyst', 'sql-window-functions', TRUE),
('data-analyst', 'dashboard-design', TRUE),
('data-analyst', 'metric-definition', TRUE),
('data-analyst', 'sql-cte', FALSE),
('data-analyst', 'cohort-retention-analysis', FALSE),
('data-analyst', 'ab-test-analysis', FALSE),
('data-analyst', 'data-storytelling', FALSE),
('data-analyst', 'technical-writing', FALSE),
('data-analyst', 'python', FALSE),

-- ── ml-engineer ────────────────────────────────────────────────────
('ml-engineer', 'model-evaluation', TRUE),
('ml-engineer', 'deep-learning-training', TRUE),
('ml-engineer', 'supervised-modelling', TRUE),
('ml-engineer', 'python', TRUE),
('ml-engineer', 'feature-engineering', FALSE),
('ml-engineer', 'transfer-learning', FALSE),
('ml-engineer', 'experiment-tracking', FALSE),
('ml-engineer', 'training-reproducibility', FALSE),
('ml-engineer', 'recommender-systems', FALSE),
('ml-engineer', 'time-series-forecasting', FALSE),
('ml-engineer', 'model-serving', FALSE),

-- ── prompt-engineer ────────────────────────────────────────────────
('prompt-engineer', 'system-prompt-design', TRUE),
('prompt-engineer', 'llm-evals-design', TRUE),
('prompt-engineer', 'prompt-robustness', TRUE),
('prompt-engineer', 'prompt-clarity', FALSE),
('prompt-engineer', 'few-shot-prompting', FALSE),
('prompt-engineer', 'chain-of-thought', FALSE),
('prompt-engineer', 'context-window-management', FALSE),
('prompt-engineer', 'llm-guardrails', FALSE),
('prompt-engineer', 'prompt-injection-defense', FALSE),
('prompt-engineer', 'prompt-library-versioning', FALSE),
('prompt-engineer', 'llm-cost-optimization', FALSE),

-- ── llm-engineer ───────────────────────────────────────────────────
('llm-engineer', 'lora-fine-tuning', TRUE),
('llm-engineer', 'hybrid-retrieval', TRUE),
('llm-engineer', 'llm-evals-design', TRUE),
('llm-engineer', 'tool-use-orchestration', TRUE),
('llm-engineer', 'rag-basics', FALSE),
('llm-engineer', 'query-decomposition', FALSE),
('llm-engineer', 'multi-agent-orchestration', FALSE),
('llm-engineer', 'model-distillation', FALSE),
('llm-engineer', 'agent-tool-safety', FALSE),
('llm-engineer', 'llm-api-integration', FALSE),
('llm-engineer', 'llm-cost-optimization', FALSE),
('llm-engineer', 'deep-learning-training', FALSE),

-- ── mlops-engineer ─────────────────────────────────────────────────
('mlops-engineer', 'model-serving', TRUE),
('mlops-engineer', 'drift-detection', TRUE),
('mlops-engineer', 'ml-ci-cd', TRUE),
('mlops-engineer', 'containers', TRUE),
('mlops-engineer', 'model-registry-versioning', FALSE),
('mlops-engineer', 'inference-cost-optimization', FALSE),
('mlops-engineer', 'gpu-capacity-planning', FALSE),
('mlops-engineer', 'feature-store', FALSE),
('mlops-engineer', 'observability', FALSE),
('mlops-engineer', 'ci-cd', FALSE),
('mlops-engineer', 'training-reproducibility', FALSE),

-- ── computer-vision-engineer ───────────────────────────────────────
('computer-vision-engineer', 'object-detection', TRUE),
('computer-vision-engineer', 'image-segmentation', TRUE),
('computer-vision-engineer', 'vision-dataset-curation', TRUE),
('computer-vision-engineer', 'deep-learning-training', TRUE),
('computer-vision-engineer', 'edge-vision-optimization', FALSE),
('computer-vision-engineer', 'vision-bias-evaluation', FALSE),
('computer-vision-engineer', 'video-tracking', FALSE),
('computer-vision-engineer', 'model-evaluation', FALSE),
('computer-vision-engineer', 'transfer-learning', FALSE),
('computer-vision-engineer', 'python', FALSE),

-- ── nlp-engineer ───────────────────────────────────────────────────
('nlp-engineer', 'named-entity-recognition', TRUE),
('nlp-engineer', 'text-classification', TRUE),
('nlp-engineer', 'text-tokenization', TRUE),
('nlp-engineer', 'model-evaluation', TRUE),
('nlp-engineer', 'entity-linking', FALSE),
('nlp-engineer', 'machine-translation-eval', FALSE),
('nlp-engineer', 'summarization-eval', FALSE),
('nlp-engineer', 'low-resource-nlp', FALSE),
('nlp-engineer', 'transfer-learning', FALSE),
('nlp-engineer', 'python', FALSE),

-- ── ai-safety-researcher ───────────────────────────────────────────
('ai-safety-researcher', 'llm-red-teaming', TRUE),
('ai-safety-researcher', 'eval-harness-design', TRUE),
('ai-safety-researcher', 'bias-measurement', TRUE),
('ai-safety-researcher', 'responsible-disclosure-ai', TRUE),
('ai-safety-researcher', 'jailbreak-taxonomy', FALSE),
('ai-safety-researcher', 'alignment-techniques', FALSE),
('ai-safety-researcher', 'dual-use-assessment', FALSE),
('ai-safety-researcher', 'prompt-injection-defense', FALSE),
('ai-safety-researcher', 'agent-tool-safety', FALSE),
('ai-safety-researcher', 'technical-writing', FALSE),

-- ── generative-ai-artist ───────────────────────────────────────────
('generative-ai-artist', 'diffusion-pipelines', TRUE),
('generative-ai-artist', 'controlnet-conditioning', TRUE),
('generative-ai-artist', 'generative-series-consistency', TRUE),
('generative-ai-artist', 'lora-training-visual', FALSE),
('generative-ai-artist', 'comfyui-workflows', FALSE),
('generative-ai-artist', 'generative-provenance', FALSE),
('generative-ai-artist', 'transfer-learning', FALSE)

) AS m(orientation_slug, skill_slug, is_core)
JOIN orientations o ON o.slug = m.orientation_slug
JOIN skill_nodes  s ON s.slug = m.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;
