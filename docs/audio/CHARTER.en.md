# Audio domain charter

*To be published at `skill-uv.com/audio/charter`.*

This charter states what is required, what is refused, and what a validation
rests on. It is binding: a delivery that departs from it is refused, however
good the music.

---

## 1. What an audio delivery is

A delivery is an **artefact somebody can hold you to**: something a stranger
can listen to, use and judge without taking your word for it.

Admissible:

- a composition delivered with its stems and its licences declared;
- a coherent sound pack, named to a convention, with its usage sheet;
- a voice actor's demo reel, judged usable by a reviewer of the trade;
- an adaptive music system integrated and verified **in a playable build**;
- an audio feature shipped in an engine or a library;
- a credit on a released work.

Not admissible:

- a thirty-second excerpt presented as a track;
- an MP3 export with no master and no stems;
- a screenshot of a DAW session;
- an FMOD project that has never run in a game;
- a voice demo when nobody can tell whether the voice is yours.

The difference is not difficulty. It is verifiability.

## 2. Four non-negotiable requirements

**Declared provenance.** Every sample, loop or library used is declared with
its licence — or everything is original and that is written down. **This is the
strictest requirement in the domain, and the only one that can make a delivery
unusable on its own**: an untraced source exposes the client to a takedown and
the author to a claim, months after delivery.

**Measured level.** Integrated loudness (LUFS) and true peak are measured, not
estimated, and matched to the destination. "It sounds loud" is not a
measurement. The platform measures the files it receives; the gap between the
level you aimed at and the one you hit is what a reviewer looks at.

**Stems, whenever there is a commission.** Without separated tracks a client
cannot adjust anything without coming back to the author. A composition
delivered without stems is incomplete, not protected.

**Serving the thing it is attached to.** Sound serves the game, the picture,
the interface, the story. Work you notice at the expense of what it accompanies
has missed, however well made.

## 3. What each trade keeps

The five trades do not sign away the same thing, and the contract says which
before the recording, not after.

| Trade | Usually granted | Kept |
|---|---|---|
| Composer | a licence to use the work | ownership of the work, unless explicitly and separately bought |
| Sound designer | the delivered files, for the agreed use | the techniques, the raw recordings, the right to make it again |
| Voice actor | a bounded use of the recording | the voice itself, always |
| Implementer | the middleware project and its integration | reusable tools and templates |
| Audio programmer | the delivered code, under the agreed licence | the generic pieces, unless otherwise agreed |

**Portfolio use is the default.** A creator who cannot show what they made
cannot prove they made it, and that is the only currency on this platform. A
clause that forbids it exists — `buyout` — and it is visible, separate, and
paid for.

## 4. Four licence scopes, and why the question is asked

Ownership and licence are two different questions, and in music they almost
always have two different answers. Every audio mission must say which applies:

- **sync only** — use to picture, in the named work;
- **limited commercial** — one medium, one territory, one duration;
- **worldwide commercial** — no territorial or time limit;
- **exclusive** — the client alone may use the work, and that is paid for.

A commission with no stated scope is the first cause of dispute in this trade:
the client assumes "worldwide" and the composer assumes "that game".

## 5. Generative AI

**Learning: allowed, declared.** Using Suno, Udio, MusicGen or a source
separation tool to learn, prototype or unblock yourself is accepted. It is
declared, as everywhere else on the platform. Hiding it is not.

**Paid missions: forbidden without the client's written agreement.** A client
commissioning original music is buying human intent and a chain of rights they
can defend. The legal status of generative output is unsettled in most
jurisdictions, and handing them that exposure without telling them is a fault.

**Synthetic voice: forbidden without the person's written consent.** No
exception, no implied clause, no "it was only a test". A voice is an attribute
of somebody; training on it without their agreement is what this platform
refuses most firmly. See [VOICE-RIGHTS.en.md](./VOICE-RIGHTS.en.md).

## 6. What a validation rests on

The family's review grid, public and readable before you submit: composition,
sound design, voice, implementation. It is applied by a reviewer holding the
matching capability — `audio_reviewer:composition`,
`audio_reviewer:sound-design`, `audio_reviewer:voice`,
`audio_reviewer:implementation`.

An attestation for a composition or a pack is not issued until the source
declaration is complete. That is not a formality: it is half of what the
attestation asserts.

## 7. The documents that go with this

- [LICENSING.en.md](./LICENSING.en.md) — samples, registration, sync.
- [VOICE-RIGHTS.en.md](./VOICE-RIGHTS.en.md) — voice rights, non-compete,
  cloning.
- Brief and writeup templates are served by the API
  (`/api/guides?domain=audio`) rather than kept in this repository: they are
  translated and edited by people who are not deploying.
