-- The communication guides, toolkit, briefs and writeup templates.
--
-- Migration 0199 built `content_guides` and 0419 added the fourth kind. These
-- are rows because they have to be translated and edited by somebody who is
-- not deploying.
--
-- ## These are the first guides seeded in English
--
-- Everything in `content_guides` before this row set is French, because the
-- platform was written French-first. The repository's convention is now
-- English by default, and a domain opening today is written in it rather than
-- translated into it later.
--
-- That has one consequence worth stating: `routes::guides` fell back to
-- French when a requested locale had no row, which would have hidden every
-- guide below from a French reader. The fallback becomes
-- requested → English → French in the same change, so a half-translated
-- catalogue shows the untranslated page instead of claiming the guide does
-- not exist. French translations are rows with `locale = 'fr'`, added the
-- same way audio's English ones were in 0421.
--
-- ## Four onboarding guides, not five
--
-- One per reviewer family, as 0199 established and 0419 restated. The
-- developer advocate and the tech content creator share `advocacy` and share
-- a guide: what a newcomer needs to know first is the same for both — that
-- the audience can leave, that the promise in the title is a contract, and
-- that the demonstration has to have a plan B.
--
-- ## Five brief templates
--
-- Ticket F-05 asked for five, one per trade, and here the trade split is the
-- right one even though the review split is not. A brief is written by the
-- person commissioning the work, and somebody commissioning a translation and
-- somebody commissioning a conference talk have nothing in common to say.
--
-- ## Ten writeup templates
--
-- Ticket G-03's list, unchanged. They are the skeletons a member fills in,
-- and they are deliberately short: a template long enough to be impressive is
-- a template people replace with a blank page.

-- ═══════════════════════════════════════════════════════════════════
-- Onboarding — one per family
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-communication-documentation', 'onboarding', 'communication', 'documentation', 'en',
 'Starting out in technical documentation',
 'Writing for somebody with a task and no patience: where to begin, and what sends work back.',
$md$
# Starting out in technical documentation

Documenting is not explaining what the code does. It is answering the question
somebody who is stuck is actually asking, in the order they ask it.

## The four page types, and why mixing them fails

This distinction comes from the Diátaxis framework and it is the most useful
thing to know before writing a line:

- **Tutorial** — I am learning. You hold my hand, I decide nothing, I reach a
  result I can see.
- **How-to guide** — I have a specific task. I know what I am doing, I want
  the recipe.
- **Reference** — I am looking up a fact. Parameters, return values, errors.
- **Explanation** — I want to understand why it is built this way.

A page that does two of these loses both readers. The tutorial that detours
into architecture halfway through loses the beginner and bores the expert.

## The first thirty days

1. **One correction accepted upstream.** A wrong sentence, a dead link, a
   command that changed. It is small, and it is the real lesson: you discover
   a project's contribution process, which is half the trade.
2. **One missing reference page.** Take a public function documented by its
   signature alone and write what is missing.
3. **One tutorial, replayed.** Write it, then run it on a clean machine. The
   difference between the two is what you assumed without saying.
4. **One changelog.** The most thankless format, and the most useful.

## What sends work back

Three things, in this order of frequency:

- **the example does not run.** Dependency versions absent, an import
  missing, output that changed. An example nobody can execute costs more than
  no example.
- **a prerequisite was not announced.** The reader hits step 4 with something
  the author already knew. Write the prerequisites at the top, and assume
  nothing else afterwards.
- **the page does not say who it is for.** One opening line saying who this is
  for and what they will have at the end is worth three paragraphs.

## Where to go next

- The `#comm-tech-writer` channel on Discord.
- The review grids are public: read the one for your family before you submit,
  not after.
- Write the Docs (writethedocs.org): the community of the trade, and its
  Slack.
$md$, 310),

