# Brief templates — AI domain

Six templates, one per kind of artefact. Use them to write the statement of an
AI challenge.

A badly written brief produces deliverables that cannot be compared: each
answers a different question, and the reviewer arbitrates on instinct. It is
worse in AI than elsewhere, because a number looks comparable even when it is
not.

**The family decides which review grid applies** (`reviewer_group` on the
orientation). Writing a brief in the wrong family promises one grid and
applies another.

---

## The common structure

Every AI brief has these seven sections, in this order.

### 1. Problem

What is wrong today, **from the point of view of somebody it bothers**. Not
the expected solution, and certainly not the architecture.

> Bad: "Train a BERT classifier."
> Good: "Support sorts twelve hundred messages a day by hand and takes four
> hours to spot the urgent ones."

### 2. Available data

What exists, in what state, under which licence, at what volume. If the brief
provides no data, it says where to find admissible data.

State what is **forbidden** too: scraping a site, reusing a
non-commercially-licensed set for a commercial deliverable.

An empty data section produces a deliverable whose provenance nobody can judge.

### 3. Baseline

What the result is measured against. A naive forecast, an existing business
rule, a published model, the version in production.

**Required.** A brief with no baseline asks for a number with no second term,
and everybody passes.

### 4. Success metrics

The metric, the unit, the direction (higher is better or the reverse), and the
threshold. With numbers.

> Bad: "The model must be good."
> Good: "Macro F1 ≥ 0.78 on the supplied test set, against 0.61 for the
> current rule."

Say what is measured **in addition to** quality: latency, memory, cost per
thousand requests. A model that does not fit the target is not finished.

### 5. Deployment target

Where the result has to run: a CPU, an eight-gigabyte card, an embedded board,
a third-party API. This section changes everything else and is almost always
forgotten.

### 6. Ethics and provenance

What to look at for this specific subject: subgroups to evaluate separately,
personal data, plausible misuse, upstream licences to respect.

A checkbox will not do. If the brief cannot name the risk of its own subject,
it is not ready.

### 7. Out of scope

What is explicitly not being asked for. This is the section that stops a
candidate spending three weeks on a web interface nobody mentioned.

---

## 1. `brief-data-pipeline.md`

On top of the common structure:

- **Sources and expected freshness**: where the data comes from, how often it
  must arrive, what lag is tolerable.
- **Delivery semantics**: at-most-once, at-least-once, exactly-once. Say which
  is required — not "reliable".
- **Volume and growth**: today's figure and the projection, or the work is
  sized for the sample.
- **Failure behaviour**: what must happen if the source disappears mid-load.
- **Budget**: bytes scanned or a monthly ceiling. A scheduled pipeline costs
  every day.

Typical acceptance: a backfill over N days that duplicates nothing, quality
checks that stop the pipeline, a documented recovery procedure.

## 2. `brief-ml-model.md`

On top of the common structure:

- **How the sets are split**: how train, validation and test are separated,
  and along which axis. A random split on time-ordered data manufactures
  leakage.
- **Class imbalance**: the real distribution, and the metric chosen because of
  it.
- **Inference constraints**: maximum latency, memory, hardware.
- **Retraining**: how often, and on what data.

Typical acceptance: the stated threshold met on the test set, a comparison
against the baseline, training that reproduces on another machine.

## 3. `brief-llm-agent-system.md`

On top of the common structure:

- **Eval set**: the cases, including failure cases chosen on purpose. Supplied
  by the brief, or to be built — and then it is a deliverable in its own right.
- **Tools available**, and what the agent must never be able to do.
- **Behaviour when it does not know**: say so, or invent. The first is the only
  acceptable answer, and it is testable.
- **Budget per run**: tokens, calls, response time.
- **Injection**: can third-party input reach it, and what should happen then.

Typical acceptance: a success rate on the eval set, a correct refusal rate on
out-of-scope cases, a measured average cost.

## 4. `brief-cv-application.md`

On top of the common structure:

- **Capture conditions**: light, angle, resolution, blur. A model trained on
  clean images fails in production.
- **Dataset composition** and subgroups to evaluate separately.
- **Annotation**: supplied, or to be produced — and then with which protocol
  and what inter-annotator agreement.
- **Target hardware** and expected frames per second.
- **People**: if the subject shows any, intended use and consent are settled in
  the brief, not in review.

Typical acceptance: a numeric mAP or mIoU, performance per subgroup,
throughput measured on the target hardware.

## 5. `brief-nlp-service.md`

On top of the common structure:

- **Languages** covered, and the level expected for each. An average hides the
  one language that fails.
- **Text domain**: legal, medical, conversational. A general model collapses
  outside it.
- **Annotated set**: supplied or to be produced, with the annotation protocol.
- **The metric and its limit**: BLEU, ROUGE and the rest are reported with what
  they do not measure.

Typical acceptance: the metric **per language** and per entity type, plus a
manual evaluation on a sample.

## 6. `brief-ai-safety-evaluation.md`

On top of the common structure:

- **Exact target and version**, including snapshot date.
- **Authorised scope**: what may be attempted, and on which accounts. A
  red-team outside scope is an incident, not a deliverable.
- **Minimum number of attempts** for a rate to mean anything.
- **Disclosure route**: who to notify, within what window, and who decides in a
  dual-use case.
- **What will not be published**: decided before starting, not after finding.

Typical acceptance: a protocol a third party can replay, a success rate over N
attempts, a proposed mitigation, and a disclosure conforming to the
[policy](./SAFETY-DISCLOSURE.en.md).

---

*See also: the [domain charter](./CHARTER.en.md).*
