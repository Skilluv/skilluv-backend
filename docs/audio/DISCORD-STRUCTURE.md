# Discord — the audio corner

What to create, what to call it, and which channels have a rule of their own.
The server-wide setup — webhooks, the notifier binary, env vars — is in
[../DISCORD_SETUP.md](../DISCORD_SETUP.md); this is the audio structure that
sits on top of it.

---

## The shape, and why it is not one channel per trade

The backlog asked for five trade channels — composer, implementer, sound
designer, voice actor, programmer. Two of those talk to each other constantly
(the implementer and the programmer answer the same questions about builds and
budgets) and two of them are almost silent early on. Five channels for a
community of forty people is five empty rooms, and an empty room reads as an
empty platform.

So the channels follow the **four review families**, which is how the trades
are grouped everywhere else on the platform — the guides, the review grids, the
capabilities. When a family gets busy enough to be split, splitting it is a
Discord setting and not a rewrite of anything.

### Text

| Channel | For |
|---|---|
| `#audio-general` | everything, and the default landing place |
| `#audio-help` | one question, one answer. Not for showing work |
| `#audio-composition` | writing music |
| `#audio-sound-design` | effects, foley, ambiences |
| `#audio-voice` | performance, home recording, castings |
| `#audio-implementation` | middleware, engines, DSP |
| `#audio-showcase` | finished work only. Listening, not critique |
| `#audio-battles` | the forty-eight-hour duels and the composition contests |
| `#audio-missions` | paid work, posted by the platform |
| `#audio-castings` | open voice castings, posted by the platform |

### Voice

| Room | For |
|---|---|
| `Audio Cowork` | working alongside each other, microphones optional |
| `Composition Feedback` | playing a sketch before finishing it |
| `Voice Casting Sessions` | direction, run by whoever opened a casting |

## Roles

| Role | Granted from | What it changes |
|---|---|---|
| `Audio Creator` | an `audio-*` orientation on the profile | access to the family channels |
| `Audio Reviewer` | any `audio_reviewer:*` capability | can pin, can close a `#audio-help` thread |
| `Voice Actor` | the `audio-voice-actor` orientation | access to `Voice Casting Sessions` |
| `Audio Champion` | top twenty audio craft scores, refreshed monthly | nothing but the colour. It is a mark, not a power |
| `Featured Audio` | the `audio-featured` badge, rotating | nothing but the colour |

Roles are assigned from the platform, not by hand. A role somebody can ask a
moderator for is a role that ends up meaning who is friendly with the
moderators.

## Bot commands

- `/skilluv audio contests` — what is open right now
- `/skilluv audio castings` — open voice castings, deadline first
- `/skilluv audio featured` — this week's featured work

## The two rules that are specific to this corner

**`#audio-showcase` is for listening.** Post finished work, say what it is for,
and stop. Critique belongs in the family channel, where the person asked for
it. A public critique nobody asked for is the fastest way to make people stop
posting, and a domain where nobody posts their work has no way to prove
anything.

**Nobody uploads somebody else's voice.** Posting a synthetic voice trained on
a person — a member, a professional actor, anybody — is an immediate removal,
without the usual warning. This is the one moderation rule in the audio corner
that is not proportionate, and the reason is in
[VOICE-RIGHTS.md](./VOICE-RIGHTS.md): it is the only thing here that can take
somebody's trade away rather than annoy them.

## Moderation

The general community rules apply. Two additions worth stating to whoever
moderates this corner:

- **A licence question is a help question, not a legal one.** Point at
  [LICENSING.md](./LICENSING.md) and, past that, at the fact that nobody in the
  channel is a lawyer. A wrong answer given confidently here costs somebody
  their delivery.
- **Feedback on a performance is about the take, not the voice.** "The read is
  rushed" is useful. Anything about how somebody's voice sounds as a voice is
  not, and is a personal remark wearing a technical hat.
