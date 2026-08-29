# Security write-ups

One file per published finding, named `YYYY-MM-DD-slug.md`.

## Why these exist at all

A finding cannot reach `disclosure_stage = 'public'` without one.
`services/security_findings.rs` refuses the transition when `writeup_url` is
absent, so *every published finding has a public write-up* is an invariant of
the code rather than a convention somebody remembers. The link then comes out
on the public card, the hall of fame and the reporter's security profile.

## How one gets published

By hand, through a pull request. There is deliberately no endpoint that commits
a file here.

An earlier version of SKI-132 asked for `POST /api/admin/security/writeups`
committing through `SKILLUV_BOT_GITHUB_TOKEN`. That was dropped, for three
reasons worth keeping written down:

- it would give the production backend a write token on its own repository —
  a surface worth more than the convenience it buys, on a platform whose
  disclosure programme invites people to attack it;
- it would couple publishing a finding to GitHub being up: the API returns 500
  and the finding stays `confirmed` while its embargo has already expired;
- a write-up is an editorial document. It wants a reading, so it wants a pull
  request, which is the ordinary gesture — not a commit made by a server.

So: write the file, open the PR, paste the path into `writeup_url`. The column
accepts a relative path for exactly this reason, and says so.

## The shape

Copy `_TEMPLATE.md`. The front matter is what the public page reads; the
sections are what a stranger needs in order to learn the class of defect
rather than the incident.

## What goes in and what does not

**In:** the defect, how it was reached, what it would have allowed, the fix,
and what would have caught it earlier.

**Out:** anything that identifies a person other than the reporter who agreed
to be credited, and anything still exploitable elsewhere. A write-up is
published *after* the fix ships — but the same class of defect may be live in
somebody else's code, so reproduction steps are written to teach the pattern,
not to hand over a working exploit against a third party.

A reporter may be credited as `Anonyme #123`. Ask; do not assume.
