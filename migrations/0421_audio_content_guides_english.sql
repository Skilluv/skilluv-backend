-- The audio guides in English.
--
-- Same split as migrations 0302 and 0304 did for AI: the French is written
-- first because that is where the words are decided, and the translation is a
-- second file because it lands later and should not hold the first one up.
--
-- `content_guides` is unique on `(slug, locale)`, so these are rows beside the
-- French ones rather than replacements, and `routes::guides` already falls
-- back to French for anything not translated — which is why a partial
-- translation is safe to ship and a missing one is not a 404.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-audio-composition', 'onboarding', 'audio', 'composition', 'en',
 'Starting out in composition',
 'Writing music for something other than yourself: what the brief fixes, what it leaves you, and where to begin.',
$md$
# Starting out in composition

Scoring a project is not composing and then looking for a project. The
constraint comes first — a length, a mood, a scene, a budget of instruments —
and the craft is making something alive inside it.

## The first thirty days

1. **A short piece.** A podcast ident or a jingle: fifteen seconds forces you
   to decide immediately what the music is saying.
2. **A loop.** Three minutes that repeat without anybody hearing the seam. It
   is as much a technical exercise as a musical one, and it teaches form.
3. **A variation.** Take your own theme and write a second version with the
   same identity and a different job. Every other part of this trade uses that
   move.
4. **To picture.** Write against a cut you did not choose.

## Why work comes back

Three things, in order of how often:

- **the sources are not declared.** A bought loop, a library, a Freesound
  sample: each one is declared with its licence. A single untraced source makes
  the piece unusable to a client, whatever else is true of it.
- **the level is not measured.** Write down the LUFS you are aiming at and
  check it. "It sounds loud" is not a measurement.
- **the stems are missing.** Without separated tracks the client cannot adjust
  anything without coming back to you. That is an incomplete delivery, not a
  service.

## Tools

Reaper is enough and costs sixty euros once. Ardour is free. Free libraries
passed the point where gear decides the result some years ago. See the
[audio toolkit](/guides/toolkit-audio).

## Where the people are

`#audio-composer` and `#audio-general` on Discord. The "Composition Feedback"
voice room is where you play a sketch before finishing it, which is more useful
than an opinion on a finished track.
$md$, 10),

('onboarding-audio-sound-design', 'onboarding', 'audio', 'sound-design', 'en',
 'Starting out in sound design',
 'Making sounds that do a job: function before material, and coherence before both.',
$md$
# Starting out in sound design

A designed sound is almost never heard alone. It arrives mid-action, stacked
with three others, to tell something to somebody who is looking elsewhere. That
context decides whether it is good.

## The first thirty days

1. **Ten interface sounds.** Short, discreet, distinguishable. The constraint
   "bearable on the hundredth play" kills half your ideas.
2. **A stack.** Build an impact in three layers — snap, body, tail — and listen
   to what each one contributes by removing it.
3. **An ambience.** A place that exists to the ear: a bed, sparse events, slow
   movement. This is where you learn patience.
4. **A pack.** Twenty sounds that visibly belong to the same world. Coherence
   is what separates a pack from a collection.

## Recording rather than buying

Recording your own is not a purity question: a library sound has been used a
thousand times and a listener recognises it without knowing why. An entry-level
microphone and a quiet room already beat the library on the thing that matters,
which is being yours.

## Why work comes back

- **naming.** Twenty files called `final_2_ok.wav` are twenty files an
  integrator cannot use. Pick a convention and apply it.
- **the usage sheet.** Which sound for which situation, at what level. Without
  it the pack gets used badly and your work looks wrong.
- **the sources.** As everywhere here: declared, with their licences.

## Where the people are

`#audio-sound-designer`, and `#audio-battles` for the forty-eight-hour duels —
the fastest way to learn, because you hear another answer to the same brief
immediately.
$md$, 20),

('onboarding-audio-voice', 'onboarding', 'audio', 'voice', 'en',
 'Starting out in voice',
 'The take, the direction, and the rights — in that order, because the third is the one people forget.',
$md$
# Starting out in voice

The trade is three unrelated skills: performing, recording cleanly, and knowing
what you are signing away. The third is the one that costs the most when it is
missing.

## The first thirty days

1. **The room before the microphone.** A wardrobe full of clothes is a better
   booth than an empty living room with a thousand-euro mic. Treat first.
2. **Five minutes of narration.** Holding a rhythm over time is what separates
   a professional voice from a nice one.
3. **Five characters on the same lines.** The only exercise that demonstrates
   range instead of claiming it.
4. **The demo reel.** Last, not first: a reel made before doing the work shows
   what you think you can do.

## Your rights, on one page

- **Your voice is yours.** It is an attribute of you, not a file. No use is
  implied.
- **Write the scope.** Medium, territory, duration, exclusivity. Four
  questions, four answers, before recording.
- **Portfolio.** Keep the right to show what you made, unless there is a
  specific reason and it was paid for. Without it you cannot prove your own
  work.
- **Cloning.** Skilluv forbids training a synthetic voice on yours without your
  explicit written agreement. Asked elsewhere, it is a negotiation of its own,
  not a clause in the small print.

## Castings

Castings here are blind by default: the creator hears the takes without the
names. That exists so a first time is possible.

## Where the people are

`#audio-voice-actor`, and the "Voice Casting Sessions" voice room.
$md$, 30),

