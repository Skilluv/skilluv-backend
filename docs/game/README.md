# Game on Skilluv

Everything this platform offers somebody who makes games, in one page. Written
for a person deciding whether to spend an evening here.

## What this is

Twenty-one trades, one rank, and proof a stranger can check.

Skilluv is a compagnonnage platform: you do real work, somebody who ships games
reads it, and what comes out is an attestation with a verification code rather
than a certificate with your name in a serif font. The game domain works like
the other ten — same rank, same badges, same craft score — with one thing no
other domain has: **a game slice is not validated until real players have
touched it.** Three playtests, an average fun score of three, and a reviewer's
sign-off. "It runs and I like it" is not enough here.

## The five families, the twenty-one trades

Every trade belongs to a review family. The family decides which grid your work
is read against and which reviewer can sign it off.

| Family | Trades |
|---|---|
| **Programming** | gameplay, engine, graphics, network, AI, tools, backend |
| **Design** | systems, level, narrative, economy, combat |
| **Art & animation** | 3D modeler (hard-surface / organic), 2D animator, 3D animator, VFX artist, technical artist |
| **Modding & mapmaking** | modder, mapmaker |
| **Web3 (experimental)** | web3 game developer |

Pick up to three at `PUT /api/users/me/orientations`. Each family has its own
onboarding — `GET /api/domains/game/guides`. The web3 trade is shown as
experimental and carries two hard lines (see the charter).

## Where to practise

The engines are open, the games are open, and a contribution is a merged pull
request in something people play.

| Ground | What for |
|---|---|
| **Godot, Bevy, LÖVE** | Open engines a whole game fits in, no licence fee |
| **0 A.D., Battle for Wesnoth, OpenTTD, Endless Sky** | Open-source games where a merge is real |
| **Nexus, CurseForge, Fortnite Creative** | Modding and mapmaking scenes, published and rated |
| **GDC, Game Maker's Toolkit, Brackeys** | The talks and channels that teach the craft |

The full catalogue is at `GET /api/external-resources?domain=game` (migration
0588).

## The objects the domain adds

Most domains have slices and deliverables. Game adds five first-class objects,
each with its own table and its own attestation basis:

| Object | What it is | Attestation |
|---|---|---|
| **Playtest** | One player's verdict on a slice — fun, clarity, would-return | — (gates validation) |
| **Jam** | A themed weekend, community-voted across axes | `game_jam_winner`, `game_jam_participant` |
| **Mod** | Content in someone else's game, hosted elsewhere, confirmed | `game_mod_published` |
| **Shipped title** | A game that reached players, confirmed by a reviewer | `game_shipped_title` |
| **Featured creator** | Editorial recognition, one per week | `featured_game_creator` |

A validated slice earns `game_artifact_validated`; a merged upstream PR earns
`game_open_source_contribution`; twenty playtests given earns
`game_playtest_hero`.

## How a slice becomes proof

1. Make a game slice — a build, an asset, an animation, a level, a design
   document, a mod package. Set its playable URL.
2. Open a playtest recruitment (`POST /api/game/slices/{id}/playtests/recruit`).
   The floor is three testers.
3. Testers submit verdicts (`POST /api/game/slices/{id}/playtests`). You cannot
   playtest your own slice.
4. Once three verdicts land with an average fun of three, a reviewer validates
   the slice (`POST /api/admin/game/slices/{id}/validate`). That creates the
   verified deliverable, credits the fragments, propagates the skills, and
   issues `game_artifact_validated`.
5. The proof engine recomputes: badges, the game craft score, the cross-domain
   rank — in the same pass.

## The craft score

Fourteen terms, the same ceiling and tiers as every domain (migration 0576).
Apprentice → Prototyper → Shipper → Craftsman → Legend. What it counts:
attestations, jam wins and placements, shipped titles, mods published and mods
past a thousand downloads, upstream contributions, missions, the average of the
review grids received, playtests given, portfolio projects, published
write-ups, years active, and featurings. Every term counts something a stranger
can check; a revoked artefact scores nothing.

Read yours at `GET /api/game/profile`.

## The badges

Twenty (migration 0577). Eleven read proof types every domain shares; nine are
the game engine's own — the five family-expert badges (`game_family_reviews`),
solo and team ship, the full multi-craft game (`game_multi_artefact_ship`), and
organising a jam (`game_jam_organized`).

## For reviewers

You hold `game_reviewer:{family}` — or `game_reviewer:all`. You read work
against your family's grid, you sign off a validation once its playtests are in,
and for the community family you confirm mods against three things: the URL is
real, the mod is theirs, the vendor's terms were kept. See
[CHARTER.md](CHARTER.md) for the lines that end the relationship if crossed.
