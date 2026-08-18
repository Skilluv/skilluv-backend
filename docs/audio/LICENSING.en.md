# Music licensing — what to know before you deliver

*To be published at `skill-uv.com/audio/licensing`.*

> **This is not legal advice.** It is what the platform requires and what it
> understands of the trade's practice. A mission beyond a few thousand euros,
> an exclusivity, or a dispute warrants a specialist lawyer. Legal review of
> this document is open and will be shared with the other domains.

---

## 1. The rule that governs everything else

**An untraced source makes the delivery unusable.**

Not "weaker": unusable. A client who discovers that a loop in your track comes
from a pack you have no licence for has three options, and all three cost them:
pull the work, remake it, or negotiate after the fact. They choose none of them
without making you pay for it.

That is why the platform asks for a **source declaration** before issuing a
composition or pack attestation. The declaration is a sentence you sign: *this
list is complete and accurate*. An empty list with that sentence is perfectly
valid — it means "everything is original". An empty list without it means
"nobody filled the form in", and those are not the same thing.

## 2. The six kinds of source

| Kind | What it requires |
|---|---|
| `original` | nothing to attribute. Say so anyway. |
| `public_domain` | public domain or CC0. No conditions. |
| `creative_commons` | **the credit line, verbatim**. Some variants forbid commercial use. |
| `royalty_free` | bought or subscribed. Keep the receipt and the licence number. |
| `licensed_commercial` | a licence negotiated for a named use. |
| `third_party_work` | somebody else's work or performance. |

### Creative Commons: free does not mean unconditional

- **BY** — credit required, in the exact wording the author asks for. Using the
  sound without the credit is not an oversight, it is infringement.
- **BY-SA** — the credit, **and** the derivative under the same licence. That
  spreads to your track: avoid it in client work unless the client knows and
  agrees.
- **BY-NC** — no commercial use. **A paid mission is commercial use**, including
  when the game is free and funded some other way.

Freesound and OpenGameArt mix all three. The licence is per file, not per pack.

### Paid libraries

Splice, Kontakt, EastWest and the rest normally sell a personal,
non-transferable licence to use the material in renders. Two consequences
people discover late:

1. **You cannot deliver the raw samples** to the client, only the render that
   contains them.
2. **The licence does not transfer** with the work. If the client wants to
   remix, they need their own licence from the library.

Declare the library once, with its licence number if it has one.

## 3. Registration: SACEM, ASCAP, BMI

Registering a work with a collecting society is about collecting performance
royalties — radio, television, platforms, public venues. It does **not**
replace the contract with the client, and it can conflict with it.

Three things to know before registering:

- **Registration does not create copyright.** You hold it from creation.
  Registration is about being paid when the work is broadcast.
- **A registered work can no longer be freely assigned.** As a SACEM member you
  have assigned your performance rights to them: a full buyout becomes
  incompatible with your status. Tell the client before, not at signature.
- **Games are badly covered.** Performance royalties on music played inside a
  game collect poorly or not at all depending on territory. Do not count on
  them to make up for a low fee.

Joining is neither compulsory nor always wise early in a career. It is a
decision to take knowingly, not a rite of passage.

## 4. Sync, mechanical, performance

Three distinct rights, often confused:

- **Synchronisation** — pairing music with picture or software. The one that
  matters in nearly every mission on this platform.
- **Mechanical** — reproducing the work on a medium. Mostly relevant to covers.
- **Public performance** — playing or broadcasting the work publicly. What a
  collecting society collects.

A Skilluv mission normally grants a **sync licence** of a declared scope. The
mission's `licensing_scope` field is that scope.

### Covers

A cover needs a mechanical licence from the rights holder, and a cover in a
game additionally needs a sync licence — which the holder may refuse without
reason. In practice: **do not accept a cover commission until the client has
the permissions in writing.**

## 5. Generated music

The legal status of output from Suno, Udio or MusicGen is unsettled. As of
2026, depending on jurisdiction, it may carry no copyright at all — meaning
anybody, including the client's competitor, can reuse it — and may attract
claims over training data.

The platform's position:

- **learning and prototyping**: allowed, declared;
- **paid missions**: forbidden without the client's written agreement, because
  they have to know what they are buying;
- **declaration**: a generated track is declared as a source, like a sample.

## 6. The minimum contract

Five lines, before recording:

1. **What is delivered** — formats, stems, dates.
2. **What is granted** — ownership, or a licence and its scope.
3. **Where and for how long** — territory and duration.
4. **Exclusive or not.**
5. **Portfolio** — yes by default; if no, why and at what price.

A template per scope ships with the brief templates
(`/api/guides?domain=audio&kind=brief_template`).
