# Discord — the education corner

What to create, what to call it, and which channels have a rule of their own.
The server-wide setup — webhooks, the notifier binary, env vars — is in
[../DISCORD_SETUP.md](../DISCORD_SETUP.md); this is the education structure
that sits on top of it.

---

## The shape

Ticket O-03 asked for seven channels. Five, because two of the seven would
have been the same room: `#edu-trainer` and `#edu-teacher` split people who
answer each other's questions constantly, and both would have been quiet.

The split that survives is the one used everywhere else — the **two review
families** — plus the three channels that are about doing rather than about a
trade.

### Text

| Channel | For |
|---|---|
| `#edu-general` | everything, and the default landing place |
| `#edu-help` | one question, one answer. Not for showing work |
| `#edu-teaching` | delivery: pacing, exercises, a room that has gone quiet |
| `#edu-curriculum` | design: objectives, sequencing, rubrics |
| `#edu-cohorts` | the cohorts currently running here, one thread each |
| `#edu-missions` | paid work, posted by the platform |

### Voice

| Channel | For |
|---|---|
| `Training feedback` | scheduled. Watching somebody run a session and telling them |
| `Curriculum review circle` | reading a programme together, out loud |
| `Office hours` | open, for whoever is teaching this week and stuck |

## The rule that is specific to this domain

**No learner is named anywhere on this server.**

Not in `#edu-cohorts`, not in a thread, not in a voice channel, not "the one
who keeps missing Thursdays". Discord is not covered by any of the platform's
learner-data controls: it is a third-party service, its logs are searchable by
everybody in the room, and nothing said there can be erased on request.

Ask about the situation, not about the person: "somebody in week three has
stopped submitting and does not answer, what do I try" is the question, and it
is a better question anyway.

A message naming a learner is deleted by a moderator without discussion, and
the person who posted it is told why. This is the only rule on the server with
that treatment, and it is because the person harmed by it is not there to
object.

## `#edu-cohorts`, one thread per cohort

Each cohort running on the platform gets a thread, opened when it starts and
archived a month after it concludes. The thread is for the **educator's**
questions about running it — pacing, an exercise that did not land, what to do
about week three.

It is not the cohort's own space. Learners talk in the cohort's own messaging
on the platform, where the rules about their data apply.

## Roles

| Role | Granted | What it does on Discord |
|---|---|---|
| `Educator` | a first verified education deliverable | access to the family channels |
| `Education Reviewer` | holds `education_reviewer:*` | reviewer tag; visible in the member list |
| `Cohort Lead` | leads a cohort that has not concluded | can open a thread in `#edu-cohorts` |
| `Education Champion` | editorial, by a curator | can pin, can schedule the voice sessions |

`Cohort Lead` is the one role here that is temporary by design: it follows
`cohorts.led_by_user_id` and lapses when the cohort concludes. Somebody who
led three cohorts two years ago is an `Educator`, which is the accurate thing
to say about them.

## What is posted automatically

From `discord_notifications_queue`, filtered on `skill_domain = 'education'`:

- an attestation issued, to `#edu-general`, with the artefact it rests on;
- a mission published, to `#edu-missions`;
- a cohort opening for applications, to `#edu-cohorts`;
- a teaching position curated on the opportunities board, to `#edu-missions`.

Never posted: a completion rate, a cohort's outcomes, a learner count under
five, or anything at all about a named person. A cohort that concluded is
announced as having concluded, and the figures live where the rules cover
them.
