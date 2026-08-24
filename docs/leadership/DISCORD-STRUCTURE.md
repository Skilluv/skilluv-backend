# Leadership — Discord structure

Backlog: leadership/O-03. The routing rows live in `discord_channels`, which
has a foreign key onto `skill_domains` since migration 0440.

---

## Channels

### General

| Channel | For |
|---|---|
| `#leadership-general` | The domain. Anything not one trade in particular |
| `#leadership-help` | Stuck on a specific document. Bring the draft |
| `#leadership-reading` | Decisions made in public elsewhere: RFCs, proposals, post-mortems worth reading |

`#leadership-reading` is the room that does the most for beginners. Almost
nobody entering this trade has read twenty real decision records, and almost
everything they need is public.

### Per trade

| Channel | Trade | Reviewer family |
|---|---|---|
| `#lead-product` | `lead-product` | `delivery` |
| `#lead-project` | `lead-project` | `delivery` |
| `#lead-tech` | `lead-tech` | `technical` |
| `#lead-people` | `lead-people` | `people` |
| `#lead-community` | `lead-community` | `community` |
| `#lead-mentor` | `lead-mentor` | `teaching` |

Six channels, five review families. `#lead-product` and `#lead-project` share a
reviewer and not a room — the conversations are different even where the
competence is one.

### Practice

| Channel | For |
|---|---|
| `#leadership-cohorts` | Running cohorts on this platform: recruiting, curricula, and what went wrong |
| `#leadership-redaction` | Getting a document read before it is published |
| `#leadership-missions` | Paid work |

**`#leadership-redaction` is the important one and it has a hard rule.**

You ask there for somebody to read a document. You do **not** post the document
in the channel. The request names what kind of artefact it is and how long it
is; the document goes to the person who volunteers, through the platform.

The reason is the whole point of the state: a document posted in a channel of
several hundred people has been published, and the whole review exists to stop
that happening before somebody has checked it.

### Voice

| Room | For |
|---|---|
| `Decision Review` | Reading a decision record out loud, which is the fastest way to find the missing alternative |
| `Cohort Sessions` | Running a cohort session |
| `Retro Practice` | Facilitating a retrospective with people who will tell you what you did wrong |

---

## Roles, assigned from the platform

| Role | Condition |
|---|---|
| `Leader` | Holds any `lead-*` orientation |
| `Leadership Reviewer` | Holds any `leadership_reviewer:*` capability |
| `Cohort Lead` | Leads a cohort that has not concluded |
| `Leadership Champion` | Top 20 by `craft_scores` in the leadership domain |

Assigned by the bot from the database. `Cohort Lead` is the one that expires
on its own: it is derived from `cohorts.concluded_at IS NULL`, so somebody who
finishes a run loses the role rather than keeping a title that no longer
describes anything — which is the same argument the whole domain makes about
titles.

---

## Bot commands

| Command | Answers |
|---|---|
| `/skilluv leadership cohorts` | Cohorts recruiting now, with their entry conditions |
| `/skilluv leadership redaction-queue` | Documents waiting for somebody to confirm a redaction |
| `/skilluv leadership commitments` | Commitments made on your projects that you have not acknowledged |
| `/skilluv leadership my-stats` | Your leadership craft score and its breakdown |

`/skilluv leadership commitments` is the one that makes the coordination
mechanism work. A commitment nobody has seen is a commitment nobody
acknowledges, and the acknowledgement is the score term nobody can produce
alone.

---

## Moderation

Three rules on top of the server's own.

**Nothing unpublished, in any channel.** Not a screenshot of a roadmap, not a
paraphrase of an unreleased strategy, not "my company is about to". If it has
not been reviewed and published through the platform, it is not said here.

**No naming a current or former employer in a complaint.** People come to this
domain during hard periods at work and the temptation is real. The channel is
for the craft; a named grievance is a legal problem for the person posting it
and for us.

**No individual performance discussion.** "How do I have a hard conversation
with somebody who is not meeting expectations" is a craft question and welcome.
Details about the person are not, in any amount of anonymisation.

Escalation goes to `community_moderator`, not to the domain's reviewers: a
reviewer judging somebody's document is not the person who should be judging
whether they breached an NDA in a chat room.
