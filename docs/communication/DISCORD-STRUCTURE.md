# Discord — the communication corner

What to create, what to call it, and which channels have a rule of their own.
The server-wide setup — webhooks, the notifier binary, env vars — is in
[../DISCORD_SETUP.md](../DISCORD_SETUP.md); this is the communication
structure that sits on top of it.

---

## The shape, and why it is not one channel per trade

Ticket O-03 asked for five trade channels plus per-language sub-channels under
translation. That is five rooms and potentially twenty, for a community of
forty people. An empty room reads as an empty platform, and twenty of them
read as an abandoned one.

So the channels follow the **four review families**, which is how the trades
are grouped everywhere else — the guides, the review grids, the capabilities.
Splitting a family when it gets busy is a Discord setting, not a rewrite.

The per-language channels are the exception worth naming: they are not created
in advance. A language gets a channel when **three people have declared they
review in it** — the `user_review_languages` table is the count — and it is
archived when it has been silent for six months. Creating `#comm-wolof` before
anybody speaks Wolof here is not welcoming, it is a room with a sign on it.

### Text

| Channel | For |
|---|---|
| `#comm-general` | everything, and the default landing place |
| `#comm-help` | one question, one answer. Not for showing work |
| `#comm-documentation` | docs, tutorials, references, changelogs |
| `#comm-advocacy` | talks, videos, streams, podcasts, communities |
| `#comm-translation` | terminology, tooling, i18n pipelines |
| `#comm-research` | whitepapers, reports, external specifications |
| `#comm-showcase` | finished work only. Reading, not critique |
| `#comm-review-swap` | "who will read this before I publish it" |
| `#comm-cfp` | open calls for papers, mirrored from `/api/opportunities` |
| `#comm-missions` | paid work, posted by the platform |

### Voice

| Channel | For |
|---|---|
| `Live writing` | working alongside each other in silence. It works |
| `Talk rehearsal` | running a talk past two people before a room sees it |
| `Content feedback` | scheduled, not drop-in. Watching somebody's video with them |

## The rules that are not the server's defaults

**`#comm-help` is one question, one answer.** Showing work goes in
`#comm-showcase`; asking for a review goes in `#comm-review-swap`. A help
channel that fills with finished work stops being usable by somebody stuck.

**`#comm-review-swap` is a swap.** Ask for a read, offer one. A channel where
the same five people read everybody's drafts burns those five people out in
two months.

**`#comm-showcase` is read-only for critique.** Reactions yes, threads no.
Somebody posting finished work is not opening a review, and unrequested
critique on a piece already published helps nobody.

**`#comm-cfp` is mirrored, not posted by hand.** It carries what a curator put
on the opportunities board, so there is one place a deadline can be wrong
rather than two.

## Roles

| Role | Granted | What it does on Discord |
|---|---|---|
| `Communicator` | a first verified communication deliverable | access to the family channels |
| `Communication Reviewer` | holds `communication_reviewer:*` | can post in `#comm-review-swap` with the reviewer tag; visible in the member list |
| `Translation Reviewer` | holds `communication_reviewer:translation` | plus the per-language channels, with the declared languages in the nickname |
| `Communication Champion` | editorial, by a curator | can pin, can open a per-language channel |

Roles follow capabilities rather than the other way round. A role granted on
Discord and not in the platform is a permission nobody can audit.

## Bot commands

The bot has one command with subcommands, and the domain is an argument
rather than part of the name. There is no `/skilluv communication my-stats`: there
were four of them written down across two of these documents, none of them
existed, and writing them as four would have made the next domain a fifth.

| Command | Answers |
|---|---|
| `/skilluv craft communication` | Your craft score in this domain, its tier, and how many attestations it rests on |
| `/skilluv queue communication` | How much work is waiting on a reviewer here, split into picked up and not, with the age of the oldest unclaimed one |
| `/skilluv cohorts communication` | Cohorts recruiting now, with places left |
| `/skilluv contests communication` | Open contests in this domain — docs jams and content sprints appear here |
| `/skilluv featured communication` | Who is featured this week |
| `/skilluv me` | Your linked profile, trades and craft score |
| `/skilluv portfolio <username>` | Somebody's public profile |
| `/skilluv verify <hash>` | Check an attestation from its hash |

`/skilluv queue` is public on purpose, and it reports the unclaimed count
first. A review queue nobody can see is a queue that grows quietly, and the
number being visible is what makes somebody volunteer. The age of the oldest
unclaimed item is there because a queue of three that turns over in a day is
healthy and a queue of three where one has sat a fortnight is not, and the
count alone hides the difference.

`/skilluv queue communication` counts what is waiting across all four review
families rather than per family. Splitting it would tell somebody in the
translation family that documentation has a backlog they cannot help with,
which is a number that produces guilt rather than reviews.

---

## What is posted automatically

From `discord_notifications_queue`, filtered on `skill_domain = 'communication'`:

- an attestation issued, to `#comm-showcase`, with the link it rests on;
- a mission published, to `#comm-missions`;
- an opportunity curated, to `#comm-cfp`;
- a contest opening or closing, to `#comm-general`.

Nothing about a review, a revision round or a rejection is ever posted. Those
are between two people, and a channel that announces them is one nobody submits
to twice.
