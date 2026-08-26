# Becoming a reviewer or a triager

This domain needs more reviewers than any other, because a finding cannot be
graded by a machine. It also needs them to be right, because a wrong
confirmation is a public claim about somebody else's system.

## The two jobs, and why they are separate

**Triage** — `security_triager`. You read the incoming queue and decide what is
worth a reviewer's afternoon. Thirty reports a week, most of them not
vulnerabilities: scanner output, out-of-scope hosts, missing headers with no
impact, reports nobody could follow. You refuse them with a reason, and you pass
on the ones that are worth reproducing.

You may **not** confirm a finding. Triage decides whether something deserves an
hour; confirming asserts publicly that a vulnerability is real.

**Review** — `security_reviewer:{family}`, one per trade. You reproduce, you
confirm or refuse on the merits, you argue severity, you open rounds. Your name
is on the confirmation.

They are separate capabilities on purpose. Tying triage to the reviewer ladder
would mean either handing thirty refusals a week to people who should be
reproducing findings, or refusing triage to people who are good at it and are
not senior enough to judge a vulnerability. Both are worse than two grants.

## What is asked

### For `security_triager`

Evidence that you can tell a report from a complaint. In practice:

- **Ten reports of your own**, filed anywhere — here, HackerOne, a project's
  issue tracker — of which some were refused, with the refusals. The refusals
  matter more than the acceptances: somebody who has been told why their report
  was thin knows what thin looks like.
- Or **experience triaging** in a programme, described concretely: roughly how
  many a week, the most common reason for refusal, and one you got wrong.

No certification. No years-of-experience floor.

### For `security_reviewer:{family}`

Evidence that you can judge work in that family, which is different per trade:

| Family | What is asked |
|---|---|
| `red-team` | Five confirmed findings, anywhere, at least one high or critical, with the reports readable |
| `blue-team` | Two analyses of real artefacts, each ending in a detection with its false-positive figure |
| `code-audit` | One audit of a real codebase, with the paths traced and the dismissed scanner output |
| `governance` | One document that was audited against a framework, and how the audit went |
| `purple-team` | One exercise run to a written outcome, with the coverage statement including the invisible techniques |

Work done on this platform counts and is easiest to check. Work done elsewhere
counts and needs a link.

### What is not asked

An OSCP, a CISSP, a CEH, a degree, or five years. If you hold one, say so — it
is a real signal and it is not the gate. The gate is work somebody can read.

## How to apply

`POST /api/validators/apply` with `domain=security` and, for a reviewer, the
family. The form asks for the evidence above as links.

An administrator decides. You will get a reason either way, because that is
what this domain asks of its reviewers and it would be strange to exempt the
decision that appoints them.

## What you are agreeing to

### Read the grid first

There is one per family, public before anybody submits
(`docs/security/REVIEW-GRIDS.md`). Read the submission against it, in order.
Every grid has at least one line that is a **check** rather than a judgement —
does it replay, does the query fire, does the path hold — and doing that check
is most of the job.

### Write the reason

Every refusal carries one, enforced by the database. Two sentences is enough;
"not applicable" is not. A reporter who is told no and not why files the same
report next week, and is right to.

### Do not review what you are too close to

If you reported it, you do not confirm it. If it is in code you wrote, say so
and pass it on. The platform does not check this — it cannot — and it is the one
thing that makes the whole record worth anything.

### Argue severity on the vector

If you disagree with a reporter's severity, change it and say which metric you
disagree about. The database requires a reason of at least twenty characters and
keeps their original figure alongside yours, so the disagreement stays readable.
An unexplained downgrade is the thing researchers leave a platform over.

### Open a proof file carefully

Proof uploads are refused if they look executable — on the extension and on the
first bytes — and **nothing is scanned for malware**. That is stated plainly
rather than implied.

Treat a proof file the way you would treat anything that arrived from a
stranger: open it in a virtual machine you can throw away, or on a machine with
nothing on it. A capture and a log file are text and are nearly always fine; a
PDF and a memory image are not text.

### Keep the embargo

You will read details of unfixed vulnerabilities in live systems, including
this one. They do not leave the review queue — not in a talk, not in a blog
post, not in a private message to a friend who works there. The embargo is a
promise the platform made the reporter, and you are the person keeping it.

## What you get

`security_reviewer:{family}` counts towards the reviewer badges and the review
grids you fill in feed the domain's craft scores. Reviewing is also the fastest
way to get good at reporting, which is the honest reason most people who do it
keep doing it.

## How a grant ends

You can hand it back at any time. It is revoked if you confirm something you
reported, if you disclose something under embargo, or if the reasons you write
stop being reasons. Each of those is a decision by an administrator, recorded
with its own reason — the same standard as everything else here.
