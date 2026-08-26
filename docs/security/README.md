# Security on Skilluv

Everything this platform offers somebody who works in security, in one page.
Written for a person deciding whether to spend an evening here.

## What this is

Five trades, one rank, and proof that can be checked by a stranger.

Skilluv is a compagnonnage platform: you do real work, somebody who knows the
trade reads it, and what comes out is an attestation with a verification code
rather than a certificate with your name in a serif font. The security domain
works exactly like the other nine — same rank, same badges, same craft score —
which is the point: a person who audits code one month and ships a feature the
next has one profile, not two.

## The five trades

| Orientation | What it is | Judged on |
|---|---|---|
| `security-red-team` | Attacking a system you have written permission to attack | Whether the exploit replays |
| `security-blue-team` | Noticing, and being able to say how | Whether the conclusion follows from the artefact |
| `security-code-audit` | Reading code for the defects that get exploited | The findings a scanner did not produce |
| `security-governance` | Writing down what an organisation does about risk | Whether an auditor would accept it |
| `security-purple-team` | Attack and defence as one exercise | Whether a detection exists that did not before |

Pick up to three at `PUT /api/users/me/orientations`. Each has its own first
month written out — `GET /api/domains/security/guides` or the onboarding wizard
at `GET /api/users/me/domain-profile/security/questions`.

## Where to practise

Nothing here requires anybody's permission except the last row, which requires
ours and has it in writing.

| Ground | What for | Cost |
|---|---|---|
| **PortSwigger Web Security Academy** | The reference free curriculum for web security | Free, labs included |
| **OWASP Juice Shop / WebGoat / DVWA** | Applications built to be broken | Free, one container |
| **VulnHub / retired HackTheBox machines** | Machines with no instructions | Free / subscription |
| **Malware Traffic Analysis, SecRepo, The DFIR Report** | Real captures, logs and memory images | Free |
| **Atomic Red Team** | Running a technique on purpose to see if you detect it | Free, needs a disposable environment |
| **`staging.skill-uv.com`** | This platform, in scope, with a safe harbour | Free — read `SCOPE.md` first |

The full toolkit with install notes and what each free tier actually does:
`GET /api/domains/security/toolkit`, or `docs/security/TOOLKIT.md`.

## What you can earn

**Attestations** — seventeen bases, each naming what it rests on. A confirmed
finding, a published disclosure, a captured flag, a reviewed walkthrough, an
audit delivered, a policy validated, a purple exercise run, a paid engagement
completed. Every one has a verification code a recruiter can check without an
account.

**A rank**, the same one every domain feeds. A confirmed vulnerability counts
towards it exactly as a merged pull request does — that is what one
cross-domain rank means, and it is why `deliverables` grew a
`security_finding_id` rather than the domain growing its own counter.

**A craft score** for the domain, computed on read from weights that are rows
in the database. You can read the formula, and you can argue with it. One
confirmed critical finding outweighs twenty solved capture-the-flag challenges,
and that ratio is visible in the numbers rather than asserted in a charter.

**Badges** — twenty-nine, including one nothing can measure
(`security-restraint`: reached the edge of the authorised scope, stopped, and
said so in the report).

**Nothing for a certification you paste in.** Declare it — it shows on your
profile marked *declared* until somebody opens the issuer's page — and it moves
no score.

## The disclosure programme

This platform is a target, on purpose, with a written safe harbour.

- What is in scope, and what is not: `docs/security/SCOPE.md`
- What happens to a report, and when: `docs/security/DISCLOSURE-POLICY.md`
- How to test without being rate-limited: `docs/security/RESEARCH-MODE.md`
- Who has found what: `GET /api/security/hall-of-fame`

Reports go to `POST /api/security/reports`. Triage is a commitment of seven
days with a written reason either way, and there is no money — this platform has
no revenue, and saying so is better than letting anybody hope.

## Paid work

There is a mission board. A security mission carries rules of engagement — a
constraint, not a suggestion: an offensive engagement cannot leave draft
without them — and usually a confidentiality agreement, which is signed here
with the hash of the exact text recorded.

`GET /api/missions?skill_domain=security`. Read `docs/security/LEGAL.md` before
signing anything, particularly the part where it says no lawyer has reviewed
our agreement templates yet.

## Competitions

Five formats: jeopardy CTF, attack and defence, bug bash, purple exercise, code
audit rally. `GET /api/tournaments?skill_domain=security`. If you want to run
one: `docs/security/COMPETITIONS-PLAYBOOK.md`.

## Reviewing

Somebody has to read what other people submit, and this domain needs more of
them than any other because a finding cannot be graded by a machine.

- `security_triager` — reads the incoming queue and decides what is worth a
  reviewer's afternoon. High volume, mostly refusals, and the job that keeps
  the programme alive.
- `security_reviewer:{family}` — reproduces, confirms, argues severity. One per
  trade.

How to be granted one: `docs/security/REVIEWER-ONBOARDING.md`. It asks for
evidence of having done the work, and not for a certification.

## If you are starting from nothing

`docs/security/CURRICULUM.md` — twelve weeks, about ten hours a week, free
throughout, ending with one confirmed finding and one write-up somebody else
can read. It is the shortest honest path from zero to a profile that means
something.

## The documents

| File | What it is for |
|---|---|
| `SCOPE.md` | What you may attack. Read before touching anything. |
| `DISCLOSURE-POLICY.md` | What happens to a report, with the clocks. |
| `RESEARCH-MODE.md` | Testing without fighting the rate limiter. |
| `CHARTER.md` | What this domain refuses to become. |
| `REVIEW-GRIDS.md` | What a reviewer reads for, per trade. |
| `REVIEWER-ONBOARDING.md` | How review rights are granted. |
| `TOOLKIT.md` | Tools and ranges, with what the free tier does. |
| `CURRICULUM.md` | Twelve weeks from nothing. |
| `COMPETITIONS-PLAYBOOK.md` | Running one without it going wrong. |
| `CTF-AUTHORING.md` | Writing a challenge somebody can pass. |
| `RANGES.md` | Hosting the training targets. Operators. |
| `LEGAL.md` | Agreements, IP, insurance, tax. What is drafted and what is reviewed. |
| `DISCORD-STRUCTURE.md` | The channels and who is in them. |
| `IDE-EXTENSION.md` | An editor extension: what it would do, and why not yet. |

At the repository root: `SECURITY.md` (the policy), `PRIVACY.md`,
`THREAT_MODEL.md`, `INCIDENT_RESPONSE.md` — all four are also audit exercises
in the catalogue, because a platform that publishes them and does not want them
read is a platform publishing decoration.
