-- The English half of the AI guides and templates.
--
-- Migration 0231 seeded French only. The code guides have carried both since
-- 0199, and F-01, F-05 and G-01 each say "FR + EN" — so the AI half was
-- simply missing, not deferred.
--
-- Same slugs, `locale = 'en'`. The listing endpoint reads one locale and
-- falls back to French, so a slug with no English row degrades to the French
-- page rather than disappearing; these rows are what stop that fallback from
-- being the normal case for half the domain.
--
-- Translated rather than rewritten: an English reader and a French reader
-- should be told the same thing, including the trap section, which is the
-- part most worth getting across.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-ai-data', 'onboarding', 'ai', 'data', 'en',
 'Getting started in data',
 'Engineering and analysis. The AI family that asks for the least compute and the most rigour about what a number means.',
$md$
# Getting started in data

Two trades: getting data to where it is useful, and turning it into a
decision. This is the family you can go furthest in without a GPU — DuckDB
runs on a laptop — and the one where the work is judged fastest, because a
wrong number shows.

## What you need

Serious SQL. Python for the engineering side. No particular hardware: the
best learning-to-cost ratio in the whole domain.

## Thirty days

**Week 1 — a question.** Take a public dataset and one precise question.
Write the metric definition before the query. That is the exercise, not the
query.

**Week 2 — a pipeline that fails well.** Orchestrate a load with Dagster,
locally. Kill it halfway on purpose, and look at what the table contains
afterwards.

**Week 3 — the checks.** Add quality checks that stop the pipeline. One that
carries on with wrong data costs more than one that stopped.

**Week 4 — the backfill.** Replay seven days of history without duplicating
anything. That is what separates a script from a pipeline, and it is the
first thing a reviewer looks at.

## The trap in this family

A beautiful dashboard whose headline number two people compute differently.
The written definition is worth more than the chart.

## Where the people are

dbt Slack, r/dataengineering, Locally Optimistic.
$md$, 10),

('onboarding-ai-ml', 'onboarding', 'ai', 'ml', 'en',
 'Getting started in models',
 'Training, and keeping alive. Two trades the market confuses and that do not resemble each other.',
$md$
# Getting started in models

Training a model and keeping one in service are two jobs. The first ends when
the curve looks good; the second starts there.

## What you need

Python, a little linear algebra, and Colab is enough to begin. For the
operations half: containers and CI/CD — the ops trade applied to artefacts
that age.

## Thirty days

**Week 1 — fast.ai.** The course starts by training a model and explains
afterwards. That is the right order.

**Week 2 — beat the baseline.** Take a tabular dataset and try to beat a
well-tuned logistic regression. Finding out that this is hard is the first
real thing the trade teaches.

**Week 3 — serve it.** Put the model behind an API and measure latency under
load. Read the high percentiles: the mean lies.

**Week 4 — watch it.** Decide what you would monitor in production, and which
threshold would trigger what. Before the incident, not after.

## The trap in this family

Data leakage. A random split on time-ordered data manufactures a score
nothing will reproduce. It is the most common error in the domain and the
least visible — and the first one a reviewer goes looking for.

## Where the people are

r/MachineLearning, PyTorch Forums, MLOps Community Slack.
$md$, 20),

('onboarding-ai-llm-nlp', 'onboarding', 'ai', 'llm-nlp', 'en',
 'Getting started in language',
 'Prompts, agents and NLP. The family everybody arrives in and almost nobody measures.',
$md$
# Getting started in language

Three trades: calibrating prompts, building systems that use a model, and
treating language as a structure. It is the busiest entrance to the domain,
and the one where the gap between somebody who knows and somebody who is
guessing shows fastest.

## What you need

Enough Python to read it. API access, or llama.cpp locally — a quantised
seven-billion-parameter model fits in eight gigabytes of RAM.

## Thirty days

**Week 1 — the eval set first.** Write twenty cases, five of them where the
system must refuse or say it does not know. Before writing a single prompt.
That inversion is the whole trade.

**Week 2 — the prompts.** Calibrate them against that set. Version them. A
change is justified by a measurement, not by an impression.

**Week 3 — a RAG.** On a corpus you know, so you can judge the answers.
Measure what each stage contributes: without an ablation, nobody knows
whether the reranker earns its place.

**Week 4 — the attack.** Try to talk your own system out of its role. Record
the success rate. That is the figure a reviewer wants.

## The trap in this family

"It works well." Without an eval set there is no work, there is an
impression — and three successful examples are not one.

## Where the people are

DSPy Discord, r/LocalLLaMA, EleutherAI, and **Masakhane** for African-language
NLP: the closest ground to here, open without an academic affiliation.
$md$, 30),

