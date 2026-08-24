-- The education guides, toolkit, briefs and writeup templates.
--
-- Migration 0199 built `content_guides`, 0419 added the fourth kind, and 0514
-- was the first to seed in English, which this follows.
--
-- ## Three onboarding guides for two review families
--
-- 0199 established one guide per reviewer family and 0419 restated it. This
-- is the first domain to depart from that, and the reason is that the two
-- splits answer different questions.
--
-- A review family is drawn by what a *reviewer* has to be able to do, and the
-- trainer and the coding teacher genuinely share that: both are judged on
-- whether the room moved. An onboarding guide is drawn by what a *newcomer*
-- needs first, and there the two diverge completely — somebody preparing
-- their first three-hour workshop for professionals and somebody about to
-- face twenty beginners every day for a term have almost no overlapping
-- first month.
--
-- Where the two splits coincide, one guide per family is right. Here they do
-- not, and following the rule would have produced a guide that served neither
-- reader. Both are tagged `teaching`, so a reader filtering by family gets
-- both and picks.
--
-- ## Three brief templates
--
-- Ticket F-05's, one per trade, and here the trade split is right for the
-- same reason the onboarding one is: a brief is written by the person
-- commissioning, and commissioning a curriculum and commissioning a term of
-- teaching have nothing in common to say.
--
-- ## Eight writeup templates
--
-- Ticket G-02's list, unchanged. Deliberately short: a template long enough
-- to be impressive is one people replace with a blank page.

-- ═══════════════════════════════════════════════════════════════════
-- Onboarding
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-education-trainer', 'onboarding', 'education', 'teaching', 'en',
 'Starting out as a technical trainer',
 'Delivering to a room that arrived with a stated starting point: where to begin, and what sends work back.',
$md$
# Starting out as a technical trainer

Training is not presenting. A talk succeeds if the room understood; a
training succeeds if the room can now do the thing. Those are different
targets and they need different preparation.

## The rule that decides everything else

Participants should spend more time working than watching. A three-hour
workshop with twenty minutes of hands-on is a talk with exercises attached.
Design the exercises first and fit the explanation around them, not the
other way round — it is the single change that improves a session most.

## The first thirty days

1. **One ninety-minute session, one concept.** Small enough that you can
   prepare it properly and find out what you did not know about your own
   subject.
2. **The exercise that fails instructively.** Write a task where the wrong
   approach visibly does not work. That failure teaches more than your
   explanation of why it would not.
3. **A run with somebody watching.** Ask a colleague to sit in and tell you
   where the room lost you. You will not see it yourself.
4. **A measured outcome.** Ask what people could do before and after, on
   something observable. Do it even for a ninety-minute session.

## Preparing the environment

More sessions fail on setup than on content:

- state the prerequisites, with versions, at least a week ahead;
- provide a fallback that needs no local install — a container, a hosted
  environment, a prepared virtual machine;
- run the setup yourself on a clean machine, not on yours;
- have something for the person whose laptop simply will not cooperate, so
  they can pair rather than sit out.

## What sends work back

- **no evidence anybody learned.** Satisfaction is a real signal about
  whether people return, and it is not evidence. Measure something.
- **materials nobody else could run.** Slides without notes, exercises
  without solutions, an environment only you can set up. That is a
  performance, not an artefact.
- **a learner in the delivery.** Names, faces, marks, messages. Anonymise at
  source. A delivery that exposes a participant is refused however good the
  teaching was.

## Where to go next

- `#edu-trainer` on Discord.
- The review grid for your family is public: read it before you submit.
- Julie Dirksen, *Design for How People Learn* — the one book that changes
  how you prepare.
$md$, 410),

