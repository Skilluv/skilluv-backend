# AI domain charter

*To be published at `skill-uv.com/ai/charter`.*

This charter states what is required, what is refused, and what a validation
rests on. It is binding: a deliverable that departs from it is refused,
whatever the score announced.

---

## 1. What an AI deliverable is

A deliverable is an **artefact a stranger can hold you to**: something they
can open, run and judge without taking your word for it.

Admissible:

- a model published at an address where it can be downloaded and run;
- a dataset published with its card;
- an agent system in service, with its evaluations;
- a paper — preprint or conference — with the code behind it;
- a public benchmark result somebody else has re-run;
- a safety finding reproduced and disclosed properly.

Not admissible:

- a Jupyter notebook reproducing a tutorial;
- a score announced with no held-out test set;
- a screenshot of a curve;
- a model described but not downloadable;
- a prompt that "works well" with no evaluation behind it.

The difference is not difficulty. It is verifiability.

## 2. Four non-negotiables

**An honest evaluation.** The test set is separate from training and
resembles what the model will actually see. The score announced is the test
score, not the best attempt. **Data leakage is the most common error in this
domain and the least visible** — it is the first thing looked for.

**Reproducibility.** Seeds, library versions and data pinned. A reader runs it
again and gets the same figures — or the expected variation is written down
before anybody asks.

**Data provenance.** Where it came from, under which licence, with what
consent. A dataset scraped without the right makes everything else unusable,
including for the company that would pick the work up.

**Stated limits.** What the work fails at is written by its author. Work that
does not know its limits has not been evaluated: it has been shown.

## 3. A baseline

A model with no control proves nothing. Every result is compared against
something: a naive forecast, a logistic regression, last week's model, a
published figure.

It is a requirement and a protection. Most complex models do not improve on
the simple baseline, and finding that out yourself beats learning it in front
of a reviewer.

## 4. Ethics

**Attribution.** Reused code, weights and data are cited. A licence chain is
respected in full: fine-tuning a model under a community licence does not
erase that licence.

**Personal data.** No published dataset contains personal data without a
legal basis and consent. A model trained on it cannot forget: that is why the
question is asked before, not after.

**Safety before publication.** A dangerous capability is not published
because it is impressive. See the [disclosure policy](./SAFETY-DISCLOSURE.en.md).

## 5. AI assistance

In a domain whose subject *is* AI, the question is put differently: what is
judged is not who typed the code, but whether you can answer for what the
system does.

Using an assistant is **accepted and declared**. The submission states the
level of assistance. Concealing it is a separate offence from using it, and
it is the concealment that is sanctioned.

Defending the work live — explaining why that learning rate, showing what
breaks when an assumption changes — is what settles it.

## 6. Validation

A validation rests on the **review grid of the family of trades** concerned,
public and readable before submitting.

A refusal names its reason among those the platform can state: insufficient
evaluation, missing reproducibility, unclear data provenance, a safety
problem — or one of the reasons common to every domain. A refusal with no
actionable reason is not a valid refusal.

Five passes at most. Beyond that the work is no longer what is wrong: the
scope or the assignment is, and somebody goes through them with you.

## 7. Reproduction

A benchmark and a safety finding count only once **somebody else has replayed
them**. Confirming your own measurement is precisely what reproduction exists
to rule out: the platform refuses it, whatever rights you hold.

## 8. Revocation

A validated artefact can be revoked: leakage discovered, a dataset withdrawn
over licensing, a result that cannot be reproduced, plagiarism.

Revocation removes the artefact from every count — rank, badges, attestations
that rested on it. It does not erase the history: what was revoked stays
visible as revoked.

---

*See also: the [brief templates](./BRIEF-TEMPLATES.en.md), the [disclosure
policy](./SAFETY-DISCLOSURE.en.md), and the writeup templates served by
`GET /api/guides?domain=ai&kind=writeup_template`.*
