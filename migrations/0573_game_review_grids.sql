-- The game domain's review grids, one per family and one default.
--
-- A grid is what a reviewer fills instead of writing free comments: named
-- criteria, each with a `looks_like` that says what a five looks like, scored
-- at review time. The same shape 0545 gave security and 0180 gave the platform.
-- One grid per `reviewer_group` (the unique index enforces it), so the five art
-- trades share the one `art-animation` grid — the reviewer capability is the
-- same, so the standard is.
--
-- The line every game grid holds that no other domain's does: a playtest is a
-- first-class piece of evidence, not a nicety. A build that runs and a
-- reviewer's taste are not enough on their own — real players had to touch it.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

-- ── The domain default ──────────────────────────────────────────────
-- Read when a game artefact reaches review without a family: a cross-family
-- project, or an orientation added later and not yet assigned one.
('game', NULL, 'Game work — the common floor', '[
  {"criterion": "It runs",
   "looks_like": "A reviewer gets it open — a build that launches, an asset that imports, a document that stands on its own — without a private setup nobody wrote down. A thing that only runs on the author''s machine is not a deliverable yet."},
  {"criterion": "Real players touched it",
   "looks_like": "At least three playtests where the work is playable, and the author changed something because of them. Play is the test this domain runs; a build nobody but the author has played is untested."},
  {"criterion": "The intent is legible",
   "looks_like": "What this is trying to do, and against what constraint — engine, platform, budget, brief. A reviewer should not have to guess the point."},
  {"criterion": "It is game-ready, not just finished",
   "looks_like": "It fits where it has to go: the polycount is under the limit, the netcode survives two instances, the level holds a difficulty curve. Finished in a vacuum is not the same as shippable."},
  {"criterion": "Third-party work is credited",
   "looks_like": "Every asset, sound, font or snippet that is not the author''s is attributed, and its licence permits the use. Kenney, Freesound and OpenGameArt are gifts with names on them."},
  {"criterion": "AI use is declared",
   "looks_like": "If a model wrote code, generated an asset or drafted a design, it is said plainly. Declared assistance is fine; hidden assistance is the only problem."}
]'::jsonb),

-- ── Programming ─────────────────────────────────────────────────────
('game', 'programming', 'Game programming', '[
  {"criterion": "Correctness",
   "looks_like": "It does what it claims across the cases that matter, edge cases included — a controller that survives being spammed, netcode that survives packet loss, a save that survives a version bump."},
  {"criterion": "Performance within the frame",
   "looks_like": "It holds its budget: frame time, allocation, draw calls, bandwidth. A technique whose cost the author cannot state has not been measured."},
  {"criterion": "Reads like the engine it is in",
   "looks_like": "Idiomatic for Godot, Unity, Unreal or Bevy — the engine''s lifecycle, its scene model, its conventions — not a foreign pattern forced through it."},
  {"criterion": "Maintainable",
   "looks_like": "Another programmer can extend it: clear boundaries, named systems, no clever thing that only the author will ever understand."},
  {"criterion": "Tested where it counts",
   "looks_like": "The parts that would break silently — serialisation, math, state machines — have tests. Not coverage theatre, the tests that catch a real regression."},
  {"criterion": "Demonstrated, not described",
   "looks_like": "A build or a demo scene shows the feature working, so a reviewer sees it rather than trusts the README."}
]'::jsonb),

-- ── Design ──────────────────────────────────────────────────────────
('game', 'design', 'Game design', '[
  {"criterion": "It is fun in play, not on paper",
   "looks_like": "The prototype''s playtest scores say so, and the design changed where they said it should. A spreadsheet that balances and a game that bores are a failure, not a success."},
  {"criterion": "The intent is clear",
   "looks_like": "A reader knows what experience this is for and why each part serves it. A document that lists mechanics without saying what they are for is a parts bin."},
  {"criterion": "Balance holds under real play",
   "looks_like": "No single dominant strategy a playtester found in ten minutes; the numbers were tuned against play, not guessed."},
  {"criterion": "It respects the player",
   "looks_like": "Accessible defaults, honest difficulty, and — for anything with money in it — no dark pattern hiding in the economy. F2P that is ethical is a criterion here, not a nice-to-have."},
  {"criterion": "There is a reason to return",
   "looks_like": "Replayability, progression, or variety that a playtester actually reached, not one asserted in a design pillar."},
  {"criterion": "It answers the brief",
   "looks_like": "The constraints the challenge set — genre, length, theme — are met, and where the design departs from them it says why."}
]'::jsonb),