('onboarding-education-coding-teacher', 'onboarding', 'education', 'teaching', 'en',
 'Starting out as a coding teacher',
 'The same people every day, most of them beginners: what the job actually is.',
$md$
# Starting out as a coding teacher

The hard part of this trade is not the subject. You know how a loop works.
The hard part is watching somebody be stuck and knowing which of four
different problems you are looking at.

## The four reasons somebody is stuck

Learn to tell them apart before anything else. They look identical from the
front of the room and they need opposite responses:

1. **A missing prerequisite.** They cannot do this because they cannot do
   something earlier. Going over the current step again will not help.
2. **A misread instruction.** They are solving a different problem correctly.
   Ask them to say back what they think they are doing.
3. **A broken environment.** Nothing is wrong with their thinking. Ten
   minutes of this and they will conclude they are bad at programming.
4. **Fear of asking.** They have been stuck for forty minutes and said
   nothing. This is the most common one and the only one that gets worse
   with time.

Ask before you explain. "Show me what you tried" separates all four faster
than any amount of watching.

## The first thirty days

1. **One lesson plan, one misconception.** Pick something beginners
   reliably get wrong and design the lesson around catching it.
2. **Live coding, slowly.** Type it out, make the mistake on purpose, and
   narrate the decision rather than the syntax. Beginners learn more from
   watching you debug than from watching you succeed.
3. **Ten minutes with the quiet one.** In every group there is somebody who
   has not spoken since week one. Going to them is the job.
4. **A handover.** Give somebody else your lesson plan and see whether they
   can run it. What is missing is what you were carrying in your head.

## The thing nobody warns you about

Week three. In almost every cohort, the people who are going to leave leave
in week three: the initial enthusiasm has gone, the material has got real,
and nobody has succeeded at anything visible yet. Build a small, definite
win into week two, and check in with everybody in week three whether they
asked or not.

## What sends work back

- **teaching by doing it for them.** A learner who can only work with you
  present has been carried. Scaffold, then take it away.
- **an exercise that can be copied.** If the previous solution passes,
  nobody learns.
- **a learner in the delivery.** Anonymise at source. Every write-up in this
  trade is about real people who did not ask to be evidence.

## Where to go next

- `#edu-teacher` on Discord.
- The review grid for your family is public.
- Look up *cognitive load theory* and *worked example effect*: two ideas
  with real evidence behind them that will change what you put on a slide.
$md$, 415),

('onboarding-education-curriculum', 'onboarding', 'education', 'curriculum', 'en',
 'Starting out in curriculum design',
 'Deciding what is learned, in what order, and how anybody knows it worked.',
$md$
# Starting out in curriculum design

A curriculum is read by people who were not in the room when it was decided.
That is the whole constraint: everything obvious to you has to be written
down, and everything written down has to survive being read literally.

## Objectives first, and observable

"Understands recursion" is not an objective — nobody can tell whether it
happened. "Writes a recursive tree traversal and predicts its depth" is: a
learner can aim at it and an assessor can check it.

Write the objectives before the content. A programme designed from a topic
list produces modules that are each defensible and add up to nothing.

## The silent jump

The most common defect in this domain, by a distance. Module four assumes
something module three did not teach, nobody notices because the author
already knew it, and half the room quietly falls behind.

Two habits catch it:

- write the prerequisites of every module explicitly, including the ones you
  think are too obvious to state — those are the ones;
- have somebody who does not know the subject read the sequence and mark
  where they would be lost.

## The first thirty days

1. **One module, complete.** Objectives, content, exercise, assessment,
   facilitator notes. Complete beats broad.
2. **A prerequisite map** of something that already exists. Take a published
   curriculum and draw what depends on what. You will find a jump.
3. **A rubric two people agree on.** Write criteria, then have two people
   grade the same work. Where they disagree, the rubric was vague.
4. **A handover.** Give your module to a trainer and watch them run it.

## Alignment

What is assessed is what gets learned, whatever the objectives say. If the
objectives are about designing systems and the assessment is a multiple
choice test on syntax, the programme teaches syntax. Check the alignment
last, every time, and change the assessment rather than the objectives.

## What sends work back

- **objectives that cannot be observed.** Understand, appreciate, be
  familiar with.
- **no facilitator notes.** Timings, what to do when a session runs long,
  where people get stuck, and the solutions. Without them only you can run
  it.
- **no expiry.** A curriculum that names tool versions and does not date
  them breaks silently.

## Where to go next

- `#edu-curriculum` on Discord.
- Cathy Moore's action mapping, for programmes commissioned by an
  organisation with a problem rather than a topic.
- The review grid for your family is public.
$md$, 420);

-- ═══════════════════════════════════════════════════════════════════
-- Toolkit
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES
('toolkit-education', 'toolkit', 'education', NULL, 'en',
 'Teaching toolkit',
 'What is enough to start with, what costs nothing, and the one thing worth paying for.',
$md$
# Teaching toolkit

Everything below is free or has a genuinely usable free tier, except where
said. None of it is required: a whiteboard and prepared exercises cover the
first six months.

## Preparing

- **Obsidian**, **Notion**, plain Markdown in a repository — anywhere you can
  version a programme. Version it: a curriculum without history is one nobody
  can review.
- **Miro**, **Excalidraw** — for the shape of a workshop before it has slides.
- **Reveal.js**, **Slidev**, **Marp** — slides written in Markdown, so they
  live next to the exercises in the same repository.

## The environment, which is where sessions actually fail

- **Dev containers** / **Docker Compose** — the setup a participant does not
  have to do.
- **GitHub Codespaces**, **Gitpod** — free tiers big enough for a workshop,
  and the answer for the laptop that will not cooperate.
- **Asciinema** — record a terminal as text: light, copyable, legible on a
  phone, and it does not need a video player.

## Delivering

- **BigBlueButton**, **Jitsi** — free software conferencing with breakout
  rooms.
- **OBS Studio** — recording, and streaming a session that somebody could not
  attend.
- **Excalidraw** again, shared, as the whiteboard everyone can draw on.

## Assessing

- **LibreForms**, **Framaforms**, **Google Forms** — for a check-in that
  takes ninety seconds.
- **Moodle** — free software, and the only full LMS on this list. Heavy;
  worth it only for a programme running repeatedly.
- **nbgrader**, **Autograding with GitHub Classroom** — where the exercise is
  code and the check can be automatic. Automate the objective part so your
  attention goes to the part that is not.

## Recording a course

- **DaVinci Resolve** — professional editing, complete free version.
- **Audacity** — cleaning up a voice track.
- **Whisper** — automatic captions to be reviewed. Never publish captions
  nobody read.

## The one worth paying for

A decent microphone. Every recorded module you ever make is limited by it,
and it is the only line item where a hundred euros changes something anybody
can hear.
$md$, 430);