('onboarding-communication-advocacy', 'onboarding', 'communication', 'advocacy', 'en',
 'Starting out in speaking and content',
 'An audience that can leave: what that changes, and how to keep the promise in a title.',
$md$
# Starting out in speaking and content

The difference from documentation fits in one sentence: your reader was stuck
and had no choice, your audience has a choice and can leave. Everything
follows from that.

## The promise

The title is a contract. It says what is inside, and it is inside. A title
that promises more than the content does not cost you this time: it costs you
next time, and that is the only currency in this trade.

## The first thirty days

1. **One long-form article.** Write before you film. An unwritten talk is a
   talk that wanders.
2. **One ten-minute recorded demonstration.** One thing shown, start to
   finish, with no magic cut.
3. **One conference proposal.** Two hundred words for a committee: why this
   subject, why this room, why you. Send it even if you expect a no — writing
   it is already the exercise.
4. **One talk delivered.** A local meetup counts, and beats a big conference
   next year.

## The live demonstration

Prepare for the moment it breaks, because it will:

- pinned environment — versions locked, dependencies installed, no
  `npm install` in front of the room;
- a screenshot or recording of every step as a fallback;
- no need for the network, or a plan that works without it;
- one full rehearsal out loud, timed.

## What sends work back

- **the sound.** It is the first reason anybody closes a video, ahead of the
  picture and ahead of the content. A decent microphone and a room with
  curtains beat an expensive camera in an empty living room.
- **unreadable code.** Increase the font size. What is legible on your screen
  is legible neither at the back of the room nor on a phone.
- **the plateau.** Ten minutes where nothing new is said. Cut.
- **the abandoned comment section.** Questions are part of the delivery.

## Where to go next

- `#comm-devrel` and `#comm-content-creator` on Discord.
- DevRel Collective: the community of the trade.
- Open calls for papers are listed under the domain's opportunities.
$md$, 320),

('onboarding-communication-translation', 'onboarding', 'communication', 'translation', 'en',
 'Starting out in technical translation',
 'Holding a vocabulary across thousands of lines, and knowing what must not be translated.',
$md$
# Starting out in technical translation

Translating technical material is not transposing sentences. It is making a
series of vocabulary decisions and holding to them across thousands of lines,
including when you no longer remember what you decided.

## What does not get translated

Decide it at the start and write it down:

- API names, language keywords, command names;
- the error messages the program itself prints — the reader is going to paste
  them into a search engine;
- project names.

The trap is the other way round: terms that already have an accepted
translation in the target language, but not the one you would have picked.
Look at what other projects do before deciding alone.

## The first thirty days

1. **One short page, finished.** One finished page beats ten half-done.
2. **The glossary.** From the second page onward, write down the terms you
   settled and why. That is what makes the rest sustainable and reviewable.
3. **One review by somebody else.** In both languages. It is the family's
   rule: a translation is validated only by a person who reads both.
4. **One contribution to the i18n pipeline.** A concatenated sentence in the
   source is an untranslatable sentence in half the world's languages.

## Under-resourced languages

If you are translating into a language with no established technical
vocabulary yet — Wolof, Lingala, Bambara, and many others — you are not
translating, you are coining. So document every coinage: the term, what it
renders, why that choice. That document is worth as much as the translation.

## What sends work back

- **two translations of one term.** Makes the translated version harder than
  the original.
- **the calqued construction.** It shows up in the first sentence.
- **the source version not recorded.** Without it a maintainer cannot tell
  what is left to redo when the original moves.

## Where to go next

- `#comm-translation` and its per-language sub-channels.
- Weblate and Crowdin host free-software projects at no cost.
$md$, 330),

('onboarding-communication-research-writing', 'onboarding', 'communication', 'research-writing', 'en',
 'Starting out in research writing',
 'A text whose value rests on its method: what was measured, how, and what it does not prove.',
$md$
# Starting out in research writing

A whitepaper, an industry report and an external specification have one thing
in common: their value does not rest on the prose but on what a reader can
check.

## The structure that works

1. **The question.** What is being asked, and why it arises now.
2. **What already exists.** Read, cited, situated. Announcing something new
   without looking at the old is the most frequent and most expensive fault.
3. **The method.** Precise enough for a stranger to replay: protocol, data,
   versions, hardware.
4. **The results.** With their uncertainties, and the unfavourable cases as
   fully as the favourable ones.
5. **The limits.** Written by you. A document with no limits section is a
   brochure.

## The first thirty days

1. **A two-page prior-art review** on a subject you believe you know. You will
   discover somebody has already written it.
2. **One reproduced measurement.** Take a published result, replay it, write
   what you found — including when it is the same thing.
3. **One short whitepaper.** Fifteen pages with a real method beat forty
   without one.

## Citations, and the tool that writes for you

Every borrowed claim carries a reachable reference. A dead link is a missing
citation.

The research-writing challenges on this platform are set to
`human_verified`, and it is the only family where that is the case. The reason
is precise: a reference invented by a language model is indistinguishable from
a real one to a reader who trusts the document, and the whole value of
research writing is that its sources can be followed. Use the tool if you
want; open every link it gives you.

## Conflicts of interest

Funding, employer, product evaluated: stated at the top. An industry report
paid for by an actor in that industry reads differently, and the reader is
entitled to that.

## Where to go next

- `#comm-research` on Discord.
- Zotero for references, Overleaf if you move to an academic format.
$md$, 340);

