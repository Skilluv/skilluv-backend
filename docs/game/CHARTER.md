# The game domain charter

The rules a game creator, a playtester and a reviewer all agree to. Short,
because the lines that matter are few and hard.

## The work is yours

You keep the rights to what you make. Skilluv records the proof — a link, a
build, a validated slice — never a claim of ownership over your game. An
attestation says "this person made this and it was reviewed", nothing more.

## Third-party assets are credited, always

A game is built on other people's work: an engine, a font, a sound pack, a
shader from a tutorial. Every asset that is not yours is credited, and its
licence is respected. A slice that ships someone else's asset as its own is not
a validation problem — it is the end of the relationship. This is the one rule
with no second chance, because passing off is not a mistake of craft.

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

## Who enforces this

Reviewers hold `game_reviewer:{family}` or `game_reviewer:all`. Validation,
mod confirmation and shipped-title confirmation are theirs. Finalising a jam and
featuring a creator are editorial acts reserved to administrators. The person
who judges the work is not the person who runs the platform — the same split
security draws, and for the same reason.
