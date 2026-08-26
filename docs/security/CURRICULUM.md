# Twelve weeks, from nothing

A path from no security experience to one confirmed finding and one write-up a
stranger can read. About ten hours a week. **Everything in it is free** —
if a step needs money, it says so and gives the free alternative.

This is not a course. It is the order in which the free material already out
there is worth doing, plus the four moments where this platform gives you
somebody to read what you produced.

## Before week one

A laptop. Any operating system; Linux or WSL2 makes some steps shorter and none
impossible. Docker installed and `docker --version` answering. Git.

Not Kali. It is convenient and it is not a requirement, and knowing which
package you installed is worth more than having four hundred of them.

Then: read `docs/security/SCOPE.md`. It takes four minutes and it is the
difference between this trade and a criminal offence.

---

## Weeks 1–2 — how the web actually works

**PortSwigger Web Security Academy**, the "Web security basics" material and
the whole **access control** section. Free, labs included, and nothing else
free is this thorough.

Install Burp Suite Community or OWASP ZAP and browse ordinary sites through it
for a week. Not for a tutorial — for familiarity. Most of what a beginner is
missing is not a technique, it is knowing what normal traffic looks like.

**By the end you can:** say what a session cookie is, what an authorisation
check looks like when it is missing, and read a request without squinting.

## Weeks 3–4 — a range, with notes

**OWASP Juice Shop** in one container. Work through the challenges of the
`training_ground` catalogue here in order of tier — they are the same
objectives, described by what has to be achieved.

Write up every solve, badly, in three lines: the request, what came back, and
**which check was missing**. That third line is the whole exercise. "The login
form is vulnerable to SQL injection" is a symptom; "the login query is built by
concatenation and the parameter is not bound" is a finding.

Submit two of those write-ups here. A reviewer will read them and tell you what
is missing. That is the first of the four moments.

**By the end you have:** ten short write-ups and two reviewed.

## Weeks 5–6 — the other side of the same thing

**OWASP WebGoat**, which explains the defect before asking you to exploit it,
and ships its own source. Read the vulnerable code, then the fix.

Then one **defensive lab** here: a real log set or capture, questions, answers
checked against hashes. Download, analyse offline with Wireshark, answer.

Most people skip this fortnight and it is the one that makes the difference. A
red teamer who has never read a log has no idea which of their actions were
loud, and that is half of what a client is paying for.

**By the end you can:** read a capture to a conclusion and say where in it the
conclusion came from.

## Weeks 7–8 — read real code

Install **Semgrep**. Run it on `skilluv-backend` — this platform's source is
public and its authors have asked to be audited.

Then triage every hit: real, unreachable, or false positive, **with the
reason**. This is the exercise that makes a code auditor. A tool that produces
two hundred hits has told you nothing until somebody reads them.

Take one of the three audit exercises in the catalogue (authentication,
authorisation, file handling) and submit it. Second reviewed moment.

**By the end you have:** one audit with its coverage stated, which is a thing
most people with two years of experience have never written.

## Weeks 9–10 — hunt

Pick a piece of `staging.skill-uv.com` inside the published scope. One hour a
day, two weeks.

Most days you will find nothing. **That is the trade**, and it is the part no
course prepares anybody for. Keep a log of what you looked at and ruled out —
it is what stops you looking at the same thing twice, and it is the skeleton of
the report if you find something.

Get a research token first (`docs/security/RESEARCH-MODE.md`) so the rate
limiter is not the thing you are fighting.

If you find nothing in two weeks: submit a **negative report** as a write-up of
what you checked and how. It earns no finding attestation and a reviewer will
still read it, and a person who can say precisely what they ruled out is
employable.

## Week 11 — the report, or the write-up

If you found something: write it up with the finding template. Reproduction
first, impact second, a CVSS vector third, and the section that says what you
did not do.

Then submit it and wait. Triage is a seven-day commitment; a confirmation takes
longer because somebody has to reproduce it.

If you found nothing: take a retired VulnHub machine and write the walkthrough
instead, in your own words, saying which existing write-ups you read and when.

**Third reviewed moment**, and the one that goes on your profile.

## Week 12 — decide what you are

You have now done a bit of all four trades. One of them was less tiring than the
others; that is data.

- Read the review grid for it (`docs/security/REVIEW-GRIDS.md`) and be honest
  about which criteria you would fail.
- Set your orientation. Up to three, and one is a stronger claim than three.
- Read `docs/security/REVIEWER-ONBOARDING.md`, not because you are ready, but
  so you know what "ready" is measured in.
- Ask for a mentor: `POST /api/users/me/mentorship/request-mentor`. Fourth
  moment, and the one that decides whether month four happens.

---

## What you should have at the end

- Ten to fifteen short write-ups, three of them reviewed.
- One code audit with its coverage stated.
- One defensive analysis ending in a detection with a false-positive figure.
- One finding submitted — confirmed, duplicate, or refused with a reason, and
  all three are a result.
- Between two and five attestations with verification codes.
- A rank, probably `ranger`, and a craft score you can explain line by line.

## What you will not have

A certification. This path is not a substitute for one and does not pretend to
be: it is the twelve weeks that make a certification worth taking, or make it
unnecessary, depending on who is hiring.

## If you fall behind

The weeks are not a schedule, they are an order. Somebody doing this in six
months is doing it properly. The only step that genuinely does not compress is
weeks 9–10, because looking for something that might not be there is a skill
that only accrues in real time.