('onboarding-ai-cv', 'onboarding', 'ai', 'cv', 'en',
 'Getting started in images',
 'Vision and generative. Where the dataset decides more than the architecture.',
$md$
# Getting started in images

Detect, segment, or produce. The two halves of this family — vision that
judges and diffusion that makes — share one thing: the result depends on the
data far more than on the model.

## What you need

PyTorch, patience with image datasets, and Colab for fine-tuning. A GPU by
the hour for generative work.

## Thirty days

**Week 1 — annotate.** Label two hundred images yourself. It is tedious, and
it is the only way to understand why the model gets things wrong.

**Week 2 — fine-tune.** A detector on that set. Look at what it misses, not
only at the mAP.

**Week 3 — degrade it.** Test on blur, backlight, an unusual angle. Clean
images prove nothing.

**Week 4 — the subgroup.** Measure performance per group. A face model that
has not been tested across skin tones has not been tested.

For generative work, that fourth week reads differently: reproduce an image
deliberately. Same seed, same parameters, same result. As long as you are
rerolling until something lands, there is no trade yet.

## The trap in this family

An average that hides the group the model fails on.

## Where the people are

r/computervision, Ultralytics Discord, r/StableDiffusion, ComfyUI Discord.
$md$, 40),

('onboarding-ai-safety', 'onboarding', 'ai', 'safety', 'en',
 'Getting started in safety',
 'Looking for failure on purpose. The family where method matters more than technique.',
$md$
# Getting started in safety

Finding what a model does when somebody tries to break it, and saying so
properly. It is the one trade in the domain where writing a protocol counts
for more than training anything.

## What you need

Rigour before technique. Access to an open model. No particular hardware.

## Thirty days

**Week 1 — read.** AI Safety Fundamentals, and two published red-team
reports. Study their shape as much as their content.

**Week 2 — replay.** Reproduce a published red-team on an open model. Replay
before you find: you learn what makes a result reproducible by trying to
reproduce somebody else's.

**Week 3 — measure.** Fifty attempts, a success rate. A screenshot is not a
finding.

**Week 4 — disclose.** Write the mitigation, notify the vendor, record the
date. The [disclosure policy](/ai/disclosure) sets the order, and it is
binding.

## The trap in this family

Publishing fast. Between the moment an attack is written publicly and the
moment it is fixed, anybody can use it.

## Where the people are

Alignment Forum, EleutherAI, Deep Learning Indaba for the continent.
$md$, 50),

-- ═══════════════════════════════════════════════════════════════════
-- The templates
-- ═══════════════════════════════════════════════════════════════════

('template-model-card', 'writeup_template', 'ai', NULL, 'en',
 'Model card',
 'HuggingFace-compatible, so one card serves here and there.',
$md$
# Model card

- **What the model does**, in one sentence, and for whom.
- **Intended use**, and **uses explicitly discouraged**.
- **Training data**: source, volume, period, licence, preprocessing.
- **Training procedure**: hardware, duration, hyperparameters, seed.
- **Evaluation**: test set, metrics, baseline compared against.
- **Performance per subgroup**, where the notion applies.
- **Known limits**: what it fails at, written by you.
- **Footprint**: compute cost of training, cost of one inference.
- **Licence** of the model, and the upstream licences respected.
- **How to cite**.
$md$, 110),

('template-dataset-card', 'writeup_template', 'ai', NULL, 'en',
 'Dataset card',
 'What is in it, where it came from, and what anybody is allowed to do with it.',
$md$
# Dataset card

- **What the dataset contains**, and what it does not.
- **Provenance**: where the data came from, how it was obtained.
- **Consent and personal data**: legal basis, anonymisation.
- **Composition**: size, class distribution, languages, period.
- **Annotation protocol**: instructions, number of annotators, agreement.
- **Splits provided** and along which axis.
- **Known biases**: what is over- and under-represented.
- **Licence** and reuse conditions.
- **Maintenance**: who fixes a reported error, and where to report one.
$md$, 120),

