# What a reviewer reads for

Six grids: one per trade, plus a floor that applies when a submission reaches
review without a family. Six criteria each, because the other nine domains
settled on six and a grid twice as long as its neighbours gets skimmed.

**These are published before anybody submits.** They are rows in `review_grids`
and readable at `GET /api/security/reference` — that is the authoritative copy,
and this page is the readable one.

What is deliberately absent from all of them: tool inventories. A finding is not
better because it came out of a commercial proxy, and a detection is not better
because the SIEM was expensive.

---

## The floor — any security work

Read when a submission arrives without a family, or in a trade added later.

| Criterion | What it looks like |
|---|---|
| **There was permission** | The target was in a written scope before anything was sent to it. A finding on something out of scope is refused however real it is — this is the whole difference between the trade and the offence it resembles. |
| **A stranger reaches the same result** | Steps precise enough that a reviewer who has never seen the system gets there. "I fuzzed it and it broke" is a story. |
| **Impact is stated, not implied** | What an attacker could do on *this* system — not the worst thing the vulnerability class has caused elsewhere. |
| **Nothing was taken that was not needed** | Enough data to prove it and no more, nothing persistent left behind, nothing broken to show it could be. |
| **Uncertainty is written down** | What was not checked, what could not be reproduced, what rests on an assumption. A report with no unknowns has usually stopped looking. |
| **AI use is declared** | A model used to draft, explain or generate a payload is named. Accepted; a report whose reproduction nobody ran is not. |

---

## Red team — offensive work

| Criterion | What it looks like |
|---|---|
| **The exploit replays** | The reviewer follows the steps and sees the same thing. Binary, and checked first: nothing else matters if it fails. |
| **The proof proves the claim** | The evidence shows the specific consequence claimed, not an error page. |
| **Severity is argued from a vector** | A CVSS vector with defensible choices, or an explicit argument why it understates. A bare adjective is not a severity. |
| **The root cause is named** | Which check is missing and where — not which request returns the wrong thing. |
| **Scope was respected under pressure** | No pivot to what was interesting but out of scope, no denial of service, no third-party account. Where the boundary was reached, the report says so. |
| **Readable by whoever has to fix it** | A developer with no offensive background can follow it to the line. |

---

## Blue team — defensive work

| Criterion | What it looks like |
|---|---|
| **The conclusion follows from the artefact** | Every claim points at a line, a packet or an offset in the material. An analysis needing knowledge the artefact lacks has guessed. |
| **The detection fires, and stays quiet** | Triggers on the sample; silent on ordinary traffic. **Both halves shown** — a rule tested only on the positive case is a hypothesis. |
| **The timeline is ordered and sourced** | Events in sequence, each with its source, timezone stated once, clock skew called out. |
| **Observation apart from inference** | "The account authenticated from this address" and "the account was compromised" are different sentences, and the second says what makes it likely. |
| **It says what to do next** | Containment, eradication, and the control that would have caught it earlier. |
| **Sensitive content is handled** | Credentials and personal data in the artefact are redacted in the write-up. Indicators stay; payload contents need not. |

---

## Code security — reading code

| Criterion | What it looks like |
|---|---|
| **The path is traced end to end** | Entry point to sink, through every layer meant to sanitise, with file and line at each step. |
| **Reachability is established** | The configuration and flags that make this code run. A vulnerable function nothing calls is documented as such rather than counted. |
| **The fix is proposed and would work** | A concrete change at the right layer, closing the class. "Sanitise the input" is not a fix; the parameterised query is. |
| **False positives are stated** | What the tool flagged and why it was dismissed. An audit reporting only hits gives no way to judge how carefully it read. |
| **Dependencies are judged, not listed** | An advisory is a finding only when the path is reachable from this project. Pasted version tables fail. |
| **Nothing sensitive is republished** | Secrets found in code or history are reported privately and redacted. An audit that publishes a live key caused the incident it was looking for. |

---

## Governance — documents and controls

| Criterion | What it looks like |
|---|---|
| **It maps to a named requirement** | Each control cites the article, control number or criterion it answers. |
| **It describes what is actually done** | Including where the practice is worse than the aspiration. A policy nobody follows is a liability. |
| **Evidence exists and is reproducible** | For each claim: what an auditor would be shown, where it comes from, how it is produced again next year. |
| **Risk is assessed with a method** | A stated scale applied consistently. Two assessors should land in the same place. |
| **Residual risk is accepted by somebody** | What is not being fixed, why, and who decided. An unowned acceptance is how a finding survives three audits. |
| **It is possible to comply with** | Workable on an ordinary day. A control requiring heroism is bypassed and then documented as met. |

---

## Purple team — exercises

| Criterion | What it looks like |
|---|---|
| **A detection exists that did not before** | A rule or control committed somewhere, naming the technique it covers. An exercise whose output is a slide deck has not finished. |
| **Techniques named in a shared vocabulary** | ATT&CK identifiers or an equally explicit taxonomy. "We tried some lateral movement" maps to nothing. |
| **The detection was validated by re-running** | Execute, observe the alert, show both. Writing the rule and asserting it would fire is the failure this line exists for. |
| **Both sides are in the record** | What the attack did and what the defence saw, on one timeline, including the steps nothing saw. |
| **The exercise was reversible** | A stop condition, verified cleanup, no artefact left behind. Tooling that leaves persistence has created a real incident. |
| **The gaps are ranked** | Which blind spots matter most here, and what closing each would cost. |

---

## How a grid is used

A reviewer scores each criterion and the average lands in `review_grid_scores`,
which feeds the domain's craft score — `review_grid_average`, counted from 3 out
of 5, and **skipped rather than zeroed** for somebody nobody has reviewed.

A score without a comment is worth very little, and a reviewer who disagrees
with a grid should say so rather than mark around it. The grids are rows and can
be changed; a criterion nobody can defend is a criterion to argue about.
