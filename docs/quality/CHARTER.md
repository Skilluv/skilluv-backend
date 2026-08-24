# Skilluv Quality — charter

Published at `skill-uv.com/quality/charter`.

> This document is written in English. The charters that came before it are in
> French with an `.en.md` translation alongside; this repository's default is
> now English, and new documents are written that way. A French translation
> lives in `CHARTER.fr.md`.

---

## 1. Quality is a trade, not a stage

The most common thing said about testing is that it happens *after*. After the
feature, after the design, after the build — a gate somebody walks through on
the way to shipping.

That description produces exactly one kind of quality work: finding out too
late. Every decision worth influencing has already been made, and the only
remaining move is to object.

On Skilluv, quality is a trade with its own artefacts, its own review, its own
scale and its own proof. A test plan written before a feature exists is a
quality deliverable. A strategy that says what an organisation will not test is
a quality deliverable. A defect report precise enough for a stranger to
reproduce is a quality deliverable. None of them is a stage in somebody else's
process.

## 2. What we accept as proof

The same rule as every other domain, applied to what this one produces: **an
artefact somebody else can act on without its author in the room.**

Concretely, one of these:

- A **test plan** or **strategy** that says what is covered, what is not, and
  what risk that accepts.
- A **test suite** another team runs in its own pipeline.
- A **defect report** whose reproduction a stranger can follow.
- A **usability study** or **accessibility audit** with a protocol, real
  sessions or a named standard, and findings kept apart from inferences.
- A **playtest report** that turns sessions into decisions a game team could
  take.
- A **coverage analysis** that ranks gaps by risk rather than by size.

What we do not accept: a coverage percentage with no report, a scanner export
with no triage, "I tested it", or a certification with no work behind it.

## 3. The defect report is the domain's signature, and it is not finished
when it is written

A defect report becomes a Skilluv proof when three things are true:

1. Somebody other than the author reproduced it, or could have.
2. A fix shipped.
3. **The person who found it went back and checked.**

The third one is the part nobody does, and it is the part we record. A merged
pull request is somebody else's claim that the problem is gone. Going back to
look is the only thing that turns it into a fact, and it is the only
attestation on this platform whose condition cannot be satisfied by working
harder alone.

## 4. Severity is argued, not asserted

A severity is a claim about what a user loses. It is not a tool score, and it
is not a feeling about how annoying the defect was.

Reporters state a severity. Reviewers may state a different one, and when they
do, **both are kept**. We do not overwrite the reporter's figure, because a
pattern of consistently over-rating is information a mentor should be able to
see — and because a scale nobody can be wrong on is not a scale.

The craft score reads the reviewed figure and nothing else.

## 5. Consent is not paperwork

Two of the five quality trades work with other people rather than with
systems.

- **No recording without written consent.** Not "they said it was fine". Not
  "it is only for internal use".
- **No participant identified in a report.** Anonymise, always.
- **Recordings go to the client, never to a portfolio.** The person consented
  to one use of their session. A showreel is not that use.

A study run without consent is refused whatever it found. This is the one
refusal in this domain that no amount of quality elsewhere compensates for.

## 6. Testing somebody else's system requires their agreement

For security testing, the scope is written and signed **before** anything is
touched. Rules of engagement are not a legal formality bolted onto the trade;
they are the discipline the trade rests on. Somebody who cannot bound their own
scope cannot be trusted with anybody's system.

For exploratory testing and playtesting, the author is asked. "It was public"
is not permission, and neither is "I did not break anything".

Findings that turn out to be exploitable stop being quality work and become a
disclosure. They go through the address in `SECURITY.md`, on the timeline
agreed with the affected party, and not into a public write-up.

## 7. What was not tested is part of the report

Every artefact in this domain states its holes.

A report that lets a reader assume full coverage is more dangerous than no
report at all: it converts an absence of evidence into a false sense of
evidence, and somebody ships on it. Saying "not checked" costs a line and saves
a release.

This is a criterion in every review grid, and the most common reason a
submission comes back.

## 8. Attribution

Every defect report, study and audit has a named author, and that name travels
with it. A finding used by a team is credited the way a merged contribution is.

Where a mission's terms prevent naming the client, the attestation says what
kind of system it was, what was found and at what scale — and not who. The
skill is demonstrable without breaching the engagement.

## 9. AI

Declared, and allowed.

Using an assistant to draft a test plan, generate cases or summarise sessions
is normal work. Hiding it is not. The declaration is a field on the submission
and the reviewer reads it as context, not as a mark against.

What an assistant cannot do is the thing this domain exists for: it cannot go
back and check that the fix worked, it cannot sit with a participant, and it
cannot decide what an organisation is willing not to test. Those are the parts
that carry the attestation.

## 10. Five trades, one domain

`qa-code`, `qa-cyber`, `qa-design`, `qa-game`, `qa-lead`.

They share a charter and share almost nothing else. Somebody who can judge a
Playwright suite cannot judge a usability protocol, and pretending otherwise
would mean routing work to reviewers who cannot read it. Review capability is
granted per family, and a grant in one family reaches no other.

What holds the domain together is not the technique. It is the question: **what
would have to be true for this to be wrong, and did anybody check?**
