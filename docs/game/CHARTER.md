# The game domain charter

The rules a game creator, a playtester and a reviewer all agree to. Short,
because the lines that matter are few and hard.

## The work is yours

You keep the rights to what you make. Skilluv records the proof — a link, a
build, a validated slice — never a claim of ownership over your game. An
attestation says "this person made this and it was reviewed", nothing more.

## What you keep, by how you made it

The rights follow the format the work was made in. Four cases, and they are not
the same:

- **Individual project** — you keep every right. Skilluv holds only a licence to
  show the work: to display it on your profile, in a portfolio, in a listing.
  Nothing else.
- **Community / open-source project** — the licence is the upstream project's,
  whatever it chose (MIT, GPL, CC, …). Contributing means accepting that
  licence; the merged work lives under it, not under a Skilluv one.
- **Game jam** — you keep the rights to your submission. Skilluv receives a
  right to showcase it — in the jam results, a highlight reel, the featured
  page — and nothing more.
- **Paid mission** — the IP terms are the contract's. Who owns the deliverable,
  what the client may do with it and what you may show afterwards are settled in
  the mission agreement before work starts (see the mission section). Skilluv
  does not override a contract either way.

## Third-party assets are credited, always

A game is built on other people's work: an engine, a font, a sound pack, a
shader from a tutorial. Every asset that is not yours is credited, and its
licence is respected — Kenney, Freesound, OpenGameArt, an itch asset pack, a
marketplace model: name the source and honour its terms. A slice that ships
someone else's asset as its own is not a validation problem — it is the end of
the relationship. This is the one rule with no second chance, because passing
off is not a mistake of craft.

## Engine licences are yours to keep too

The engine you build in comes with its own terms, and they are yours to respect:

- **Godot, Bevy, LÖVE** — open source (MIT and the like). Nothing to owe, and
  the reason the domain leans on them first.
- **Unity, Unreal** — free to start, under an EULA, and Unreal takes a royalty
  past a revenue threshold. If you ship commercially on one of these, the
  royalty and the licence are yours to settle with the vendor; Skilluv is not a
  party to it and does not collect on it.

Naming the engine on a slice is not bureaucracy — it is what lets a reviewer
know which terms apply and which build checks to run.

## Mods live inside someone else's game, within its rules

A mod is content for a game you do not own. Two lines hold:

1. **Publish on the platform that game uses** — Nexus, CurseForge, the Steam
   Workshop, Fortnite Creative — and register the live URL here. Skilluv never
   hosts the package. It holds the proof and the metadata; the file stays where
   the game's community expects it.
2. **Keep the vendor's terms.** The first thing a reviewer checks before
   confirming a mod is that the game's modding terms were kept and nothing
   proprietary was re-hosted. Broken, that line ends the relationship, the same
   as passing off an asset.

A mod is confirmed by a community reviewer, not its author. A confirmed mod
becomes a deliverable and earns `game_mod_published`.

## Playtesting is evidence, and it is honest

A game slice reaches validated only after at least three playtests with an
average fun score of three. That is not a formality:

- **You cannot playtest your own slice.** The service refuses it.
- **A verdict is a verdict, not a ballot.** One row per person per slice,
  editable in place — you do not stack ten sessions to inflate a score.
- **Giving playtests is service to the domain.** Twenty of them, each with the
  questionnaire filled in, earns `game_playtest_hero` — recognition that does
  not move a rank, because supplying play is not shipping a game.

## Jams are won on the work, not the turnout

A jam is scored on a submission's **average** vote across every axis, so a game
with five votes and a game with fifty are judged on the same scale. A win is a
top-three placement; both the win and the placement are attested. You cannot
vote on your own submission.

## Web3 is experimental, and shown as such

The web3 game trade carries an `experimental` tag everywhere it appears, and two
hard lines a reviewer enforces before anything it touches counts:

1. **The contract is audited by a senior reviewer** before any on-chain claim is
   validated.
2. **Nothing bought on-chain changes the outcome of play.** Cosmetics and
   access, never power — no pay-to-win. A design that puts advantage behind a
   purchase does not belong, experimental or not.

The chain has to do something the game genuinely needs, or it is decoration with
a gas fee.

## Free-to-play design respects the player

Any slice with money in it is read against one more question on the design
grid: no dark patterns. A progression built to frustrate you into paying is a
design failure here, not a monetisation success.

## Age ratings are yours to declare

A game says who it is for. Where an age rating applies — ESRB, PEGI, or the
equivalent for your audience — declaring it is the creator's responsibility, and
the declaration is taken at its word. Skilluv does not rate games and does not
verify a rating; it records what you declared. Declaring nothing on a game that
plainly needs a rating, or declaring one it does not deserve, is on you, and a
reviewer who spots the mismatch will say so.

## Where a game is published

The domain runs on work that reached players, and the platforms it points at are
the open ones: **itch.io** and **GameJolt** are encouraged — they are
OSS-friendly, they take a build without a fee, and a reviewer can play a slice
there. Publishing to **Steam or a console** is out of Skilluv's scope: the
store fees and the certification process are the creator's to carry, and a
title that reached players on itch proves exactly what one on Steam would. A
shipped-title attestation vouches for players being able to reach the game, not
for which storefront it sits in.

## Who enforces this

Reviewers hold `game_reviewer:{family}` or `game_reviewer:all`. Validation,
mod confirmation and shipped-title confirmation are theirs. Finalising a jam and
featuring a creator are editorial acts reserved to administrators. The person
who judges the work is not the person who runs the platform — the same split
security draws, and for the same reason.