('onboarding-audio-implementation', 'onboarding', 'audio', 'implementation', 'en',
 'Starting out in audio integration and programming',
 'Sound at runtime: middleware, budget, and the audio-thread rule.',
$md$
# Starting out in audio integration and programming

Here a sound does not exist until it fires at the right moment in a build.
Anything that sounds right in the editor and not in the game does not count.

## Two paths that meet

- **Integration.** FMOD, Wwise, or the bare engine. You are not writing the
  music, you are writing its behaviour.
- **Programming.** The level below: DSP, spatialisation, synthesis. You write
  what the middleware calls.

They share a review family because they share a question: does it arrive
correctly, in time, within budget.

## The first thirty days

1. **A simple integration, checked in a build.** One sound fired by one game
   event. Get into the habit of testing in the build and nowhere else.
2. **Vertical remixing.** Three layers that switch without breaking the bar.
   The first place music becomes a system.
3. **The budget.** Count voices, memory, streaming. On the target, not on your
   machine.
4. **A degraded case.** What happens when a bank is missing or everything fires
   at once. A chosen silence beats an accidental blowout.

## The audio-thread rule

No allocation, no lock, no I/O on the audio thread. This is not a performance
tip: a two-millisecond stall is a click everybody hears. If you take one thing
from this guide, take that.

## Where the people are

`#audio-music-implementer` and `#audio-programmer`.
$md$, 40),

('toolkit-audio', 'toolkit', 'audio', NULL, 'en',
 'Audio tools',
 'What to install per trade, what it costs, and the free equivalent where one exists.',
$md$
# Audio tools

Written under one constraint: somebody starting with zero euros must be able to
do everything the platform asks. Paid software is listed because it exists in
the industry, never because it is required.

## Workstations (DAWs)

| Tool | Cost | Note |
|---|---|---|
| **Reaper** | $60 personal licence, unlimited evaluation | The independent standard. Light, scriptable, runs on an old machine. |
| **Ardour** | Free | Complete, cross-platform. The default at zero budget. |
| **Audacity** / **OcenAudio** | Free | Editing and cleanup, not composition. |
| Logic Pro, FL Studio, Ableton, Cubase, Pro Tools | €200–600 | Common in the industry. None is required here. |

## Game middleware

| Tool | Cost | Note |
|---|---|---|
| **FMOD Studio** | Free under $200k revenue | Easiest to learn. |
| **Wwise** | Free under $200k | More powerful, more verbose. |
| **Godot AudioStreamPlayer**, **bevy_audio** | Free | No external dependency. Enough for many projects, and the only path when the target platform is one the middleware does not cover. |

## Processing

The free suites from **Melda Production** and **Voxengo** cover EQ, compression
and loudness metering. **Youlean Loudness Meter** (free) gives you the LUFS the
review grid asks for.

For voice, **iZotope RX** is the industry cleanup tool; its free equivalent is
patience and clean editing.

## Recording

A hundred-euro dynamic microphone in a treated room beats a thousand-euro one
in a bare room. Treat first: blankets, mattresses, an open wardrobe. A usable
chain starts around a hundred and fifty euros.

## Libraries and sources

- **Freesound**, **OpenGameArt** — free, with licences to read.
- **Airwindows** — free processing, professional quality.
- Splice, Kontakt, EastWest — paid, and to be declared in your licences as soon
  as any excerpt reaches a render.

## Programming

**JUCE** (C++) for plugins, **cpal** and **fundsp** (Rust), the **Web Audio
API** for the browser, the FMOD and Wwise APIs for integration.
$md$, 100),

-- ── Brief templates ────────────────────────────────────────────────

