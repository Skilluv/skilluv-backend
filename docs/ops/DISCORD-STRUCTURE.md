# Discord — ops

The structure the ops side of the community runs on, and the reasoning behind
it. Same rule as the code side: **a channel exists when a conversation is
already happening and is drowning somewhere else.** The list below is the
target, not the opening state.

## Ops General

| Channel | For |
|---|---|
| `#ops-general` | Everything with no better home. The default. |
| `#ops-help` | Somebody is stuck, with a plan output or a log to show. |
| `#ops-tools` | What we are running and what it costs. Tool arguments live here rather than leaking into every other channel. |
| `#ops-missions` | Paid work: briefs posted, questions asked before applying. |

`#ops-help` is the one to protect, and it is harder to protect here than in
code. An ops question often arrives with a production incident behind it, so
the answer time matters more and the person asking is usually not free to
wait a day. If it cannot be answered fast, say so fast.

## Trades

One per trade rather than one per review family, unlike the code side. Eight
channels for eight trades: a database administrator and a platform engineer
share a reviewer and share almost nothing else day to day.

`#ops-devops` · `#ops-sre` · `#ops-cloud` · `#ops-platform` · `#ops-k8s` ·
`#ops-observability` · `#ops-incident` · `#ops-db`

Open them as the members exist. Eight empty rooms are worse than one busy one.

## The one that is not like the others

`#ops-incidents-lounge` — where people tell what happened to them. Not a
help channel and not a support channel: a place to say "we lost four hours to
a DNS record nobody owned" without it being a request.

This channel is worth having early and worth moderating carefully. Two rules,
stated in the topic:

1. **no employer named, no colleague named.** The same reason post-mortems on
   this platform are blameless: a story that names somebody is a story nobody
   tells honestly the next time, and it can also get the teller fired;
2. **no live incident.** Somebody currently on fire goes to `#ops-help` or to
   their own escalation path. This room is for afterwards.

## Voice

| Room | What happens |
|---|---|
| `Incident Simulation Live` | A drill, run for real, with roles assigned. The scenario is announced beforehand; the failure is not. |
| `Chaos Engineering Sessions` | Somebody breaks their own system while others watch and predict. |
| `Runbook Review Circles` | A runbook is read aloud by somebody who did not write it. Every place they hesitate is a defect. |

The third one is the most useful and the least obvious. A runbook is only
tested when a stranger reads it, and reading it aloud makes every ambiguity
audible.

## Roles

| Role | Given for |
|---|---|
| `Ops Engineer` | Any verified ops artefact. The entry role. |
| `Ops Reviewer` | Holds an `ops_reviewer:*` capability on the platform. Mirrors the family, so the review families are visible in the server too. |
| `Ops Champion` | Community recognition: the person others go to. Given by the community, taken away by nobody. |
| `Incident Commander` | On the Skilluv community rotation — the people who respond when our own platform falls over. |

The last one is a real responsibility and not a decoration. Anybody holding
it is on a rotation with a response time, and the rotation is published. If
Skilluv ever hands this role out as a badge, the role stops meaning anything
and the rotation stops working.

## What the bot does

- posts new ops missions into `#ops-missions`;
- posts a verified artefact into `#ops-general` when its author allows it;
- announces drills, and nothing else on a schedule.

No automated welcome message per channel, no daily digest. A server where the
bot speaks more than the people is a server that reads as empty even when it
is not.
