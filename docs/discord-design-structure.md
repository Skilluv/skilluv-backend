# The design community on Discord

What SKI-268 asked for, and what was built — with the reasons for the
differences.

## The short version

Design is one of eleven domains that share one shape, declared in
[`ops/discord/server.toml`](../ops/discord/server.toml) and applied by
[`scripts/discord-setup.py`](../scripts/discord-setup.py). Nothing is created by
hand: if a channel is not in that file it does not exist.

## Channels

**The common shape**, the same four every domain has:

| Channel | For |
| --- | --- |
| `design-general` | Everything, and where a newcomer lands |
| `design-help` | Stuck, ask here |
| `design-showcase` | Show finished work |
| `design-newsletter` | Announcement. What happened this month |

**Specific to the craft:**

| Channel | For |
| --- | --- |
| `design-critique` | Work in progress, brought for feedback |
| `design-veille` | References, articles, what is worth looking at |
| `design-relecteurs` | Reviewers coordinating on the queue |

`showcase` and `critique` are not the same room on purpose. One is "this is
done", the other is "tell me what is wrong with this" — and a critique posted
under a showcase gets applause instead of an answer.

**One per trade**, the thirteen families:

`design-produit` · `design-system` · `design-web` · `design-mobile` ·
`design-motion` · `design-brand` · `design-illustration` · `design-dataviz` ·
`design-ux-writing` · `design-marketing` · `design-game` · `design-3d-viz` ·
`design-immersif` · `design-service`

**Events:**

| Channel | For |
| --- | --- |
| `design-concours` | Announcement. Contests, and their winners |
| `design-missions` | Paid work |

Voice: **Design Cowork Room**, **Critique Live**.

### `design-system` was missing

It is one of the 26 trades and one of the four in the first batch to open
(SKI-349) — the four a developer can review without recruiting anybody. It was
the only family with no room to be discussed in, which would have been noticed
by the first person to arrive for it.

### The channels the ticket asked for and did not get

`design-battles`, `design-sprints` and `design-awards` are not declared,
because battles, sprints and awards are not things this platform does. There is
no battle, no sprint and no award in the schema, in the services or in the API.

A channel for a feature that does not exist is worse than no channel: on a
server with nothing in it yet, three empty rooms read as an abandoned project
rather than a young one. `design-concours` already carries contests and their
winners, which is the part that is real.

They are one line each in `server.toml` the day the feature ships.

## Roles

Assigned by the bot from the profile, never by hand, and reconciled as a diff —
a role the profile no longer justifies is removed.

| Role | Granted on |
| --- | --- |
| `Designer` | Having declared a trade in this domain |
| `Relecteur design` | Capability `design_reviewer:*` |
| `Jury` | Capability, for contest juries |
| `Mentor` | Capability `mentor` — one role across all domains |

### The roles the ticket asked for and did not get

`Design Champion` (top 20 craft score), `Contest Winner` and
`Featured Designer` (rotating) are not declared. All three need a periodic
recomputation over a population that does not exist yet, and a role that never
gets granted reads as a broken bot rather than an empty leaderboard.

The data behind them is there when it is worth doing: `craft_scores` per
domain, the contest-winner badge, and the featured rotation. The missing part
is people, not code.

`Design Mentor` is served by the domain-neutral `Mentor` role. A mentor's
domain is on their profile; splitting the role eleven ways would give the
server eleven roles saying what one already says.

## Moderation

The general rules apply. Three are specific to a design community, and they
exist because this craft has failure modes the others do not:

- **Critique addresses the work, never the person.** "This hierarchy does not
  read at a glance" is useful. "This is amateur" is not, and it is what makes
  people stop posting.
- **Credit what you did not make.** Fonts, photography, icons, references — say
  where they came from. This is a platform whose whole promise is that work is
  attributable; a design channel that shrugs at attribution contradicts it.
- **Self-promotion belongs in `design-showcase` and `design-missions`.** Not in
  the family channels, which are for the craft.