('brief-audio-composition', 'brief_template', 'audio', 'composition', 'en',
 'Brief — composition',
 'What to write before commissioning music, so that what arrives is what you expected.',
$md$
# Brief — composition

Copy this and fill it in. Every blank line is one more round trip.

## The project
- What is it, and what is the music doing in it?
- Where will it be heard: headphones, phone, room, under speech?

## The piece
- **Length**, and whether it must loop.
- **Mood** in three adjectives, plus a counter-example: "not triumphant".
- **References**: two or three existing tracks, with what you like in each. A
  reference with no comment says only "do that".
- **Instrumentation** wanted or excluded.

## The delivery
- **Formats**: WAV 48 kHz / 24-bit by default, plus compressed versions.
- **Stems**: yes by default. Say no and you can no longer adjust anything.
- **Loudness** target, per destination.
- **Date**.

## The rights
- **Scope**: sync only, limited commercial, worldwide, exclusive?
- **Duration** and **territory**.
- Who keeps ownership, and who may show the work in a portfolio?

## Revisions
Five rounds by default on this platform. Say what you intend to spend them on.
$md$, 200),

('brief-audio-sound-pack', 'brief_template', 'audio', 'sound-design', 'en',
 'Brief — sound pack',
 'Commissioning effects: the list, the use, and the integration format.',
$md$
# Brief — sound pack

## The use
- Where do these fire, and what do they tell whom?
- What will they sit on top of? A sound that is perfect alone can vanish in the
  final mix.
- Target platform and constraints: size, format, memory.

## The list
A table, one row per sound: name, situation, rough length, note. A precise list
beats an adjective: "twenty combat sounds" can be delivered twenty ways.

## The style
- Realistic, stylised, retro? One reference per family.
- Do frequently repeated sounds need variations?

## The delivery
- **Formats** and sample rate.
- **Naming convention**: give yours if you have one.
- **Usage sheet** expected.

## The rights
Scope, exclusivity, portfolio — the same four questions as everywhere.
$md$, 210),

('brief-audio-voice', 'brief_template', 'audio', 'voice', 'en',
 'Brief — voice',
 'Commissioning a voice: the character, the audition lines, and the scope of use.',
$md$
# Brief — voice

## The character
- Who is this? Age, situation, what they want, what they hide.
- How do they speak when things are fine, and when they are not?
- **Counter-example**: which voice would be wrong for this part?

## The language
Language and variant — French from Cotonou, Montréal and Lyon are not
interchangeable, and saying so avoids an expensive misunderstanding.

## The audition lines
Three to five lines from the real text, covering different registers. Everybody
reads the same ones, or the takes are not comparable.

## The volume
- Final line count, and delivery rhythm.
- Will there be pickups when the text changes?

## The delivery
- Format, edited or raw, breaths kept or removed.
- File naming, especially past fifty lines.

## The rights — the part not to leave blank
- **Medium, territory, duration, exclusivity.**
- **Portfolio**: may the actor show an excerpt? The answer is yes unless there
  is a specific reason.
- **Synthetic voice**: training a model on these takes needs a separate written
  agreement. Without it, it is forbidden on this platform.
- **Sequels and pickups**: reuse in a later version is agreed here or
  renegotiated later.
$md$, 220),

('brief-audio-adaptive', 'brief_template', 'audio', 'implementation', 'en',
 'Brief — adaptive music',
 'Commissioning an integration: the game states, the transitions, and the budget.',
$md$
# Brief — adaptive music

## The game
- Engine and version. Middleware already in place, or to be chosen?
- Who on the team wires the events? That question decides half the work.

## The states
A list of the situations the music must distinguish, and what triggers each in
code. "Combat" is not a state until somebody knows which event announces it.

## The transitions
- Which switches must be imperceptible, which can be hard cuts?
- What delay between event and musical reaction is acceptable?

## The budget
- Memory available for audio, and simultaneous voices.
- The most constrained target platform.

## The delivery
- Middleware project, demo build, documentation of the exposed parameters.
- Who maintains the integration after delivery?
$md$, 230),

('brief-audio-programming', 'brief_template', 'audio', 'implementation', 'en',
 'Brief — audio development',
 'Commissioning audio code: the problem, the target, the performance budget.',
$md$
# Brief — audio development

## The problem
Describe what does not work or what is missing, not the solution you have in
mind. "We need HRTF" is a solution; "you cannot tell a sound is behind you" is a
problem, and it may have three answers.

## The target
- Engine, language, platforms.
- Buffer size and sample rate in production.
- What already exists and must not break.

## The budget
- Acceptable CPU cost, as a share of a core or milliseconds per block.
- Memory.
- Maximum tolerable latency.

## The delivery
- Code, licence, and where it lives.
- A minimal example that runs, and what counts as a demonstration.
- Documentation expected: integration, error cases, known limits.
$md$, 240),

-- ── Writeup templates ──────────────────────────────────────────────