-- ═══════════════════════════════════════════════════════════════════
-- Toolkit
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES
('toolkit-communication', 'toolkit', 'communication', NULL, 'en',
 'Technical communication toolkit',
 'What is enough to start with, and what costs nothing.',
$md$
# Technical communication toolkit

Everything below is free or has a genuinely usable free tier. None of it is
required to start: a text editor and a decent microphone cover the first six
months.

## Writing

- **Vale** — configurable prose linter. The one tool on this list people
  regret not adopting sooner: it catches the vocabulary inconsistencies human
  review lets through.
- **markdownlint**, **lychee** (dead links) — wire them into the pipeline.
- **LanguageTool** — grammar and style, free software, offline capable.
- **Obsidian**, **Zettlr**, **VS Code** — writing Markdown with your notes
  next to you.

## Publishing

- **MkDocs Material**, **Docusaurus**, **mdBook**, **Sphinx** — documentation
  site generators. The first asks you to make the fewest decisions.
- **Hugo**, **Zola**, **Eleventy** — for a personal blog that will still build
  in ten years.

## Speaking and showing

- **Reveal.js**, **Slidev**, **Marp** — slides written in Markdown, and
  therefore versioned with everything else.
- **OBS Studio** — recording and streaming. Free software, and the reference.
- **Asciinema** — record a terminal as text rather than video: light,
  copyable, legible on a phone.

## Editing

- **DaVinci Resolve** — professional editing, complete free version.
- **Shotcut**, **Kdenlive** — free software, lighter.
- **Audacity** — cleaning up a voice track.
- **Whisper** (openai-whisper, whisper.cpp) — automatic captions to be
  reviewed. Never publish captions nobody read.

## Translating

- **Weblate** — free software, free hosting for free-software projects.
- **Crowdin** — free for open source.
- **Poedit**, **OmegaT** — offline, for PO files and translation memories.

## Researching and citing

- **Zotero** — reference management, free software.
- **Overleaf**, **Typst** — for an academic format.
- **OpenAlex**, **Semantic Scholar** — finding what already exists, without a
  paywall.

## Sound, which matters more than the rest

A decent USB headset microphone and a room with curtains beat an expensive
camera in an empty room. It is the only line item where a hundred euros
changes something anybody can hear.
$md$, 350);

-- ═══════════════════════════════════════════════════════════════════
-- Brief templates — written by whoever commissions the work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('brief-communication-docs', 'brief_template', 'communication', 'documentation', 'en',
 'Brief — documentation',
 'To be filled in before commissioning documentation. Without these answers there is no commission.',
$md$
# Brief — documentation

## The reader
- Who are they? (level, role, what they already know)
- What are they trying to do when they land on this page?
- What will they have done when they leave it?

## The page type
- [ ] Tutorial (they are learning) · [ ] How-to (they have a task) ·
  [ ] Reference (they want a fact) · [ ] Explanation (they want to understand)

## Scope
- What is inside:
- What is explicitly outside:
- Target length:

## Technical context
- Repository / product:
- Versions covered:
- Who answers technical questions, and within what delay:
- Access required (repository, test environment):

## Delivery
- Format and location (repository, file format, existing style to follow):
- Date:
- Number of review rounds included:
- Who signs it off:

## Rights
- Author byline: yes / no
- Licence of the page:
- May the author show it in their portfolio: yes / no
$md$, 360),

('brief-communication-talk', 'brief_template', 'communication', 'advocacy', 'en',
 'Brief — talk',
 'To be filled in before commissioning a talk, a workshop or a demonstration.',
$md$
# Brief — talk

## The event
- Name, date, venue (or online):
- Expected audience: how many, what level, what they came for
- Exact duration, questions included:

## The point
- What the audience must have understood on the way out, in one sentence:
- What they must be able to do on the way out:
- Subjects to avoid:

## The demonstration
- Is there one? On what?
- Network available? Bandwidth?
- Who provides the environment?

## Delivery
- Materials expected, and in what format:
- Recording: by whom, published where, under what licence
- Rehearsal with the organiser: date

## Terms
- Fee, and what it covers (preparation included or not):
- Travel and accommodation: covered or not
- Exclusivity requested on the content: yes / no, for how long
$md$, 370),

