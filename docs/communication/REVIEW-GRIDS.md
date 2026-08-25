# Communication review grids

The machine-readable half of this document lives in `review_grids`, seeded by
migration `0503_communication_review_grids.sql`. A seeded challenge copies the
grid of its family as its rubric, so a submission is read against criteria its
author could read first.

This file says what the grids are *for* and how to read a score. The criteria
themselves are in the database, because they are edited by people who do not
deploy.

---

## Four families for five trades

| Family | Capability | Reads |
|---|---|---|
| `documentation` | `communication_reviewer:documentation` | pages, tutorials, references, changelogs |
| `advocacy` | `communication_reviewer:advocacy` | talks, videos, articles, streams, episodes |
| `translation` | `communication_reviewer:translation` | translations, glossaries, style guides |
| `research-writing` | `communication_reviewer:research-writing` | whitepapers, reports, external specifications |

Five trades, four families, and the merge is deliberate: the developer
advocate and the technical content creator are read by the same person. What a
reviewer of that family has to be able to do is judge whether a promise made
in a title was kept in front of an audience that could have left, and that
skill does not change between a conference room and a video.

The two that do not merge are the interesting ones. A documentation reviewer
is not automatically a translation reviewer, because reading a translation
means reading two languages and holding a vocabulary steady across both.
Research writing does not merge into documentation, because what is being
judged there is a method rather than a page.

`communication_reviewer:all` reaches every family. It is granted sparingly,
and holding it is not the same as being able to judge everything — it means
being trusted to know which submissions to leave to somebody else.

---

## The common criteria, and why they are separate

`review_grids` holds a fifth row with no `reviewer_group`: the criteria every
communication delivery is read against whatever its family. They are applied
in addition to the family grid, not instead of it.

They are separate because they are the ones that refuse a delivery outright.
An unattributed paragraph, an undeclared paid partnership, an example nobody
ran: none of these is a lower score, and treating them as criteria among
others would let a strong family score carry them.

The seven:

1. **Service to the reader** — somebody arrived with a question and left with
   an answer.
2. **Technical accuracy** — every claim checkable, every example executed.
3. **Structure** — the reader always knows where they are.
4. **Level announced and held** — no unannounced jump.
5. **Attribution and sources** — including the sponsorship declaration, at the
   top.
6. **Accessibility** — alt text, contrast, captions, transcript.
7. **Transparency about AI** — declared use is accepted; unverified output is
   not.

---

## How to read a score

A grid is not a mark out of ten. Each criterion is read against what the
`looks_like` text describes, and a reviewer says what they saw. A submission
that meets every criterion but one is not "almost validated" — it is a
submission with one thing to fix, named, with the fix usually smaller than the
author feared.

The score exists to make a review arguable. A contributor who can read why
their work came back can tell us when the reason was wrong, and that has
happened and should keep happening.

---

## The one grid that is stricter

Research writing is set to `human_verified`, and it is the only family in this
domain that is. A reference invented by a language model is indistinguishable
from a real one to a reader who trusts the document, and the whole value of
research writing is that its sources can be followed.

A reviewer of that family is expected to open links. Not all of them, every
time — enough of them, unpredictably, that a document with fabricated sources
does not survive review.
