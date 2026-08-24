# Leadership review grids

The machine-readable half lives in `review_grids`, seeded by migration
`0464_leadership_practice_data.sql`. A seeded challenge copies the grid of its
family as its rubric, so a submission is read against criteria its author could
read first.

This file says what the grids are for and how to read a score.

---

## Five families for six trades

| Family | Capability | Trades | Reads |
|---|---|---|---|
| `delivery` | `leadership_reviewer:delivery` | lead-product, lead-project | roadmaps, specs, delivery plans, retrospectives |
| `technical` | `leadership_reviewer:technical` | lead-tech | decision records, technical strategy |
| `people` | `leadership_reviewer:people` | lead-people | ladders, hiring loops, health audits |
| `community` | `leadership_reviewer:community` | lead-community | community strategy, programmes, playbooks |
| `teaching` | `leadership_reviewer:teaching` | lead-mentor | curricula, cohort outcome reports |

`delivery` covers two trades because the competence is one: reading whether a
sequence of commitments survives its own assumptions. A roadmap and a delivery
plan differ in horizon, not in what makes them good.

`people` and `teaching` are kept apart although both are about growing people.
A career ladder is a contract between an organisation and its staff; a
curriculum is a sequence of things somebody has to be able to do. Reading the
first well says nothing about reading the second.

## The three refusals

At the top of the domain grid. Not criteria to score — nothing in the rest of a
submission compensates.

1. **Somebody identifiable who did not agree to be.** Including by a detail
   only they would have: the one person who joined in March, the only designer
   on the team, the customer named in an example.
2. **Nothing given up.** A plan that pursues everything has decided nothing.
3. **A claim about people with no way of checking it.** This is the domain
   where unfalsifiable claims are easiest to make, and the refusal is what
   keeps the attestations worth anything.

## How a score is read

Each criterion is marked out of five, and `craft_score_weights` counts the
average from **3** (`review_grid_average`, `offset_scaled`, baseline 3.0).

Three is a reviewer saying "this is fine" and is worth nothing. Points come
from being better than fine, and below three costs points — a domain where
nothing can lower a score is one where the review is decorative.

Somebody nobody has reviewed has the term skipped rather than scored zero.

## The criterion the whole domain turns on

**Could somebody who was not in the room act on this?**

It appears in the domain grid and, worded for the family, in all five. It is
the question the trade exists to answer, and the most common reason a first
submission comes back: the author knows what they meant, and the document does
not say it.

## Revision rounds

Four, in `revision_round_limits`, with six kinds in `revision_round_kinds`:

| Kind | What it means |
|---|---|
| `leadership_alternatives_thin` | One option presented as a decision. The most common round on a decision record, and the one that changes the outcome most often |
| `leadership_rationale_missing` | A choice made and the reason not written down |
| `leadership_prioritisation_disputed` | Settled by naming what is given up, not by reordering until objections stop |
| `leadership_actions_vague` | Sentiments rather than items with an owner and a date |
| `leadership_redaction_incomplete` | The document still identifies somebody. The only round that blocks publication outright |
| `leadership_measurement_missing` | Nothing will move if it works — or worse, nothing will move if it does not |

A round is closed by the person who **asked** for the change.

## Confirming a redaction is not reviewing

They are separate acts and the guards differ.

**Reviewing** an artefact needs the capability for its family. **Confirming a
redaction** needs any leadership review capability, because what is being
confirmed is not domain expertise — it is that a careful reader could not work
out who this is about.

Both refuse the author. A self-confirmed redaction is a tickbox, and the whole
value of the state is that a second person looked.

Reviewers doing this should know what they are being trusted with: it is done
on behalf of people who are not on this platform and did not ask to be written
about. When in doubt, send it back. `leadership_redaction_incomplete` costs the
author a round; getting it wrong costs somebody else something they cannot take
back.

## Becoming a reviewer

Through `validator_applications`, per family. What is looked at is whether the
applicant's own submissions show the three refusals applied to their own work
— somebody who has never written down what they were giving up will not ask
others to.
