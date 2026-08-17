# Code — licences, intellectual property, and AI disclosure

What Skilluv enforces, what it warns about, and where it stops and a lawyer
starts.

**This is not legal advice.** It is the platform's position, written so that
somebody can act on it and so that a lawyer reviewing it has something
specific to correct. Where the platform is unsure, this document says so
rather than choosing a comfortable answer.

---

## 1. Respecting upstream licences

### The accident this exists to prevent

An enterprise publishes a mission: extend this repository, we own the result.
The repository is GPL. The enterprise does not own the result and cannot,
whatever anybody signed. Nobody involved finds out until a lawyer does —
usually after the work is delivered and paid for, and the person who pays for
it is the contractor, because they are the one who promised something they
could not deliver.

This is not a rare edge case. It is the most common legal accident in
commercial open source work.

### What Skilluv enforces

`software_licenses` (migration 0202) records each licence, its category, and
two facts: whether a client can be promised ownership of a derivative work,
and whether the output must be released openly. A mission that names an
upstream licence and promises terms that licence forbids **is refused at
creation**, with the licence's own caveat in the message.

A mission that names no upstream licence is not refused. Most work has no
upstream, and demanding one would block the ordinary case to catch the rare
one.

### The categories

| Category | Means | Examples |
|---|---|---|
| Permissive | Do what you like, keep the notice. | MIT, Apache-2.0, BSD, ISC |
| Weak copyleft | Modified files stay under it; your program does not. | MPL-2.0, LGPL |
| Strong copyleft | The whole derivative work carries the licence. | GPL-2.0, GPL-3.0 |
| Network copyleft | Strong copyleft that also triggers on running it as a service. | AGPL-3.0 |
| Source available | Readable, not freely usable. | BSL, SSPL |
| Proprietary | Only what the contract grants. | — |

### Attribution

Almost every licence here requires it, and it is the obligation forgotten most
often. Apache-2.0 in particular requires the `NOTICE` file to travel with the
distribution — not only the licence text.

**A contribution that strips or omits attribution is refused in review**, and
that is one of the things the review grid's correctness criterion covers.

### Compatibility

Two traps worth naming, because they are the two that catch people:

- **Apache-2.0 into GPLv2** is incompatible. Into GPLv3 it is fine. If your
  deliverable is destined for a GPLv2 project, check before you start.
- **Static linking against LGPL** turns a simple answer into a legal question.
  Dynamic linking does not. If the mission requires a single static binary and
  the dependency is LGPL, raise it before writing code.

---

## 2. Intellectual property on missions

Four terms, stated on the mission before anybody applies, never negotiated
after the work exists — at which point the person who did it has no leverage
left.

| Term | Who owns what |
|---|---|
| `full_ownership_client` | The client owns the delivered code. The usual arrangement. |
| `open_source_output` | The deliverable is released under an open licence. |
| `retain_reusable_components` | The client owns the domain-specific work; the creator keeps the generic pieces they would otherwise rewrite every time. |
| `dual_license` | Delivered to the client and released openly at the same time. |

### On `retain_reusable_components`

This is the term Skilluv recommends by default for anything that is not
strictly domain-specific, and it is the one clients push back on hardest. The
argument for it is simple: a contractor who has to rewrite the same
authentication helper on every mission is being paid to produce waste, and the
client is paying for it.

The line has to be drawn in the mission, not afterwards. "Generic" is not
self-evident.

### What Skilluv does not do

It does not draft your contract. The `ip_terms` field is a statement of intent
that both parties have read; it is not a signed agreement, and for anything
substantial there should be one.

---

## 3. AI-assisted code

### The position: disclosure, not prohibition

Using an assistant is fine. Hiding it is not.

The reasoning is practical rather than moral. A maintainer who does not know
where a contribution came from cannot review it properly — the failure modes
of generated code are different from the failure modes of hand-written code,
and a reviewer who knows which they are looking at reviews better. A client
who does not know cannot judge their own copyright exposure.

### The five levels

Declared on the deliverable:

- `none`
- `autocomplete` — the assistant completed lines you were already writing.
- `pair_programming` — you worked with it, and reviewed everything.
- `generated_then_refactored` — it produced a first version you reworked.
- `generated_as_is` — it produced what was submitted.

None of these is disqualifying. `generated_as_is` on a trivial fix is
reasonable; on a security-sensitive change it is something a reviewer will
want to talk about, which is exactly the conversation this makes possible.

### How it is enforced

Not by a database constraint. The obvious implementation — "a verified
deliverable states a level" — would break the main path, because a merged pull
request is verified by a GitHub webhook and a webhook has nobody to ask.

Instead: when an artefact is verified with nothing declared, the author is
prompted and given fourteen days. Past the deadline with nothing declared, the
artefact **stops counting** — it is not revoked, because somebody on holiday
is not somebody hiding something, but it is not credited towards the craft
score or shown as proof either.

### On missions

For paid work the client's explicit consent is required if the deliverable is
AI-assisted beyond `autocomplete`. This is not Skilluv being cautious on their
behalf: in several jurisdictions the copyright status of substantially
generated code is unsettled, and a client who did not know cannot make an
informed decision about their own exposure.

### On attestations

`code_pr_merged_upstream` and the other artefact attestations carry the
declared level. The attestation is a public statement, and a public statement
that omits how the work was produced is a public statement that is not
complete.

---

## 4. Copyleft on client missions

If a mission's deliverable derives from AGPL code, the client's obligations
follow the code, including when they expose it as a service. This surprises
more people than any other clause in open source licensing, and it always
surprises them late.

**Skilluv refuses the combination at creation**: an AGPL upstream with
`full_ownership_client` is a contradiction the mission cannot honour.

What Skilluv cannot detect is a transitive dependency. A permissively licensed
repository that depends on a GPL library produces a derivative work with GPL
obligations, and no field on a mission records that. Checking the dependency
tree is the contractor's job, and it is worth an hour before accepting.

---

## 5. Compliance frameworks

Some clients will ask for more than a licence check.

- **Finance and banking**: SOC 2, ISO 27001. Usually the client's certification
  rather than yours, but it constrains what you may access and from where.
- **Health**: HIPAA in the United States, GDPR strictly applied in Europe.
  Both restrict handling real data — the usual consequence is that you develop
  against synthetic data and never see production.
- **Public sector**: national data residency rules, which may forbid a
  contractor from accessing the system from outside the country.

Skilluv records none of this today and does not pretend to check it. A mission
with compliance requirements should say so in its description, and a
contractor should read it as a constraint on *where and how they work*, not as
paperwork somebody else handles.

---

## 6. What needs a lawyer

Written down so it is not quietly forgotten. Skilluv's position on each of
these is provisional and should be reviewed:

1. **The enforceability of `retain_reusable_components`** under Beninese and
   French law, and whether the mission's stated terms constitute an agreement
   at all without a signature.
2. **Static linking against LGPL** in the specific case of a delivered binary.
3. **The copyright status of substantially AI-generated code** in the
   jurisdictions Skilluv operates in, and whether the disclosure levels above
   are the right granularity for a client's consent to mean anything.
4. **The Unlicense** and public-domain dedications generally, whose validity
   in civil-law jurisdictions is disputed.
5. **Skilluv's own liability** as an intermediary when a mission's terms turn
   out to be unenforceable — the platform states the terms and refuses the
   obvious contradictions, which may or may not make it a party to the
   arrangement.

Item 5 is the one that matters most to Skilluv and is listed last on purpose:
the first four protect the people using the platform, and they come first.