('writeup-composition-notes', 'writeup_template', 'audio', 'composition', 'en',
 'Composition notes',
 'What accompanies a delivered piece: the intent, the choices, what was rejected.',
$md$
# Composition notes — {title}

## Intent
What the music should make somebody feel, in two sentences. Not what it
contains: what it is aiming at.

## How it is built
The main material, where it returns, how it is varied. Overall form.

## The choices, and what they rule out
Two or three decisions, with the alternative that was tried and dropped. A
reviewer judges a choice better knowing what it excludes.

## References
The starting points, and where the piece deliberately departs from them.

## Technical
Measured integrated loudness, true peak, loudness range. Sample rate and depth.
Stem list.

## Sources
Every sample, loop or library with its licence — or "all original".
$md$, 300),

('writeup-sound-pack-usage', 'writeup_template', 'audio', 'sound-design', 'en',
 'Sound pack usage sheet',
 'The document without which a pack is used badly and looks wrong.',
$md$
# Usage sheet — {pack name}

## What is in the pack
Number of sounds, families, naming convention explained in one line.

## Which sound for which situation
A table: file, situation, suggested level, note. This is the part an integrator
reads, and often the only one.

## How it is made
The common principle: which layers, which space, which grain. What gives the
pack its unity.

## Integration
Delivered format and why. Sounds that benefit from random variation. Mixing
cautions — what these conflict with.

## Sources
Every recording, sample or library with its licence.
$md$, 310),

('writeup-voice-reel-notes', 'writeup_template', 'audio', 'voice', 'en',
 'Demo reel description',
 'What accompanies a reel: the contents, the conditions, the scope of use.',
$md$
# Demo reel — {name}

## Contents
For each excerpt: character or register, context, length, language and variant.

## What the reel demonstrates
Three honest lines about the range actually shown. A reel that promises more
than it contains is found out in the first session.

## Recording conditions
Microphone, interface, room treatment. Editing applied, if any.

## Technical
Loudness, true peak, format.

## Rights
What somebody hearing this reel may do with it. Excerpts from commissioned work
appear with the client's agreement, or not at all.
$md$, 320),

('writeup-adaptive-implementation', 'writeup_template', 'audio', 'implementation', 'en',
 'Adaptive integration report',
 'How the system is built, what it costs, and where it breaks.',
$md$
# Integration — {project}

## The system
The states, what triggers them, and the map of allowed transitions. A diagram is
worth a page.

## The layers
What makes up each state, and what remains when one is removed.

## The exposed parameters
Each with its name, range and effect. This is what lets somebody else wire a new
event without asking you.

## The budget
Measured simultaneous voices, memory, streaming, CPU cost. On the target.

## Edge cases
Fast switching, back and forth, missing bank, everything at once. What the
system does, and why that is the chosen behaviour.

## What is left
Known limits, written by you rather than found by the reviewer.
$md$, 330),

('writeup-audio-programming-breakdown', 'writeup_template', 'audio', 'implementation', 'en',
 'Audio development breakdown',
 'The problem, the method, the measurements, the limits.',
$md$
# {system name}

## The problem
What was missing, and why existing solutions did not fit.

## The approach
The algorithm or architecture, in terms a non-specialist developer can follow.
The trade-offs taken.

## The measurements
Cost per block, allocation on the audio thread (ideally: none), latency, memory.
On which hardware, at which buffer size.

## Validation
How you know it is correct: tests, comparison against a reference, test signals,
listening.

## Limits
What it fails on, and what was not addressed. Written by you.

## Reuse
Licence, dependencies, a minimal example that runs.
$md$, 340),

('writeup-audio-licensing', 'writeup_template', 'audio', NULL, 'en',
 'Source and licence declaration',
 'The document that makes an audio delivery usable. Without it nothing else counts.',
$md$
# Sources and licences — {delivery}

## Declaration
> I declare that the list below is complete and accurate, and that no other
> third-party source is present in this delivery.

## Third-party sources
A table: source, where it was obtained, exact licence, required attribution
wording verbatim, commercial use permitted or not.

One row per source. A commercial library is declared once, with its licence
number if it has one.

## Original elements
What was created for this delivery, and who owns it at the end.

## Credits to display
The credit block to copy verbatim, if there is one. Creative Commons BY licences
are free only on that condition.

## Voice
For any voice recording: performer's name, scope of use granted, and an explicit
statement of agreement or refusal regarding synthetic-voice training.
$md$, 350),

('writeup-audio-post-mortem', 'writeup_template', 'audio', NULL, 'en',
 'Audio post-mortem',
 'What was learned, written while you still remember it.',
$md$
# Post-mortem — {project}

## The frame
Duration, budget, exact role, with whom.

## What worked
Two or three things, and why — the cause, not the result.

## What did not
Two or three things, stated without looking for somebody to blame. The vague
brief and the one revision too many appear here more often than the technique.

## Revisions
How many rounds, on what, and what would have avoided them. The most useful
section for the next commission.

## What I would do differently
Three concrete sentences.
$md$, 360);
