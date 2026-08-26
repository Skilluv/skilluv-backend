# Security — Discord structure

Backlog: security/O-04. The rows that make the routing work live in
`discord_channels`, keyed on `skill_domains` since migration 0440, so a
mistyped domain no longer routes an announcement nowhere.

The channel ids are filled in by whoever creates the channels — this document is
the plan and the reasoning, not the configuration.

---

## Channels

### General

| Channel | For |
|---|---|
| `#security-general` | The domain. Introductions, anything not one trade in particular. |
| `#security-help` | Stuck on something specific. Bring the request, the response and what you expected. |
| `#security-writeups` | Published write-ups, yours and other people's. |

### Per trade

| Channel | Trade | Reviewer family |
|---|---|---|
| `#security-red` | `security-red-team` | `red-team` |
| `#security-blue` | `security-blue-team` | `blue-team` |
| `#security-code-audit` | `security-code-audit` | `code-audit` |
| `#security-governance` | `security-governance` | `governance` |
| `#security-purple` | `security-purple-team` | `purple-team` |

Five channels for five trades. Unusual — most domains group by reviewer family
and end up with fewer rooms than trades — and here the two coincide, for the
reason the grids give: no two of these are read by the same person.

### Practice

| Channel | For |
|---|---|
| `#security-ranges` | Juice Shop, WebGoat, retired machines. Hints with spoiler tags, always. |
| `#security-tooling` | Tools, and what the free editions actually do. |
| `#security-competitions` | Announcements, teams, and the live channel during an event. |

### The one that is private

| Channel | Who is in it | Why it is closed |
|---|---|---|
| `#security-triage` | `security_triager`, `security_reviewer:*`, admins | It carries details of unfixed vulnerabilities in live systems, including this one. An embargo that leaks in a chat room is an embargo that did not exist. |

Nothing about a specific finding goes anywhere else until it is published. That
is not a moderation preference, it is the promise made in `SCOPE.md`.

### Voice

| Channel | For |
|---|---|
| `Security study` | Working alongside somebody. Screen sharing a proxy is how most people learn to drive one. |
| `Purple session` | Scheduled exercises. Two sides, one room, one timeline. |

---

## Roles

Assigned by the bot from what the platform already knows, so a role is never a
second record that drifts.

| Role | Granted from |
|---|---|
| `Security` | A `security` orientation chosen |
| `Researcher` | One confirmed finding |
| `Triager` | `security_triager` |
| `Reviewer` | Any `security_reviewer:{family}` |
| `Mentor` | An active mentorship in this domain |

`Researcher` deliberately needs a *confirmed* finding rather than a submitted
one. The role means somebody else reproduced your work, which is what makes it
worth having.

---

## The rules that are specific to this domain

Three, and they are stricter than the server's general rules for reasons that
are not about tone.

**1. No unpublished finding, anywhere but `#security-triage`.** Not a hint, not
a screenshot with the host cropped, not "has anyone else noticed the export
endpoint is weird". If it is not published, it is under embargo.

**2. No target outside a published scope, ever, in any channel.** Somebody
asking for help testing their employer's site is asking the room to help them
commit an offence. The answer is `SCOPE.md`, and it is not a judgement about
them.

**3. Hints on ranges go behind spoiler tags.** Somebody in `#security-ranges`
is mid-way through a machine, and an unmarked answer takes the afternoon away
from them.

---

## What the bot posts, and where

| Event | Channel |
|---|---|
| A finding published | `#security-writeups` |
| A first solve on a challenge | `#security-ranges` |
| A competition opening, starting, ending | `#security-competitions` |
| A report arriving with no triage | `#security-triage` |
| A featured researcher, weekly | `#security-general` |

Notably absent: anything about a finding before it is published, and anything
about a paid mission. The first is the embargo; the second is somebody's
commercial business.
