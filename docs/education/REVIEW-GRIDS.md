# Education review grids

The machine-readable half of this document lives in `review_grids`, seeded by
migration `0520_education_review_grids.sql`. A seeded challenge copies the grid
of its family as its rubric, so a submission is read against criteria its
author could read first.

This file says what the grids are *for* and how to read a score. The criteria
themselves are in the database, because they are edited by people who do not
deploy.

---

## Two families for three trades

| Family | Capability | Reads |
|---|---|---|
| `teaching` | `education_reviewer:teaching` | sessions, lesson plans, cohorts, post-mortems |
| `curriculum` | `education_reviewer:curriculum` | programmes, rubrics, outcome reports |

Fewer families than trades, and the merge is the one that matters: the
technical trainer and the coding teacher are read by the same person. Both are
judged on whether the room moved, and somebody who can tell that about a
three-hour workshop can tell it about a term.

They do not share an *onboarding* guide, and that difference is worth knowing
about. A review family is drawn by what a reviewer must be able to open; an
onboarding guide is drawn by what a newcomer needs first. Here the two splits
genuinely differ, so there are three onboarding guides and two grids, and both
are right.

Curriculum design does not merge into teaching, because a programme is judged
on whether somebody *else* could run it — a question a reviewer answers by
reading facilitator notes rather than by watching anybody teach.

`education_reviewer:all` reaches both families.

---

## The common criteria, and why they are separate

`review_grids` holds a third row with no `reviewer_group`: the criteria every
education delivery is read against whatever its family. They are applied in
addition to the family grid, not instead of it.

Two of the eight can refuse a delivery on their own:

- **Somebody learned something.** A session everybody enjoyed and nobody can
  act on has failed at the only thing it was for. Satisfaction is reported
  separately and is never offered as evidence.
- **Learner data handled.** Names, faces, marks and messages anonymised or
  carrying explicit consent, and nothing identifiable about a minor at all. A
  delivery that exposes a learner is refused whatever else it does — this is
  not a score, it is a condition of publication.

The other six: objectives stated and met; the level was right; learners did
something; materials somebody else can use; satisfaction read as what it is;
transparency about AI.

---

## What a reviewer looks for that is specific to this domain

**The silent jump.** Module four assumes something module three did not teach.
It is the most common defect here by a distance, and it is invisible to the
author because they already knew the missing thing. A reviewer reads the
sequence as somebody who does not know the subject.

**Evidence that is not a proxy.** Attendance is not learning. Hours taught is
not learning. A satisfaction average is not learning. The criterion asks for a
before and after, a completed project, or a measured assessment — and asks how
it was measured.

**Whether a second person could deliver it.** Timings, what to cut when it
runs long, where people get stuck, solutions, and the environment as it works
on a clean machine. A reviewer who could not run the session from what was
handed in says so, and that is a return rather than a low score.

---

## How to read a score

A grid is not a mark out of ten. Each criterion is read against what the
`looks_like` text describes, and a reviewer says what they saw. A submission
that meets every criterion but one is not "almost validated" — it is a
submission with one thing to fix, named.

The score exists to make a review arguable. Two assessors reading the same
work should reach the same conclusion; where they do not, the criterion was
vague and it is the grid that gets fixed. That is the same standard this
domain asks its own members to hold their rubrics to.
