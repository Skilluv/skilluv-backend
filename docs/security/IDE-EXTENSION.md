# An editor extension

Backlog: security/G-04. Not built, deliberately, and this page is the decision
rather than a placeholder.

## What was proposed

A VS Code extension with a Skilluv sidebar listing your open findings, claimed
challenges and running missions; a command that files a finding from the
selected code; CVE autocompletion; Semgrep results inline with a "submit as a
finding" button; and tooltips on risky patterns linking to a Skilluv write-up.

## Why it is not built

**The API is the extension.** Everything the sidebar would show is one
authenticated GET — `/api/security/reports`, `/api/users/me/next-challenges`,
`/api/users/me/missions` — and everything the commands would do is one POST.
The extension is a client, and writing it before anybody has asked for it is
writing a client for a workflow nobody has yet.

**The valuable half already exists elsewhere.** Semgrep has a VS Code
extension. So does Snyk, so does SonarLint. Reimplementing inline SAST results
to add one button is the expensive part done again for the cheap part.

**It is a second thing to keep in step.** An extension that shows a stale
finding status, or posts to a route that has moved, is worse than no extension:
somebody trusts it.

**Nobody is blocked.** The thing that stops people filing a first finding is not
the number of clicks. It is not knowing what a report should contain, which is
what the templates and the guides are for.

## When it would be worth building

One of these, actually observed rather than predicted:

- People are filing findings and saying the web form is what slows them down.
- Somebody is running a security review inside an editor often enough that the
  round trip to a browser is the friction.
- Somebody outside the team wants to write it. Then the answer is yes, and this
  page becomes the specification.

## What it would need from the platform

Nothing new, which is the point. For the record, if it is built:

| Feature | Endpoint |
|---|---|
| My findings | `GET /api/security/reports` |
| File a finding | `POST /api/security/reports` |
| Attach a proof | `POST /api/security/reports/uploads` |
| Answer a round | `POST /api/security/reports/{id}/answer-round` |
| Suggested challenges | `GET /api/users/me/next-challenges?domain=security` |
| The vocabulary | `GET /api/security/reference` |
| The scope | `GET /api/security/scope` — unauthenticated |

Authentication would be an API key (`/api/users/me/api-keys`) rather than a
session cookie, and it would need a scope narrower than the existing keys have —
which is the one genuine platform change the extension implies, and a reason to
design it when somebody is actually writing one.

## One thing it should not do

Submit a finding from a selection without the person writing a reproduction.
The report shape is what makes a finding usable, and a one-click submission that
skips it would produce exactly the reports the triage queue exists to refuse.

Whatever gets built, the reproduction stays a required field.
