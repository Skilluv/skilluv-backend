# Brief templates — Code

Eight templates, one per family of trades. Use them to write the statement of
a challenge.

A badly written brief produces deliverables that cannot be compared: each one
answers a different question, and the reviewer ends up judging by feel. The
shared structure below exists to prevent that.

**The families are the review groups** (`reviewer_group` on the orientations),
and therefore the review grid that will be applied. Writing a brief in the
wrong family promises one grid and applies another.

---

## Shared structure

Every brief has these six sections, in this order.

### 1. Problem

What is wrong today, in one or two sentences, **from the point of view of
somebody it bothers**. Not the expected solution.

> Bad: "Implement a Redis cache."
> Good: "The home page queries the database on every visit and takes two
> seconds to answer at peak."

### 2. Technical constraints

What is imposed and what is free. An unwritten constraint is one the candidate
discovers in review, which is unfair.

State: required languages or platforms, forbidden dependencies, minimum
version to support, deployment environment.

### 3. Deliverables

Always the three: **code**, **tests**, **documentation**. Say where they land
— a repository, an upstream contribution, a published package.

### 4. Acceptance criteria

Checkable, in the sense that two reviewers would reach the same conclusion.
Put numbers on whatever can carry them.

> Bad: "The site must be fast."
> Good: "LCP under 2.5 s on simulated mobile 4G, measured before and after."

### 5. Licence

The deliverable's, and the ones to respect upstream. Work with no licence is
unusable by anybody.

### 6. Out of scope

What **not** to do. The most frequently omitted section, and the one that
avoids the most wasted work.

---

## 1. Web application — `web`

*frontend, backend, fullstack, performance, web3-frontend*

Also state:
- browsers and screen sizes to support;
- the API contract, if front and back are separate;
- the accessibility requirement (target level, and how it will be checked);
- a performance budget, with numbers.

For web3: the target network, what happens when the user refuses to sign, and
the behaviour on a chain reorganisation.

## 2. Mobile application — `mobile`

*iOS, Android, cross-platform*

Also state:
- minimum OS versions;
- expected offline behaviour — the question that separates serious mobile work
  from the rest;
- permissions requested, and what to do when one is permanently refused;
- whether store publication is part of the deliverable.

## 3. Desktop and enterprise software — `devtools-media`

*desktop, enterprise, low-code*

Also state:
- target operating systems and installation method;
- signing and updates: expected, or out of scope;
- for enterprise: authentication method, data separation, audit requirements.

## 4. Systems and embedded — `systems`

*systems programming, kernel, firmware, robotics, safety-critical*

Also state:
- target hardware, or a simulator accepted instead;
- memory and power constraints, with numbers;
- expected behaviour on failure — an embedded system with no defined degraded
  mode is not finished;
- for safety-critical: the applicable standard and the target level.

## 5. Blockchain — `blockchain`

*smart contracts, protocols*

Also state:
- testnet then mainnet, or testnet only;
- gas budget where relevant;
- trust assumptions: who can do what, and what the administrator can do;
- **always**: a deployment cannot be corrected. The brief must say what is
  irreversible.

## 6. Compilers and formal methods — `compilers`

*compilers, languages, proofs*

Also state:
- the reference grammar or specification;
- the generation target, or the property to prove;
- the requirement on error messages: a tool that only says "no" helps nobody,
  and that is judged;
- a set of test programs, supplied or to be built.

## 7. Data and distributed systems — `data`

*database engines, search, distributed, streaming*

Also state:
- expected guarantees: consistency, durability, delivery semantics;
- reference volume and load for the measurements;
- failures to survive, and the expected behaviour under each;
- what is measured: high percentiles, not the mean.

## 8. Scientific and GPU computing — `scientific`

*scientific computing, GPU, quantitative*

Also state:
- the validation reference: an analytical solution, a known dataset;
- the reproducibility requirement — seeds, a pinned environment;
- reference hardware for performance measurements;
- for quantitative work: transaction costs and the biases to avoid in a
  backtest.

---

## On the French version

Briefs are published in the language of the challenge. The structure above
translates without adaptation. The numbered examples do not translate — they
are recalculated for the context in question.
