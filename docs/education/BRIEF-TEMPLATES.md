# Education — brief templates

Backlog: education/F-05. What a client or a challenge author fills in when they
want teaching or curriculum work done. The **write-up** templates — what the
contributor hands back — are the other end of the same conversation.

Both halves are rows in `content_guides`, seeded by migration
`0530_education_content_guides.sql` and translated in
`0536_education_guides_french.sql`: `kind = 'brief_template'` for these,
`kind = 'writeup_template'` for the others. They are rows rather than files
because they are edited by people who do not deploy, and this document says
what they are for.

Three briefs, one per trade — and here the trade split is right even though
there are only two review families, for the same reason it is right for the
onboarding guides. Commissioning a curriculum and commissioning a term of
teaching have nothing in common to say.

---

## The shared skeleton

Every education brief answers these:

1. **Who the learners are**, how many, and what they can already do —
   honestly. The optimistic answer here is the single most expensive error in
   this domain.
2. **Whether they are attending voluntarily.** If they are not, the first
   session is about that whether or not anybody planned it.
3. **What they must be able to do afterwards**, stated as something
   observable.
4. **How you will know it worked.**
5. **The shape**: hours, period, live or remote, group sizes.
6. **The environment**: whose machines, what is installed, who can install,
   and the fallback for the laptop that will not cooperate.
7. **What is handed over**: slides, exercises, solutions, facilitator notes,
   recording — and who owns them afterwards.

A brief that cannot answer 1 and 3 is not ready.

---

## Learner data is in every one of them

The three briefs each carry a data section, and it is not a formality. This is
the domain whose artefacts contain facts about people who are not members of
this platform, are sometimes minors, and never asked to be evidence.

Each brief asks:

- what learner data the contributor will see, and where it lives;
- what may leave the room, and with whose consent;
- **whether any participants are under 18**;
- what must be deleted at the end, and when.

The minors question is asked in the brief rather than discovered later because
the answer changes what may be collected at all, not what may be published. A
programme for minors that starts before anybody asked has already collected
things it should not have.

---

## `brief-education-training` — delivering a workshop, course or cohort

The environment section is the longest, because more sessions fail on setup
than on content. It asks who can install software and how long approval takes,
which is the question that turns a three-hour workshop into a two-hour one.

Also asks how many runs the fee includes. A course prepared once and delivered
four times is a different commission from one delivered once, and the
difference is usually discovered on the second delivery.

## `brief-education-curriculum` — designing a programme somebody else runs

Opens with the problem rather than the topic: what can people not do today,
what changes in the organisation when they can, and why a programme rather
than documentation or better tooling.

That last question is asked in good faith. A fair number of training requests
are a tooling problem wearing a training costume, and a contributor who says
so early is worth more than one who delivers the programme.

The rights section asks who updates the curriculum when a tool version moves,
and whether maintenance is part of this commission. A curriculum with no named
maintainer breaks silently within a year.

## `brief-education-teaching-engagement` — a term, a module, a series

The one for ongoing teaching rather than a single delivery. It asks what
proportion of learners are expected to complete, **based on your history** —
not a target. A client who says ninety per cent and has never measured is
telling the contributor something useful either way.

It also asks who handles a learner who falls behind, and whether office hours
and correction time are paid or assumed. They are usually assumed, and that is
where these engagements quietly become underpaid.

---

## The clause every education brief should carry

**What happens if the group does not fill.** A cohort designed for twenty and
attended by four is a different job — better for the learners, worse for the
economics — and who carries that is a question with a clean answer before
enrolment opens and no clean answer after.
