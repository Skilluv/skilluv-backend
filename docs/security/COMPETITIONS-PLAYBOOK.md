# Running a security competition

For whoever is organising one. The five formats, four weeks of preparation, and
the mistakes that are cheap to avoid and expensive to make.

Competitions are `tournaments` rows with `skill_domain = 'security'`. Create
one at `POST /api/admin/tournaments`; the format is a `tournament_kinds` slug.

---

## Choosing a format

| Format | Slug | People needed | Infrastructure | Best for |
|---|---|---|---|---|
| **Jeopardy CTF** | `sec_ctf_jeopardy` | 5 and up | A board of challenges, each independent | A first event. Scales down to five people without feeling empty. |
| **Bug bash** | `sec_bug_bash` | 5–30 | One live scope, and somebody triaging live | A community that has already filed reports |
| **Code audit rally** | `sec_code_audit_rally` | 3–20 | A repository at a pinned commit | The format with the least infrastructure and the most learning |
| **Purple exercise** | `sec_purple_exercise` | 4–10, in two sides | A disposable environment with telemetry | A community with both offensive and defensive people |
| **Attack and defence** | `sec_attack_defence` | 12 and up, in teams | The hardest: one service per team, network isolation, a tick system | Not a first event. Not a second one either. |

**If this is your first: jeopardy, or a code audit rally.** The rally in
particular needs a repository, a commit hash and a jury, and produces better
write-ups than a CTF because everybody read the same code.

**Do not start with attack and defence.** It is the format people picture and
the one that fails: the infrastructure is a project of its own, and an outage
forty minutes in ends the event.

## Four weeks

### S-4 — decide, and write the rules

Format, theme, window, prizes. Write the rules *now*, not the week before: the
document is what settles a contested score, and one written after the contest
starts is a document one side will not accept.

The rules must say: the window in a named timezone, what counts as a valid
submission, how ties break, when the scoreboard freezes, and how a dispute is
raised. Five lines each.

### S-3 — build, and have somebody else try it

Challenges written, or the scope defined, or the environment built.

Then the step that matters more than any other: **somebody who is not you
solves every challenge from the instructions alone.** Not you re-reading them —
somebody else, in a room you are not in.

Every event that goes wrong goes wrong here. A challenge whose flag was
regenerated after the hash was recorded is unanswerable, and nobody finds out
until forty people are stuck on it.

### S-2 — open registration, and announce

`status = 'registration_open'`. Announce in `#security-competitions`, and
elsewhere if you want people who are not already here.

Say the prize in the announcement, including when it is nothing. "Fragments, a
badge and a place on the hall of fame" is an honest offer; a vague hint at
prizes is not.

### S-1 — a dry run, and the moderators

Run the platform side end to end with two accounts: register, submit, see the
scoreboard move, see the badge land.

Brief whoever is moderating: where the rules are, what a hint may say, who
decides a dispute, and the phone number of whoever can restart the
infrastructure.

### The day

- Open on time. Late is worse than short.
- One channel for questions, one for announcements, and nothing important only
  in voice.
- Answer the first three questions in public even if they are answered in the
  rules. Everybody has the same three.
- **Freeze the scoreboard fifteen minutes before the end.** Contests are
  disputed in the last ten minutes and a frozen board is what settles them.

### J+7 — finish it

Finalising awards fragments, badges and attestations from the recorded
placings. Then:

- publish the winners on the competition page;
- publish the write-ups, or ask the top few for theirs;
- send the prizes. **Within seventy-two hours.** A late prize is the single
  thing that stops people entering the next one;
- write a short post-mortem, including what broke.

---

## The mistakes

**Under-estimating the infrastructure.** Ten simultaneous participants is not
ten times one person browsing: it is ten proxies, ten scanners and somebody who
misread "no denial of service". Size for three times the registration count, and
watch the load in the first twenty minutes.

**Badly calibrated challenges.** Everything solved in the first hour, or nothing
solved at all. The dry run is the only fix. A useful shape for a board of ten is:
three anybody can do, five that take an hour, two that maybe nobody gets.

**No written rules.** Every contested score becomes a negotiation, and you lose
either the score or the person.

**Prizes that arrive late.** See above. If the prize needs a bank transfer,
start the paperwork in S-1.

**A purple exercise with no cleanup.** Simulation tooling that leaves
persistence behind has created a real incident. Verify the cleanup before
anybody goes home, and say in the rules who verifies it.

---

## Sponsors

A competition can carry `sponsor_enterprise_id`, which shows as "powered by"
and can fund a cash prize held in escrow until the event concludes.

What a sponsor may be offered: their name on the event, the write-ups if the
authors agree, and a look at the profiles of the people who did well — the
same talent search any recruiting enterprise pays for.

What a sponsor may not be offered: influence over the scoring, the participants'
contact details, or anything a participant did not agree to. Say that to the
sponsor in the first conversation; it is easier than saying it in the third.

---

## What the platform does for you

- Registration, with `min_rank` if you want one.
- Scoring from what participants actually did: flags captured and findings
  confirmed inside the window, read from `security_flag_attempts` and
  `security_findings` rather than from a separate event log that could disagree
  with them.
- A live scoreboard, and `side` on a participant for the two formats that have
  sides.
- Finalising: placings, fragments, badges, attestations.
- A jury, for the three juried formats, with each juror's scores recorded.

## What it does not

Host your infrastructure. A jeopardy board is challenges in the catalogue and
needs nothing; a bug bash needs a scope you control; an attack-and-defence
event needs an environment that is somebody's job for a fortnight. See
`RANGES.md` for what this platform runs and what it does not.
