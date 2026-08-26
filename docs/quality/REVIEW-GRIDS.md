# Quality review grids

The machine-readable half of this document lives in `review_grids`, seeded by
migration `0452_quality_practice_data.sql`. A seeded challenge copies the grid
of its family as its rubric, so a submission is read against criteria its
author could read first.

This file says what the grids are *for* and how to read a score. The criteria
themselves are in the database, because they are edited by people who do not
deploy.

---

## Five families, and why they do not collapse

| Family | Capability | Reads |
|---|---|---|
| `automation` | `quality_reviewer:automation` | test suites, plans, coverage |
| `intrusion` | `quality_reviewer:intrusion` | penetration reports, scanner triage |
| `usability` | `quality_reviewer:usability` | study protocols, WCAG audits |
| `playtest` | `quality_reviewer:playtest` | session records, balance datasets |
| `strategy` | `quality_reviewer:strategy` | team strategies, quality initiatives |

Ops has eight trades in five families. Code has more than thirty in eight.
Here five trades produce five families, which looks like a modelling failure
and is not: the competence is defined by what the reviewer has to be able to
*open*, and no two of these are opened by the same person.

`quality_reviewer:all` reaches every family. It is granted sparingly, and
holding it is not the same as being able to judge everything — it means being
trusted to know which reports to leave to somebody else.

## The three refusals

At the top of the domain grid, and they are not criteria to score. Nothing in
the rest of a submission compensates for them.

1. **A finding nobody else can reproduce.** However real it is.
2. **A figure with no source.** A coverage number without its report, a
   duration without its measurement, a rate without its denominator.
3. **A session run without written consent.** Whatever it found.

A reviewer marking one of these does not score the rest. The submission comes
back, and the round is `quality_repro_insufficient`,
`quality_evidence_missing` or a refusal.

## How a score is read

Each criterion is marked out of five. The average lands in
`review_grid_scores.average`, and `craft_score_weights` counts it from **3**,
not from 0 (`review_grid_average`, `offset_scaled`, baseline 3.0).

That means three out of five is worth nothing and is not a failure — it is the
line where a reviewer is saying "this is fine". Points come from being better
than fine, and a submission below three costs points, which is deliberate: a
domain where nothing can lower a score is a domain where the review is
decorative.

Somebody nobody has reviewed has the term skipped entirely rather than scored
zero. Counting an unreviewed person as zero would subtract the whole baseline
from their total, which would say something false about them.

## The criterion every grid shares

**What was not tested is written down.**

It is in the domain grid and it recurs, worded for the family, in all five.
It is the most common reason a submission comes back and the thing most new
contributors omit — not through dishonesty, but because a report feels finished
when the findings are in it.

A report that lets a reader assume full coverage converts an absence of
evidence into a false sense of evidence. Somebody ships on it. Saying "not
checked" costs a line.

## Revision rounds

Four, set in `revision_round_limits` for this domain, with five named kinds in
`revision_round_kinds`:

- `quality_repro_insufficient` — the most frequent, and the only one that
  invalidates the report if it does not converge.
- `quality_evidence_missing`
- `quality_severity_disputed`
- `quality_coverage_gap`
- `quality_protocol_revision`

Four rather than audio's five. A fourth round on substance usually means the
two people disagree about what the product should do, and that is not a testing
question — it goes back to whoever owns the product.

A round is closed by the person who **asked** for the change, not by the person
who made it. A counter the author can run down alone is not a count both sides
agree on.

## Becoming a reviewer

Through `validator_applications`, in the ordinary way, per family. What is
looked at is not seniority but whether the applicant's own submissions show the
three refusals being applied to their own work — somebody who has never written
down what they did not test will not ask others to.