-- ── Art & animation ─────────────────────────────────────────────────
('game', 'art-animation', 'Game art and animation', '[
  {"criterion": "Technical craft",
   "looks_like": "Clean topology and UVs, sane textures, animation that reads as weight and timing before anything else. A rigger, a shader, an engine can pick it up without a fight."},
  {"criterion": "Stylistic coherence",
   "looks_like": "It belongs to one world. A pack that mixes three unrelated styles is a mood board, not a set."},
  {"criterion": "Within budget",
   "looks_like": "Polycount, texture size, particle count, bone count — under the limit the target engine and platform impose, and the author can state them."},
  {"criterion": "Pipeline-ready",
   "looks_like": "LODs where they are needed, a rig where a rig is claimed, an atlas where one is expected, an export that the engine actually accepts. Rig-ready means a rigger tried it, not that the file says so."},
  {"criterion": "Serves the game",
   "looks_like": "For a VFX or an animation: it reads at gameplay distance and speed, and it serves a moment a player has, not a turntable."},
  {"criterion": "It answers the brief",
   "looks_like": "The count, the theme and the constraints the challenge set are met, third-party source is credited, and departures are explained."}
]'::jsonb),

-- ── Community ───────────────────────────────────────────────────────
('game', 'community', 'Modding and mapmaking', '[
  {"criterion": "The platform''s terms were kept",
   "looks_like": "Nothing proprietary re-hosted, nothing ripped, the vendor''s modding rules respected. This is the criterion that ends the relationship if it fails, because it is the one that can end ours with the vendor."},
  {"criterion": "It is really there, and really theirs",
   "looks_like": "A live URL a reviewer opens, and proof it is the author''s — the username matches, or the mod page credits them. A download count is evidence, not decoration."},
  {"criterion": "It installs and holds together",
   "looks_like": "A player can install it from the instructions and it does not corrupt their game. Stability is the modder''s reputation and the platform''s."},
  {"criterion": "Documented",
   "looks_like": "Install steps, dependencies, compatibility, and what it changes — clear enough that a stranger succeeds without the author present."},
  {"criterion": "The community met it",
   "looks_like": "Ratings, comments, endorsements — some signal that people other than the author used it and had something to say."}
]'::jsonb),

-- ── Web3 ────────────────────────────────────────────────────────────
('game', 'web3', 'Web3 game', '[
  {"criterion": "The contract is safe",
   "looks_like": "Audited by a senior reviewer, with the known classes checked — reentrancy, access control, integer bounds, upgrade paths. An unaudited contract that touches value is refused, however clever the game."},
  {"criterion": "The chain earns its place",
   "looks_like": "It does something the game needs — provable play, a tamper-resistant leaderboard, an interoperable cosmetic — rather than wrapping an ordinary game in a token for its own sake."},
  {"criterion": "Never pay-to-win",
   "looks_like": "Nothing bought on-chain changes the outcome of play. Cosmetics and access, yes; power, no. This is a hard line, not a spectrum."},
  {"criterion": "Playable without a wallet",
   "looks_like": "A person who has never held a token can play and understand it. The crypto is a layer, not a toll gate at the front door."},
  {"criterion": "Tokenomics are honest",
   "looks_like": "If there is an economy, it is not a pyramid; the incentives do not depend on the next buyer. Said plainly, and defensible."}
]'::jsonb);

-- Every game family with a reviewer capability has a grid, plus the default.
DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM review_grids WHERE domain = 'game';
    IF n <> 6 THEN
        RAISE EXCEPTION 'expected 6 game review grids (5 families + default), found %', n;
    END IF;
END $$;