-- ═══════════════════════════════════════════════════════════════════
-- Brief templates — written by whoever commissions the work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('brief-education-training', 'brief_template', 'education', 'teaching', 'en',
 'Brief — training delivery',
 'To be filled in before commissioning a workshop, a course or a cohort.',
$md$
# Brief — training delivery

## The learners
- How many:
- What they already know, honestly:
- What they do in their job:
- Are they attending voluntarily?

## The outcome
- What they must be able to do afterwards, stated as something observable:
- How you will know it worked:
- What happens if they cannot?

## The shape
- Format: [ ] workshop · [ ] short course · [ ] cohort · [ ] recorded
- Total hours in front of people:
- Over what period:
- Live, remote, or both?

## The environment
- Whose machines? What is installed on them?
- Who can install things, and how long does approval take?
- Is there a fallback for somebody whose setup fails?
- Network, screens, room layout:

## Delivery
- Materials handed over: [ ] slides [ ] exercises [ ] solutions
  [ ] facilitator notes [ ] recording
- Who owns them afterwards, and may the trainer reuse them?
- Number of runs included in the fee:

## Learner data
- Will the trainer see names, marks or assessments?
- What may leave the room, and with whose consent?
- Are any participants under 18? (if yes, say so now — it changes what may
  be collected at all)
$md$, 440),

('brief-education-curriculum', 'brief_template', 'education', 'curriculum', 'en',
 'Brief — curriculum design',
 'To be filled in before commissioning a programme somebody else will run.',
$md$
# Brief — curriculum design

## The problem
- What can people not do today?
- What changes in the organisation when they can?
- Why a programme rather than documentation or tooling?

## The learners
- Who, how many, and what they can already do:
- What they have time for, weekly:
- Voluntary or required?

## The programme
- Total duration and cadence:
- Delivered by whom? (the author, your staff, a third party)
- Existing material to build on or replace:
- Constraints: tools, languages, platforms that must or must not appear

## Assessment
- Does completion have to mean something formal?
- Who assesses, and what training do they have?
- Is there an appeal process? (there should be)

## Delivery
- Format of the handover: repository, documents, LMS import
- Facilitator notes expected: yes / no (say yes)
- Number of review rounds included:
- Who signs it off, and against what?

## Rights and maintenance
- Who owns the curriculum? May the author publish it?
- Who updates it when a tool version moves?
- Is a maintenance period part of this commission?
$md$, 450),

('brief-education-teaching-engagement', 'brief_template', 'education', 'teaching', 'en',
 'Brief — teaching engagement',
 'To be filled in before commissioning ongoing teaching: a term, a module, a series of cohorts.',
$md$
# Brief — teaching engagement

## The engagement
- Period, and hours per week:
- Number of learners per group, number of groups:
- Existing programme, or to be designed? (if to be designed, that is a
  separate commission)

## The learners
- Level, background, why they are here:
- Any under 18? (if yes, parental consent and data minimisation apply from
  the start, not later)
- What proportion are expected to complete, based on your history?

## The teaching
- Who else teaches on the programme, and how is it coordinated?
- Who handles a learner who falls behind?
- Office hours, follow-up, correction time: paid or assumed?

## Assessment
- What is assessed, by whom, against what:
- Correction time included in the fee: yes / no
- Who defends a grade if it is contested?

## Data
- What learner data does the teacher hold, and where?
- What must be deleted at the end, and when?
- What may the teacher publish about the engagement afterwards?

## Terms
- Fee, and what it covers (preparation and correction included or not):
- Cancellation of a session: by either side, with what notice
- What happens if the group does not fill
$md$, 460);

