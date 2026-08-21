# Discord — code

The structure the code side of the community runs on, and the reasoning
behind it. Written down because a server that grows one channel at a time
ends up with forty channels and three conversations.

## The rule this follows

**A channel exists when a conversation is already happening and is drowning
somewhere else.** Not before. Creating a channel in advance produces a room
somebody has to keep alive, and a dead channel does more damage than a busy
one: it tells a newcomer the community is empty.

So the list below is the target, not the opening state. Start with the four
general channels and open the rest as they earn it.

## Code General

| Channel | For |
|---|---|
| `#code-general` | Everything with no better home. The default. |
| `#code-help` | Somebody is stuck. Questions with code, not "does anybody know Rust". |
| `#code-showcase` | What you shipped. Links, screenshots, no permission needed. |
| `#code-newsletter` | Read-only. What happened this week, posted by the bot. |

`#code-help` is the one to protect. A help channel where questions go
unanswered for a day teaches everybody that asking is pointless, and it never
recovers. Better to close it than to let it rot.

## Families

One per reviewer group — the same eight the guides, the review grids and the
reviewer capabilities already use. Anything else would mean somebody has to
learn a second map.

`#code-web` · `#code-mobile` · `#code-systems` · `#code-blockchain` ·
`#code-compilers-formal` · `#code-data-distributed` · `#code-scientific` ·
`#code-devtools`

Open these as the members exist. Eight empty rooms is worse than one busy one.

## Languages

`#lang-rust` · `#lang-go` · `#lang-python` · `#lang-typescript` ·
`#lang-swift` · `#lang-kotlin` · `#lang-cpp` · `#lang-zig` · `#lang-elixir` ·
`#lang-julia` · `#lang-other`

These overlap with the families on purpose: people identify with a language
before they identify with a trade, and the language channel is often where
somebody speaks for the first time.

`#lang-other` is not a leftovers bin — it is where the next channel gets
proposed. Watch it.

## Events

| Channel | For |
|---|---|
| `#code-contests` | Hackathons, code golf, TDD contests. |
| `#code-oss-marathon` | The annual contribution marathon. |
| `#code-missions` | Paid missions, posted by the bot when published. |
| `#code-first-issues` | The curated feed, posted automatically. |
| `#code-review-lounge` | Asking for a review, and offering one. |

`#code-first-issues` is a bot channel and should stay one. A feed people talk
in stops being a feed.

## Voice

| Room | For |
|---|---|
| Code Cowork Room | Open, silent by default. Presence, not conversation. |
| Live Coding Streams | Somebody shares a screen and works. |
| Code Review Circles | Scheduled. Reviewing together, out loud. |
| Hackathon Live | Open only during a contest. |

The cowork room is the one that matters for a distributed community across
several timezones. It costs nothing and it is the closest thing to sitting in
the same office.

## Roles

Assigned by the bot from what the platform already knows. Nothing here is
requested by hand: a role somebody asks for is a role somebody argues about.

| Role | Granted from |
|---|---|
| `Coder` | An active orientation whose domain is `code`. |
| `Code Reviewer` | Any `code_reviewer:*` capability. |
| `Code Champion` | Top 20 by `craft_score_code`. Recomputed weekly. |
| `Featured Coder` | A live `featured_coder` attestation. |
| `OSS Contributor` | At least one `code_pr_merged_upstream` attestation. |
| `Language: X` | Self-selected through the bot. |

Two of these deserve a note.

**`Code Champion` is a leaderboard role and rotates.** Anybody who holds it
permanently means the score has stopped moving, which is a problem with the
score rather than a fact about the person. Recompute weekly and let it change.

**`Language: X` is self-selected and unverified**, unlike every other role
here. That is deliberate: it exists to route notifications, not to make a
claim. Nobody should read it as competence, and the bot's message when
granting it should say so.

## Moderation

The community moderation capabilities already exist on the platform
(`community_moderator`, `forum_moderator`, `community_curator`) and the
Discord roles mirror them rather than being granted separately. One person
having to be promoted twice is one person who ends up promoted once.

Three rules, and they are the same as the forum's:

1. **Technical disagreement is not a conflict.** Do not moderate an argument
   about approaches. Moderate the moment it stops being about the approach.
2. **A question is never off-topic in `#code-help`.** Redirect it, do not
   close it. Somebody who is told their question does not belong asks nowhere
   next time.
3. **Recruitment posts belong in the missions channel.** Everywhere else they
   are advertising, and a community that fills with advertising loses the
   people who were there for the work.

## What this deliberately does not have

**No `#introductions`.** People introduce themselves in the channel they came
for, and an introductions channel is a room where the last message is always
three weeks old.

**No per-project channels.** A project's conversation belongs on its own
repository, where its maintainers are. Mirroring it here splits the discussion
and the mirror always loses.

**No announcement-only category.** One read-only channel is enough. Three is a
website.
