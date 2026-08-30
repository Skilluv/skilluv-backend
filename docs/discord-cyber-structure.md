# The cyber community on Discord

What SKI-180 asked for, and what was built instead — with the reasons, because
the differences are deliberate and someone will otherwise "fix" them back.

## The short version

There is no cyber tree on this server. There is a **security** tree, and it is
one of eleven domains that all have the same shape. The whole server is
declared in [`ops/discord/server.toml`](../ops/discord/server.toml) and applied
by [`scripts/discord-setup.py`](../scripts/discord-setup.py).

Nothing here is created by hand. If a channel is not in that file, it does not
exist; if it is, the script creates it and records the id.

## Channels

| Channel | For |
| --- | --- |
| `security-general` | Everything, and the room a newcomer lands in |
| `security-help` | Stuck on something, ask here |
| `security-red` | Offensive work — pentest, exploitation, CTF |
| `security-blue` | Defensive work — detection, forensics, incident response |
| `security-purple` | The two together, which is where most real work sits |
| `security-code-audit` | Reading code for vulnerabilities |
| `security-governance` | Policy, compliance, risk — the unglamorous half |
| `security-ranges` | The practice targets, and how to reach them |
| `security-tooling` | What people run, and what it is worth |
| `security-triage` | Reviewers coordinating on the disclosure queue |
| `security-writeups` | Announcement. Published disclosures |
| `security-hall-of-fame` | Announcement. Who found what |
| `security-competitions` | Announcement. Contests, and their winners |
| `security-missions` | Paid work |

Voice: **Purple Session**, **Range Cowork Live**.

### Why `security-` and not `cyber-`

The ticket wrote `#cyber-*`. The domain is called `security` in the database,
in `SKILL_DOMAINS`, in every capability (`security_reviewer:*`), in every route
and in the other ten domains' channel names. Two words for one thing is a debt
that gets paid every time somebody has to remember which context they are in —
and the bot would have to translate between them on every command.

### `security-hall-of-fame`

Backed by `GET /api/security/hall-of-fame`, which is public and already serves
the same rows to the website. A figure posted in this channel and a figure on
the page come from one query, so they cannot drift.

## Roles

Assigned by the bot from what the profile says, never by hand. They follow the
profile: they change when trades, rank or capabilities change.

| Role | Granted on |
| --- | --- |
| `Security Researcher` | Having declared a trade in this domain |
| `Security Reviewer` | Capability `security_reviewer:*` |
| `Security Triager` | Capability `security_triager`, **manually** |
| `Mentor` | Capability `mentor` — one role across all domains |

Reconciliation is a **diff**, not an addition: a role the profile no longer
justifies is removed. `services::discord_roles::diff` only ever touches roles
the declaration names, so a role somebody was given by hand for another reason
survives.

### `Security Triager` is manual, and stays manual

It is on the never-published deny list with `admin`, `kyc_reviewer` and
`plagiarism_reviewer`. Those capabilities say who can see other people's
unpublished vulnerabilities and identity documents. A role that announces that
in a public member list tells an attacker exactly whom to target, and a bot
that grants it automatically turns a capability change into a disclosure.

### The roles the ticket asked for and did not get

`Cyber Champion` (top 10 all-time) is not declared. It needs a periodic
recomputation, and on a platform with no users yet it would rank nobody: a role
that never appears reads as a broken bot rather than an empty leaderboard. It
is a good idea for the day there is something to rank.

## Slash commands

The ticket asked for four. Two were built, and they cover three of the four.

| Asked | Built | Why |
| --- | --- | --- |
| `/skilluv leaderboard cyber` | `/skilluv leaderboard <domain>` | Generic. Covers every domain, including the twelfth |
| `/skilluv cyber-me` | the same command | It ends with where *you* stand in that domain |
| `/skilluv cyber` | `/skilluv findings` | The hall of fame — stats, top reporters, latest published |
| `/skilluv finding <hash>` | **not built** | See below |

### Why not a `cyber` subcommand

This bot has already served a stale command tree once, because it kept its own
copy of the domain list and that copy went out of date: four domains had opened
and the bot was quietly telling people they did not exist. A `cyber` subcommand
is the same mistake with a different shape. `leaderboard <domain>` reads the
list every validator reads.

### `/skilluv finding <hash>` was not built

Findings have no hash. They are identified by a UUID, and what is public about
a published one is its `writeup_url`. The `<hash>` in the ticket is the shape of
`/skilluv verify <hash>`, which looks up an **attestation** — a different object
with a real content hash.

A per-finding lookup is reasonable and would need a public read route that does
not exist yet. It belongs with SKI-132's public writeup pages, where the
question "what is the public identity of a finding" has to be answered once.

Anonymous reporters stay anonymous in every one of these commands, exactly as
they do on the website. Somebody who reported under an alias chose that, and a
bot that expands it in a public channel has broken the promise the disclosure
policy makes.

## Moderation

The general rules apply. Two are specific here:

- **No live target that is not ours.** Discussing a technique is fine.
  Coordinating against a third party from this server is not, whatever the
  justification.
- **An unpublished finding stays in `security-triage`.** Not in DMs, not in
  `security-red`. The disclosure policy sets when a finding becomes public, and
  a screenshot in a general channel decides it for everybody.
