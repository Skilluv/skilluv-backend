# Quality — Discord structure

Backlog: quality/O-03. The rows that make the routing work live in
`discord_channels`, which has held a foreign key onto `skill_domains` since
migration 0440 — so a typo'd domain no longer routes an announcement nowhere.

---

## Channels

### General

| Channel | For |
|---|---|
| `#quality-general` | The domain. Anything that is not one trade in particular. |
| `#quality-help` | Stuck on something specific. Bring the link and the command. |
| `#quality-showcase` | Reports, suites and studies people are proud of. |

### Per trade

| Channel | Trade | Reviewer family |
|---|---|---|
| `#quality-code` | `qa-code` | `automation` |
| `#quality-cyber` | `qa-cyber` | `intrusion` |
| `#quality-design` | `qa-design` | `usability` |
| `#quality-game` | `qa-game` | `playtest` |
| `#quality-lead` | `qa-lead` | `strategy` |

Five channels for five trades, which is one per trade and unusual — the other
domains group by reviewer family and end up with fewer rooms than trades. Here
the two coincide, for the reason the review grids give: no two of these are
read by the same person.

### Practice

| Channel | For |
|---|---|
| `#quality-participants` | Recruiting for studies and playtests. The hardest part of two trades, and the one nobody can solve alone. |
| `#quality-tools` | Tooling, and what the free tiers actually cover. |
| `#quality-bug-bashes` | Running and joining defect hunts. |
| `#quality-missions` | Paid work. |

`#quality-participants` is the room that would not exist in any other domain's
structure and is the most useful one here. A usability researcher with no
participants has a protocol and nothing else, and the platform's own community
is the cheapest place to find five people who have never seen a given product.

**One rule in that channel:** you may ask for participants, and you may not
run the session in it. Consent, recording and anonymisation happen in the
study, not in a public chat.

### Voice

| Room | For |
|---|---|
| `Defect Hunt Live` | Running a bug bash together. |
| `Playtest Sessions` | Facilitating, with the author present if they want. |
| `Report Review` | Reading somebody's report out loud, which is the fastest way to find the step a stranger cannot follow. |

---

## Roles, assigned from the platform

| Role | Condition |
|---|---|
| `Quality` | Holds any `qa-*` orientation |
| `Quality Reviewer` | Holds any `quality_reviewer:*` capability |
| `Quality Champion` | Top 20 by `craft_scores` in the quality domain |

Assigned by the bot from the database, never by hand. A role granted manually
is one that survives the capability being revoked, which is how somebody keeps
review-coloured standing after losing review rights.

---

## Bot commands

The bot has one command with subcommands, and the domain is an argument
rather than part of the name. There is no `/skilluv quality my-stats`: there
were four of them written down across two of these documents, none of them
existed, and writing them as four would have made the next domain a fifth.

| Command | Answers |
|---|---|
| `/skilluv craft quality` | Your craft score in this domain, its tier, and how many attestations it rests on |
| `/skilluv queue quality` | How much work is waiting on a reviewer here, split into picked up and not, with the age of the oldest unclaimed one |
| `/skilluv cohorts quality` | Cohorts recruiting now, with places left |
| `/skilluv contests quality` | Open contests in this domain — a bug bash is a contest, so this is where they appear |
| `/skilluv featured quality` | Who is featured this week |
| `/skilluv me` | Your linked profile, trades and craft score |
| `/skilluv portfolio <username>` | Somebody's public profile |
| `/skilluv verify <hash>` | Check an attestation from its hash |

`/skilluv queue` is public on purpose, and it reports the unclaimed count
first. A review queue nobody can see is a queue that grows quietly, and the
number being visible is what makes somebody volunteer. The age of the oldest
unclaimed item is there because a queue of three that turns over in a day is
healthy and a queue of three where one has sat a fortnight is not, and the
count alone hides the difference.

The bot does not post participant recruitment calls. A study recruitment
carries a consent protocol, a compensation statement and a description of what
is recorded, and a slash command is the wrong door for all three. Recruitment
goes through `#quality-participants` as a written post, which is also the
record of what participants were told.

---

## Moderation

Two rules specific to this domain, on top of the server's own.

**No unpublished defect details in public channels.** Not a screenshot, not a
"has anyone else seen X on Y". A defect found under an engagement stays in the
engagement until the fix ships. Somebody who posts one is asked to delete it,
and it is treated as a mistake the first time and as a disclosure breach the
second.

**No participant footage.** Ever, in any channel, including a clip with the
face blurred. The consent covered the study.

Escalation for both goes to `community_moderator`, not to the domain's own
reviewers: a reviewer judging a report is not the person who should be judging
whether its author breached an NDA.
