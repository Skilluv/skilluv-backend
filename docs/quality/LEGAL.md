# Quality — legal and confidentiality

Backlog: quality/L-01. Shares a counsel review with the security, design, game
and leadership charters — the questions overlap and paying four times to be
told the same thing is not a budget this project has.

Nothing here is legal advice. It is the position the platform takes and the
questions counsel has to answer before paid quality missions open.

---

## 1. The four situations that differ from every other domain

| Situation | Why it is not like the others |
|---|---|
| A usability study | Third parties' personal data, collected by the contributor, about a client's product |
| A penetration test | Access to a system, authorised in writing, with a disclosure clock |
| A defect found under NDA | Real, valuable, and unpublishable until the vendor ships |
| A playtest of an unreleased game | Participants see something under embargo |

No other Skilluv domain has a contributor holding somebody else's users'
personal data. That is the one that decides the shape of the terms.

## 2. Personal data in research

**Position:** the participant's data belongs to the participant, the client is
the controller, and the contributor is a processor acting on written
instructions.

Practically:

- Consent is written, obtained before recording, and states who will see the
  recording and for how long it is kept.
- Recordings are delivered to the client and deleted by the contributor at the
  end of the engagement. `session_recordings` exists as a mission deliverable
  format precisely so this is a handover with a date on it.
- The report is anonymised. Participant 3, not a name, not a job title
  specific enough to identify one person in a company of forty.
- Nothing from a session goes into a portfolio. Not a clip, not a quote with a
  name, not a screen recording. The person consented to one use.

**For counsel:** the processor agreement wording, and whether a contributor
based outside the EU handling EU participants' recordings needs anything
beyond the client's own transfer mechanism.

## 3. Rules of engagement for security testing

**Position:** no test without a signed scope, and the signature has to come
from somebody with authority over the system.

The template is `quality-template-rules-of-engagement` (migration 0457), and it
covers: parties, written authorisation, in scope, explicitly out of scope,
window, techniques not permitted, what happens if something breaks, handling of
data accessed accidentally, and the disclosure timeline.

Two clauses that matter more than they look:

- **Data accessed accidentally.** The tester stops, records the fact, and does
  not download. Written down in advance, because the decision cannot be made
  well in the moment.
- **What is never publishable.** Some findings stay private permanently. Saying
  so up front avoids a fight at the end.

**For counsel:** whether an authorisation signed by a client covers a system the
client does not own — a hosted service, a third-party integration — and what
the tester has to verify before starting.

## 4. Defects found under NDA

**Position:** the finding belongs to the engagement. It is not published, not
summarised on a blog, and not used as a portfolio piece until the vendor has
shipped a fix and agreed a date.

The attestation still works. `quality_bug_report_validated` can be issued
against an anonymised record: what kind of system, what class of defect, what
scale, and that a fix shipped. What it does not carry is the client, the
reproduction, or the product.

That is the same call the security and leadership domains make, and it is the
only way a contributor working under NDA can build a public record at all.

**For counsel:** the boundary of the anonymised claim — how much can be said
about a defect before the description identifies the client.

## 5. Intellectual property in test artefacts

**Position:** a test plan, a suite or a report authored under a paid mission
belongs to the client, on delivery and on payment.

The contributor keeps:

- the right to state that the work happened, at the level of abstraction in
  section 4;
- the right to reuse **technique** — a fixture pattern, a page object
  structure, a triage workflow. A method is not a deliverable.

The contributor does not keep the right to reuse the suite itself, or the
report, or anything naming the client's system.

**For counsel:** whether "technique" survives contact with a client who
considers their internal test architecture confidential, and how to word the
carve-out.

## 6. Non-solicitation and conflict

A quality contributor sees a client's defect backlog, which is a map of what is
weak. Two consequences:

- **No testing two direct competitors within the same quarter** without both
  being told. The declaration is on the contributor.
- **The backlog is not intelligence.** What was seen on one engagement is not
  brought to the next, in either direction.

## 7. Participant compensation

Not a legal question so much as a commercial one that keeps being treated as an
afterthought.

**Position:** participant compensation is billed separately from the mission
fee and is stated in the brief. A study priced as though five people will give
an hour for free is a study that will be run on five colleagues, and section 2
of the review grid refuses those.

## 8. Skilluv's own surfaces

Contributors testing Skilluv itself (migration 0459) work on public surfaces,
from their own environments, against our public repositories. No production
access, no production data, no credentials.

Anything that turns out to be exploitable leaves the quality path immediately
and goes through `SECURITY.md`. It is not written up in public, and doing so is
the one thing that costs a contributor their standing here rather than a
revision round.

---

## What counsel is being asked for

1. Processor wording for research recordings, in a cross-border case.
2. Whether a client's authorisation covers third-party systems in scope.
3. The wording of the anonymised attestation, so it says something true without
   identifying anybody.
4. The IP carve-out for technique.
5. Redlines on the rules-of-engagement template.

Shared session with the security, design, game and leadership charters.
