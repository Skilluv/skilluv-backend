-- The `ai` catalogue, from four entries to ten.
--
-- ## Why four was not enough
--
-- "ML Engineer" answered for someone who trains a model, someone who keeps it
-- alive in production, someone who fine-tunes a language model and someone
-- who segments images. Those are four jobs with four hiring markets, and one
-- name for them tells a recruiter nothing.
--
-- The four that exist keep their slugs. They name real trades; two of them
-- simply claimed territory that now has its own name, and their descriptions
-- are narrowed here rather than left to overlap. An orientation whose text
-- still promises MLOps once `mlops-engineer` exists is a description that
-- lies about where the work goes.
--
-- ## Naming
--
-- The default locale carries the French name, as everywhere else. English
-- lives in `orientation_translations`, and the four pre-existing trades get
-- theirs here too — they had none, which made half the catalogue invisible to
-- an English reader for no reason other than that nobody had written it.
--
-- ## `generative-ai-artist`
--
-- Tagged `experimental`. The trade is real and moving fast enough that its
-- boundary with design is not settled, and the tag is what lets a caller
-- exclude it: `GET /api/orientations` already filters on tags, so a surface
-- that should not promote it can say so without a second flag being invented.
-- Somebody who does that work can still claim it, which is the point of
-- listing it at all.

-- ═══════════════════════════════════════════════════════════════════
-- A domain arriving has to be a domain challenges can belong to
-- ═══════════════════════════════════════════════════════════════════
--
-- `challenge_templates.skill_domain` still enumerated the four domains of
-- migration 0003 — code, design, game, security — while `skill_nodes` and
-- `orientations` have known seven since 0056 and 0088. Nothing noticed,
-- because until now no migration inserted a template outside the original
-- four; 0219 seeds forty-one AI challenges and is refused by this constraint,
-- which makes every later migration unreachable.
--
-- Widened here rather than in 0219 because the constraint enumerates domains,
-- and this is the migration where the `ai` domain arrives. A template
-- inserted between the two would otherwise fail for the same reason.
--
-- Two sibling constraints stay narrow on purpose, being nobody's blocker
-- today and both carrying product questions this migration should not answer:
-- `sponsored_challenge_requests.skill_domain`, and `users.skill_domain` —
-- which currently means somebody cannot declare `ai`, `ops` or `soft_skills`
-- as their domain at signup.

ALTER TABLE challenge_templates DROP CONSTRAINT IF EXISTS challenges_skill_domain_check;

ALTER TABLE challenge_templates
    ADD CONSTRAINT challenges_skill_domain_check
    CHECK (skill_domain IN (
        'code', 'design', 'game', 'security', 'soft_skills', 'ai', 'ops'
    ));

-- ═══════════════════════════════════════════════════════════════════
-- The six new trades
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations (slug, name, description, primary_domain, secondary_domains, tags, is_curated) VALUES

('llm-engineer', 'Ingénieur LLM',
 'Fine-tuning, RAG avancé, systèmes multi-agents. Faire tenir un modèle de langage dans un produit qui ne ment pas.',
 'ai', ARRAY['code'], ARRAY['llm','rag','agents'], TRUE),

('mlops-engineer', 'Ingénieur MLOps',
 'Servir, surveiller et redéployer des modèles. Détecter la dérive avant que ce soit l''utilisateur qui la signale.',
 'ai', ARRAY['ops','code'], ARRAY['ml','infra','monitoring'], TRUE),

('computer-vision-engineer', 'Ingénieur Vision par Ordinateur',
 'Détection, segmentation, inférence embarquée. Des modèles qui regardent, et les biais que cela implique.',
 'ai', ARRAY['code'], ARRAY['ml','vision'], TRUE),

('nlp-engineer', 'Ingénieur TAL',
 'Extraction d''entités, sentiment, traduction, résumé. Le langage traité comme une structure, pas comme une invite.',
 'ai', ARRAY['code'], ARRAY['nlp','ml'], TRUE),

('ai-safety-researcher', 'Chercheur en Sûreté des IA',
 'Alignement, red-teaming, robustesse adverse. Chercher ce qu''un modèle fait quand on essaie de le faire échouer.',
 'ai', ARRAY['security','soft_skills'], ARRAY['safety','alignment','red-team'], TRUE),

('generative-ai-artist', 'Artiste IA Générative',
 'Diffusion, ControlNet, LoRA, chaînes ComfyUI. Une direction artistique tenue sur une série, pas une image réussie.',
 'ai', ARRAY['design'], ARRAY['generative','experimental'], TRUE);

-- ═══════════════════════════════════════════════════════════════════
-- The two that now overlap something with a name
-- ═══════════════════════════════════════════════════════════════════

UPDATE orientations
   SET description = 'Entraînement, évaluation et mise en production de modèles. L''expérimentation et ce qui la rend reproductible.',
       tags = ARRAY['ml','training'],
       updated_at = NOW()
 WHERE slug = 'ml-engineer';

UPDATE orientations
   SET description = 'Invites calibrées, évaluations, garde-fous. Rendre prévisible un modèle qu''on ne contrôle pas.',
       tags = ARRAY['llm','evals'],
       updated_at = NOW()
 WHERE slug = 'prompt-engineer';

-- ═══════════════════════════════════════════════════════════════════
-- English
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientation_translations (orientation_id, locale, name, description)
SELECT o.id, 'en', t.name, t.description
FROM (VALUES
    ('data-engineer', 'Data Engineer',
     'ETL pipelines, warehouses, streaming. Moving data so that what arrives is what was sent.'),
    ('data-analyst', 'Data Analyst',
     'Advanced SQL, dashboards, cohort work. Turning a table into a decision somebody can act on.'),
    ('ml-engineer', 'ML Engineer',
     'Training, evaluating and shipping models. The experiment, and what makes it reproducible.'),
    ('prompt-engineer', 'Prompt Engineer',
     'Calibrated prompts, evals, guardrails. Making a model you do not control behave predictably.'),
    ('llm-engineer', 'LLM Engineer',
     'Fine-tuning, advanced RAG, multi-agent systems. Fitting a language model into a product that does not lie.'),
    ('mlops-engineer', 'MLOps Engineer',
     'Serving, monitoring and redeploying models. Catching drift before a user reports it.'),
    ('computer-vision-engineer', 'Computer Vision Engineer',
     'Detection, segmentation, edge inference. Models that look, and the bias that comes with it.'),
    ('nlp-engineer', 'NLP Engineer',
     'Entity extraction, sentiment, translation, summarisation. Language as a structure rather than a prompt.'),
    ('ai-safety-researcher', 'AI Safety Researcher',
     'Alignment, red-teaming, adversarial robustness. Finding what a model does when someone tries to break it.'),
    ('generative-ai-artist', 'Generative AI Artist',
     'Diffusion, ControlNet, LoRA, ComfyUI graphs. A direction held across a series, not one lucky image.')
) AS t(slug, name, description)
JOIN orientations o ON o.slug = t.slug
ON CONFLICT (orientation_id, locale) DO UPDATE
    SET name = EXCLUDED.name,
        description = EXCLUDED.description,
        updated_at = NOW();