('brief-communication-video', 'brief_template', 'communication', 'advocacy', 'en',
 'Brief — video content',
 'To be filled in before commissioning a video, a series or an episode.',
$md$
# Brief — video content

## The format
- [ ] Tutorial · [ ] Demonstration · [ ] Interview · [ ] Stream · [ ] Series
- Target duration:
- Number of episodes, and cadence:

## The subject
- What is shown, precisely:
- What the viewer can do at the end:
- Level assumed at the start:

## Production
- Who writes the script? Is it approved before shooting?
- Who supplies the code shown, and where does it live?
- Voice, face on camera, or screen capture only?
- Captions: which languages?

## Publication
- Whose channel? (yours, the author's, both)
- Author byline, and where it appears:
- Paid-partnership disclosure: it is mandatory — where are you putting it?

## Delivery
- Rendered files plus sources: yes / no
- Date, and number of revision rounds included:
$md$, 380),

('brief-communication-translation', 'brief_template', 'communication', 'translation', 'en',
 'Brief — translation',
 'To be filled in before commissioning a technical translation.',
$md$
# Brief — translation

## Languages
- Source language:
- Target language(s):
- Regional variant required? (pt-BR or pt-PT, fr-FR or fr-CA…)

## The content
- Exactly what, with a word or segment count:
- Source version to translate (commit, tag, date):
- File format:

## Vocabulary
- Existing glossary? Where?
- Terms not to translate:
- Decisions already made that must be honoured:

## Review
- Who reviews? Does that person read both languages?
- Is review part of this commission or a separate one?

## Delivery
- Where to file it (repository, translation platform):
- Is the glossary used delivered with it? (it should be)
- Date, and what happens when the source moves in the meantime:
$md$, 390),

('brief-communication-research', 'brief_template', 'communication', 'research-writing', 'en',
 'Brief — research writing',
 'To be filled in before commissioning a whitepaper, a report or a specification.',
$md$
# Brief — research writing

## The question
- The question the document answers, in one sentence:
- For whom, and for what decision:

## Scope
- What is studied:
- What is explicitly out of scope:
- Target length:

## Method
- Are there measurements to produce? On what?
- Who supplies the data, and in what form?
- May the author publish the method and the data? (if not, say so now: it
  changes what the document is allowed to claim)

## Independence
- Is the commissioner an actor in the field being studied?
- That relationship will be stated in the document. Where?
- May the commissioner ask for an unfavourable result to be removed?
  (the only good answer is no, and it is written before, not after)

## Delivery
- Format, licence, where the data is deposited:
- Date, review rounds included, who signs off:
$md$, 400);

-- ═══════════════════════════════════════════════════════════════════
-- Writeup templates — filled in by whoever did the work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('writeup-comm-docs-contribution', 'writeup_template', 'communication', 'documentation', 'en',
 'Documentation contribution',
 'The write-up of a contribution accepted upstream.',
$md$
# Documentation contribution — {project}

**Link to the contribution:**
**Merged on:**

## What was missing
For whom, and what that person could not do.

## What I wrote
Three lines. The link does the rest.

## What review changed
What the maintainers asked for, and what I learned from it.

## What I would do differently
$md$, 410),

('writeup-comm-tutorial', 'writeup_template', 'communication', 'documentation', 'en',
 'Step-by-step tutorial',
 'The skeleton of a tutorial somebody can follow without getting stuck.',
$md$
# {Title: what the reader will be able to do}

**Who this is for:**
**How long it takes:**
**What you will have at the end:** (a screenshot, an output, an address that
answers)

## Prerequisites
Everything that must be installed or known. Nothing beyond this is assumed.

## Step 1 — {verb}
What we do, the command, and what must appear.

## Step 2 — …

## Checking it worked
How the reader knows they succeeded.

## If it did not work
The two or three errors people actually hit, and their cause.

## Where to go next
One link, not five.
$md$, 420),

('writeup-comm-api-reference', 'writeup_template', 'communication', 'documentation', 'en',
 'API reference entry',
 'The skeleton of one reference entry.',
$md$
# `{signature}`

One sentence: what it does.

## Parameters
| Name | Type | Required | Default | Description |
|---|---|---|---|---|

## Returns
Type, and what the value means.

## Errors
| Condition | Error raised |
|---|---|

## Example
Copyable, runnable, with its output.

## Notes
Edge cases, behaviour under concurrency, deprecations. The version from which
this is true.
$md$, 430),

('writeup-comm-talk-outline', 'writeup_template', 'communication', 'advocacy', 'en',
 'Talk outline',
 'The skeleton of a talk, before any slides.',
$md$
# {Title}

**Duration:** · **Audience:** · **What they take away:**

## The hook (2 min)
The problem, shown rather than stated.

## The promise (1 min)
What the room will know at the end.

## The body (15 min)
- Idea 1 → demonstration → consequence
- Idea 2 → demonstration → consequence
- Idea 3 → demonstration → consequence

## The demonstration
What runs, and the plan B if it breaks.

## The close (2 min)
One thing to do on the way out. The links.

## Expected questions
The three I will be asked, and my answers.
$md$, 440),

('writeup-comm-blog-tutorial', 'writeup_template', 'communication', 'advocacy', 'en',
 'Article — tutorial',
 'The skeleton of an article that teaches something.',
$md$
# {Title: the outcome, not the technology}

**What you will be able to do at the end:**
**Prerequisites:**
**Complete code:** {link}

## The problem
A concrete situation, not an abstraction.

## The solution, step by step
Every code block is complete and runnable.

## What it gives you
The output, the screenshot, the measurement.

## Limits
What this approach does not do.

## Going further
$md$, 450),

('writeup-comm-blog-deep-dive', 'writeup_template', 'communication', 'advocacy', 'en',
 'Article — deep dive',
 'The skeleton of an article that explains why.',
$md$
# {Title}

## Why this subject now

## What is generally believed
And what is true in it.

## What actually happens
The mechanism, with sources and measurements.

## Practical consequences
What this changes for somebody writing code tomorrow.

## What I do not know
The section that separates an analysis from an opinion.

## Sources
Every link reachable.
$md$, 460),

('writeup-comm-video-script', 'writeup_template', 'communication', 'advocacy', 'en',
 'Video script',
 'What is said, and what the picture shows while it is said.',
$md$
# {Title} — script

**Target duration:** · **Format:**

## Hook (0:00–0:15)
| Voice | Picture |
|---|---|

## Announcement (0:15–0:45)
What the video is going to show.

## Body
| Voice | Picture | Duration |
|---|---|---|

## Close
What you take away, and the link.

## Shooting notes
What has to be prepared before pressing record.
$md$, 470),

('writeup-comm-podcast-outline', 'writeup_template', 'communication', 'advocacy', 'en',
 'Podcast episode outline',
 'The skeleton of an episode, interview or solo.',
$md$
# {Episode title}

**Guest:** · **Target duration:**

## Why this episode now

## What the listener knows at the end

## Questions
1. The opening question, broad.
2. …
Listen to the answer rather than to your next question. Let the silence work.

## What must not be forgotten
Questions I will regret not asking.

## Show notes
Everything cited, with its link.
$md$, 480),

('writeup-comm-translation-style', 'writeup_template', 'communication', 'translation', 'en',
 'Translation style guide',
 'The vocabulary decisions, written down once.',
$md$
# Style guide — {target language}

**Project:** · **Source version:**

## Register
Formal or informal address, and why. Impersonal constructions or not.

## What is not translated
API names, keywords, the program's error messages, project names.

## Glossary
| Source term | Chosen translation | Rejected | Why |
|---|---|---|---|

## Conventions
Dates, numbers, units, quotation marks, non-breaking spaces.

## Open questions
What still has to be decided, with the options.
$md$, 490),

('writeup-comm-whitepaper', 'writeup_template', 'communication', 'research-writing', 'en',
 'Whitepaper',
 'The structure of a text whose value rests on its method.',
$md$
# {Title}

**Author:** · **Date:** · **Version:**
**Interests declared:** (funding, employer, product evaluated)

## Summary
One paragraph: the question, the answer, and how much confidence it deserves.

## The question
What is being asked, and why now.

## What already exists
Read, cited, situated.

## Method
Protocol, data, versions, hardware. Enough to be replayed.

## Results
With their uncertainties. The unfavourable cases too.

## Limits
What this document does not prove.

## Conclusion

## Sources
$md$, 500);
