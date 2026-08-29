---
finding_id: 00000000-0000-0000-0000-000000000000
severity: high            # critical | high | medium | low | informational
cwe: CWE-89
reporter: username        # or "Anonyme #123" — ask, never assume
confirmed_at: 2026-01-01
published_at: 2026-01-01
---

# Title, naming the defect rather than the drama

## TL;DR

Two sentences. What it was, and what it would have allowed. Somebody who reads
only this should come away able to check whether their own code has it.

## Context

The module or endpoint, and what it does for the platform. A reader who has
never seen this codebase needs this paragraph to make sense of the next one.

## The defect

The technical description, with the code as it was. Name the assumption that
turned out to be false — that is the part that transfers to other codebases.

## Reproduction

The steps, written to teach the pattern. Redact anything still live elsewhere:
the fix has shipped here, and the same class of defect may not have shipped
anywhere else.

## Impact

What an attacker could have done, stated plainly and without inflation. A
write-up that oversells its own finding is read once and trusted never again.

## The fix

The code as it is now, with the commit and the pull request.

## What would have caught it earlier

The honest part, and the reason to publish at all:

- what let the defect exist — a missing check, an assumption, a review that
  looked at the wrong thing;
- what now catches the whole class rather than this one instance: a test, a
  lint rule, a guard, a script in CI.

"We will be more careful" is not an answer. Name the mechanism.

## References

The OWASP page, the CWE entry, other public write-ups of the same class.

## Credits

Found by {reporter}. Thank you.