('template-experiment-report', 'writeup_template', 'ai', NULL, 'en',
 'Experiment report',
 'The question asked before the run, and what did not work.',
$md$
# Experiment report

- **Question**: what you were trying to find out, written before the run.
- **Hypothesis** and what would have refuted it.
- **Protocol**: variants compared, what varies and what is held fixed.
- **Results**: the table, with standard deviations across several seeds.
- **Interpretation**: what the numbers support, and what they do not.
- **What did not work.** The most useful section for whoever comes next, and
  the first to disappear when the report is written afterwards.
$md$, 130),

('template-benchmark-report', 'writeup_template', 'ai', NULL, 'en',
 'Benchmark report',
 'What makes a result replayable by a stranger.',
$md$
# Benchmark report

- **Benchmark**: name, version, dataset and exact split.
- **Metric**, unit, and which direction is better.
- **Baselines** with their sources.
- **Harness** used and its version — `lm-evaluation-harness`, `criterion`.
- **Hardware**: machine, card, memory.
- **Method**: warm-up, iteration count, what is timed.
- **The exact command** to run it again.
- **Expected variation** between two runs.
$md$, 140),

('template-paper-abstract', 'writeup_template', 'ai', NULL, 'en',
 'Paper abstract',
 'What this work adds to what is already published.',
$md$
# Paper abstract

- **Context** in two sentences.
- **What this work adds** to what is already published.
- **Method**, enough to follow without reading the code.
- **Headline result**, with a number.
- **Limits**, before a reviewer finds them.
- **Code and data**: the addresses.
$md$, 150),

('template-rag-design', 'writeup_template', 'ai', NULL, 'en',
 'RAG system design',
 'The corpus, the chunking, and the ablation that says what each stage contributes.',
$md$
# RAG system design

- **Corpus**: what, how much, how often it changes.
- **Chunking**: fragment size, overlap, and why.
- **Retrieval**: lexical, dense, hybrid; the reranker if there is one.
- **Ablation**: what each stage contributes, measured. Without it nobody
  knows whether the reranker earns its place.
- **Eval set**: the questions, and the failure cases chosen on purpose.
- **Behaviour when nothing is found.**
- **Cost and latency** per query.
$md$, 160),

('template-agent-design', 'writeup_template', 'ai', NULL, 'en',
 'Agent system design',
 'What the agent can reach, what it cannot, and when it stops.',
$md$
# Agent system design

- **Task** and stopping condition.
- **Tools** exposed, and their permission limits.
- **Shared state**: what passes between agents.
- **Loops**: what prevents an infinite run.
- **Sandbox**: what the agent cannot reach.
- **Evaluation**: success rate, average cost, one complete trace.
- **Failure recovery**: what happens when a tool returns an error.
$md$, 170),

('template-red-team-report', 'writeup_template', 'ai', 'safety', 'en',
 'Red-team report',
 'The shape the disclosure policy expects.',
$md$
# Red-team report

- **Target**: model, version or snapshot date, access mode.
- **Attack type** and why that one.
- **Reproduction**: the procedure, precise enough for a stranger.
- **Observed output**, verbatim.
- **Rate**: successes over attempts.
- **Severity** and the reasoning behind it.
- **Proposed mitigation.**
- **Disclosure timeline**: notification, acknowledgement, agreed window,
  publication.
- **Dual use**: what is withheld, and why.
$md$, 180),

('template-deployment-runbook', 'writeup_template', 'ai', NULL, 'en',
 'Deployment runbook',
 'What somebody needs at three in the morning.',
$md$
# Deployment runbook

- **What is deployed**: model version, fingerprint, date.
- **Deployment**: commands, dependencies, secrets needed.
- **Rollback**: the command, and how long it takes.
- **Monitoring**: metrics watched, thresholds, who is paged.
- **Drift**: what triggers it, and the action attached.
- **Capacity**: what the current machine can serve.
- **Known failures** and their remedies.
$md$, 190),

('template-ai-post-mortem', 'writeup_template', 'ai', NULL, 'en',
 'Incident post-mortem',
 'Without naming anybody: a post-mortem that hunts for a culprit stops producing information the second time.',
$md$
# Incident post-mortem

- **What happened**, and what users saw.
- **Timeline**: from first symptom to recovery.
- **Cause**: technical, and what made it possible.
- **Detection**: how we found out, and how long after.
- **What limited the damage.**
- **What made it worse.**
- **Actions**: each with an owner and a date.
$md$, 200);