-- ═══════════════════════════════════════════════════════════════════
-- Writeup templates — filled in by whoever did the work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('writeup-edu-curriculum-doc', 'writeup_template', 'education', 'curriculum', 'en',
 'Curriculum design document',
 'The structure of a programme somebody else can run.',
$md$
# {Programme name}

**Audience:** · **Duration:** · **Version:** · **Last reviewed:**

## What learners will be able to do
Observable, testable, and written for the learner.

## Prerequisites
Including the ones that seem too obvious to state. Those are the ones.

## Sequence
| # | Module | Objectives | Depends on | Hours |
|---|---|---|---|---|

## Assessment
What is assessed, how, and against which objective.

## For the facilitator
Timings, what to cut if it runs long, where people get stuck, solutions.

## Maintenance
Tool versions named, and what breaks when each moves.
$md$, 470),

('writeup-edu-lesson-plan', 'writeup_template', 'education', 'teaching', 'en',
 'Lesson plan',
 'One session, in a form another teacher can run.',
$md$
# {Lesson title}

**Duration:** · **Audience:** · **Prerequisites:**

## Objective
What they will be able to do at the end.

## The misconception this catches
What learners reliably get wrong here.

## Plan
| Time | What happens | Who is doing what |
|---|---|---|

## Exercise
The task, the starting point, and the solution.

## Checking
How you know it landed, before they leave.

## If it runs long
What to cut, in order.
$md$, 475),

('writeup-edu-workshop-outline', 'writeup_template', 'education', 'teaching', 'en',
 'Workshop outline — three hours',
 'The shape of a hands-on session.',
$md$
# {Workshop title}

**Duration:** 3h · **Participants:** · **Level:**

## What they leave able to do

## Environment
What has to work before you start, and the fallback for when it does not.

## Plan
| Time | Segment | Watching or doing |
|---|---|---|
| 0:00 | Setup check | |
| 0:15 | | |

Keep the "doing" column longer than the "watching" one. That is the whole
design.

## Exercises
Each with its starting point, its solution, and its instructive failure.

## Checking
What you ask at the end to find out whether it worked.
$md$, 480),

('writeup-edu-cohort-syllabus', 'writeup_template', 'education', 'teaching', 'en',
 'Cohort syllabus — eight weeks',
 'What a cohort commits to, on both sides.',
$md$
# {Cohort name}

**Dates:** · **Places:** · **Weekly commitment:**

## Who this is for
And who it is not for. Being explicit saves people eight weeks.

## What you will be able to do at the end

## Week by week
| Week | Subject | What you do | Handed in |
|---|---|---|---|

## Week three
Say what happens in week three, and what support exists. It is where people
leave, and naming it in advance helps.

## Assessment
What is assessed, when, and against what.

## What we ask of you, and what you can ask of us
$md$, 485),

('writeup-edu-rubric', 'writeup_template', 'education', 'curriculum', 'en',
 'Assessment rubric',
 'Criteria two assessors can reach the same grade with.',
$md$
# Rubric — {what is being assessed}

**Objective assessed:**

| Criterion | Not yet | Approaching | Meets | Exceeds |
|---|---|---|---|---|

Each cell describes something observable in the work. "Good structure" is
not observable; "every function has one responsibility and its name says
which" is.

## Worked example
One piece of real work, graded, with the reasoning.

## Appeal
How a learner contests a grade, and who decides.
$md$, 490),

('writeup-edu-pedagogy-post', 'writeup_template', 'education', 'teaching', 'en',
 'Pedagogy write-up',
 'Writing about how people learn, from what you saw.',
$md$
# {Title}

## What I observed
Concretely, with numbers where there are any. Anonymised at source.

## What I think is happening
Your interpretation, marked as interpretation.

## What the literature says
If it says anything. If you did not look, say so.

## What I tried
And what happened.

## What I cannot conclude
The section that separates this from an opinion piece. One classroom is one
classroom.
$md$, 495),

('writeup-edu-post-mortem', 'writeup_template', 'education', 'teaching', 'en',
 'Training post-mortem',
 'What happened, honestly, while it is still fresh.',
$md$
# Post-mortem — {session or cohort}

**When:** · **Participants:** · **Completion:**

## What worked
Specifically enough to repeat.

## What did not
Specifically enough to fix. Include the setup problems.

## Where the room got lost
And whether you noticed at the time.

## Outcomes
What people could do afterwards, measured.

## What I change next time
Three things, in order.
$md$, 497),

('writeup-edu-outcomes-report', 'writeup_template', 'education', 'curriculum', 'en',
 'Learner outcomes report',
 'What changed, in a form that exposes nobody.',
$md$
# Outcomes — {programme}

**Cohort:** · **Learners:** · **Period:**

## Method
What was measured, how, and when. Before and after, on something observable.

## Results
| Objective | Before | After | Measured by |
|---|---|---|---|

Aggregate only. No row is one person.

## Completion
Started, finished, and — where known — why the others stopped.

## Satisfaction
Reported as a separate figure, and read as a signal about return rather than
about learning.

## Limits
Sample size, self-selection, what this does not show.

## Learner data
Confirm what was anonymised and what consent covers the rest.
$md$, 499);
