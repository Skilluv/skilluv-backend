# Leadership — brief templates

Backlog: leadership/F-05. What a client fills in when they want leadership work
done. The **write-up** templates — what the contributor hands back — are rows
in `content_guides` (`kind = 'writeup_template'`, migration 0470).

Six briefs, one per trade. The shared skeleton first.

---

## The shared skeleton

Every leadership brief answers these:

1. **What decision is waiting on this.** The single most useful line, and the
   one most briefs omit. A leadership engagement with no pending decision
   behind it produces a document that is read once and filed.
2. **Who can accept the answer.** If nobody in the engagement has the authority
   to accept the omissions the work will propose, the deliverable is a
   proposal rather than a strategy — and the brief should say so, because the
   two are priced differently.
3. **What has already been tried**, and why it stopped. Most leadership
   engagements are the third attempt. A brief that hides that buys the second
   attempt again.
4. **Confidentiality.** NDA yes or no; whether the contributor may say publicly
   that the engagement happened; and — the one that gets forgotten — whether
   they may submit an **anonymised** version for review. If not, they can still
   build a record: the confidential state exists for exactly this, and the
   brief should say which applies.
5. **Who they can talk to**, and whether those people know the engagement is
   happening. Arriving to interview a team that has not been told is the
   fastest way to a useless engagement.
6. **What is handed back**, from `mission_deliverable_formats`.
7. **When**, and what happens if the client's own inputs are late — which in
   this domain is the normal failure and should be priced.

A brief that cannot answer 1 and 2 is not ready, and the honest response is to
say so rather than start.

---

## `brief-roadmap-quarterly` — product or delivery direction

Beyond the skeleton:

- **The horizon, and why that one.** A quarter and a year are different
  documents.
- **What is already committed** to customers, to a board, or to another team,
  and cannot move.
- **Who else's time this plan will spend**, and whether they know. A roadmap
  that commits four teams and has been shown to one is a wish.
- **The measure the client already watches.** Introducing a new one is
  sometimes right and always slower.

**Not in this brief:** the solution. A client who has written the roadmap and
wants it formatted should say so — it is a real and much cheaper engagement.

## `brief-tech-rfc` — a technical decision

Beyond the skeleton:

- **The constraint that makes this hard.** Every architecture question is easy
  without one; naming it is most of the brief.
- **The scale.** Load, data volume, team size, growth expected. A decision with
  no numbers fits every situation and suits none.
- **What is already decided** and not up for revisiting, however tempting.
- **Access.** Can the contributor read the code, see the metrics, talk to the
  people who operate it? A decision record written from a description is worth
  what the description was worth.
- **Who decides**, and by when. An RFC with no decider is a document.

## `brief-project-delivery-plan` — delivery, usually recovery

Beyond the skeleton, and this one is almost always a rescue:

- **How many times it has slipped**, and what was said each time.
- **What is fixed**: the date, the scope, or the people. At most two. A brief
  that fixes all three is asking for a plan that does not exist, and the
  contributor should say so on day one.
- **Who is allowed to cut scope**, and whether they have agreed to be asked.
- **What the client has told people outside the team**, and how recently.

**The clause worth putting in:** the first deliverable is usually an honest
date, and it will be later than the current one. A client who cannot accept
that is not buying delivery leadership.

## `brief-team-health-audit` — people

Beyond the skeleton:

- **Why now.** Something happened. A brief that does not say what produces an
  audit that finds it in week three.
- **What the team has been told**, and by whom. An audit sprung on people
  produces the answers they think are safe.
- **What the client will do with it.** If the answer is "decide whether to
  restructure", the people answering have a right to know that, and the
  contributor should insist.
- **Anonymity guarantee, and its limit.** In a team of six, some answers
  identify their author. Agree in advance what is aggregated and what is not
  asked.
- **Individual data.** None leaves the client. This is not negotiable and the
  brief states it.

## `brief-community-strategy` — community

Beyond the skeleton:

- **The current numbers**: how many arrived last quarter, how many returned.
  If the client only has the first, measuring the second is phase one.
- **Who runs it today**, how many hours, and whether they are paid. A strategy
  handed to exhausted volunteers is a strategy that does not happen.
- **What the community is for the client.** Support deflection, recruitment,
  product feedback, goodwill — the honest answer changes the whole design, and
  a client who says "all of them" has not decided.
- **What is out of bounds**: topics, tone, competitors, moderation the client
  will not allow.

## `brief-mentoring-cohort` — mentoring and curriculum

Beyond the skeleton:

- **Who the cohort is**, and how they were selected. A cohort assembled by
  self-selection and one assembled by a manager behave differently.
- **The entry condition**, or the statement that the contributor is to
  establish it. Half a cohort is lost by week two when this is wrong.
- **The outcome**, stated as something people will be able to do — and how it
  will be checked.
- **Who owns the curriculum afterwards.** If the client keeps it, say so; if
  the contributor keeps it and licenses it, say that.
- **Participant time**, in hours per week, and whether their managers have
  agreed to it. This is the assumption that most often turns out false.
- **What happens to somebody who falls behind**, and who decides.

---

## The clause every leadership brief should carry

> The deliverable will state what is **not** being pursued and what that costs.
> A document that pursues everything is refused in review, so a brief that
> pushes for one is a brief that will not be delivered against.

And the second, specific to this domain:

> Any claim about people in the deliverable will come with what was measured,
> of how many, and when. If that measurement is not possible within the
> engagement, the claim will not be made.
