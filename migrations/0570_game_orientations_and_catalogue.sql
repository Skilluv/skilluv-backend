-- The game domain opens, with twenty-one trades and five review families.
--
-- ## What was there
--
-- Seven coarse orientations from 0088, most with a null `reviewer_group`:
-- `game-programmer`, `game-designer`, `game-sound-engineer`, `game-artist-2d`,
-- `game-artist-3d`, and the two `design-game-*` that legitimately belong to
-- design. A person who writes netcode, a person who balances an economy and a
-- person who rigs a character all had to pick `game-programmer` or
-- `game-artist-*`, and a reviewer had no family to be granted rights on — the
-- exact state 0542 ended for security and 0176 warned against.
--
-- ## Why twenty-one, and why five families
--
-- The trades are what a studio hires for and what a portfolio is judged as,
-- one row each. The families are what a *reviewer* has to be able to do — the
-- line 0176 drew and 0517 restated — because review rights are granted per
-- family, not per trade. Five families:
--
--   * programming     — code a reviewer reads and runs
--   * design          — a document and a prototype a reviewer plays
--   * art-animation    — an asset a reviewer opens in the DCC tool
--   * community        — a mod or a map on a third-party platform
--   * web3            — a contract a reviewer audits
--
-- The reviewer capability is `game_reviewer:{reviewer_group}`, derived the
-- same way `security_reviewer:{family}` is — a free capability string, not an
-- enum, so nothing here extends a type. A super-reviewer holds
-- `game_reviewer:all`.
--
-- ## The five that are archived
--
-- `game-programmer`, `game-designer`, `game-sound-engineer`, `game-artist-2d`
-- and `game-artist-3d` are archived and pointed at what replaces them, the
-- mechanism 0089 built: the people who chose one keep the row in their
-- history, `replaced_by` says where it went, and nobody chooses it again.
-- `game-sound-engineer` goes to the audio domain's `audio-sound-designer`,
-- where game sound lives now — not to a game trade, because game does not
-- review audio.
--
-- The two `design-game-*` orientations are NOT touched: UI and environment art
-- for games are a design trade, reviewed by design, and 0229 already owns them.
--
-- ## web3-game-dev is experimental
--
-- It carries the `experimental` tag. It is a real trade and it opens, but the
-- tag lets the profile and talent search present it as emerging rather than
-- established, and a contract is audited by a senior reviewer before anything
-- it touches counts.

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated, reviewer_group)
VALUES
('game-gameplay-programmer', 'Gameplay programmer',
 'The feel of the thing: controllers, cameras, inventories, dialogue, save systems, the mechanics a player touches. Judged on a build somebody else can pick up and on how it holds up to three playtests, not on the diff.',
 'game', ARRAY['code'],
 ARRAY['gameplay', 'mechanics', 'controller', 'prototype'],
 TRUE, 'programming'),

('game-engine-programmer', 'Engine programmer',
 'The layer under the game: ECS, rendering loops, plugins, native extensions, the tools a team builds on. Godot modules, Bevy crates, a custom engine. The proof is a piece other people''s games can depend on.',
 'game', ARRAY['code'],
 ARRAY['engine', 'ecs', 'godot', 'bevy', 'systems'],
 TRUE, 'programming'),

('game-graphics-programmer', 'Graphics programmer',
 'Shaders and the pipeline that runs them: lighting models, post-processing, GPU particles, the frame budget. GLSL, HLSL, WGSL, and a demo build that shows the technique and its cost.',
 'game', ARRAY['code'],
 ARRAY['shaders', 'rendering', 'gpu', 'glsl', 'wgsl'],
 TRUE, 'programming'),

('game-network-programmer', 'Network programmer',
 'Two machines agreeing on one game state: authoritative servers, lag compensation, rollback, matchmaking. Judged on a build tested across instances, because netcode that only works on localhost is not netcode.',
 'game', ARRAY['code', 'ops'],
 ARRAY['netcode', 'multiplayer', 'rollback', 'matchmaking'],
 TRUE, 'programming'),

('game-ai-programmer', 'Game AI programmer',
 'Believable behaviour: behaviour trees and utility systems, crowds and pathfinding, procedural generation, LLM-driven NPCs. Judged on what a player reads as intent, on a build that demonstrates it.',
 'game', ARRAY['code', 'ai'],
 ARRAY['behaviour-tree', 'pathfinding', 'procgen', 'utility-ai'],
 TRUE, 'programming'),

('game-tools-programmer', 'Tools programmer',
 'The editor extensions and asset pipelines a team never notices until they break: importers, in-game level editors, localisation tooling. Judged on the hours it gives the rest of the team back.',
 'game', ARRAY['code'],
 ARRAY['editor-plugin', 'pipeline', 'tooling', 'automation'],
 TRUE, 'programming'),

('game-backend-programmer', 'Game backend programmer',
 'The services a live game leans on: leaderboards, achievements, cloud saves, live-ops schedulers. A backend and a client demo that proves it under the shapes a game actually sends.',
 'game', ARRAY['code', 'ops'],
 ARRAY['leaderboards', 'cloud-saves', 'live-ops', 'backend'],
 TRUE, 'programming'),

('game-systems-designer', 'Systems designer',
 'How the parts multiply: crafting, progression, combat balance, run structure, ethical free-to-play. A design document and a prototype that makes the numbers playable, because a spreadsheet is not a system.',
 'game', ARRAY['leadership'],
 ARRAY['systems', 'balance', 'progression', 'gdd'],
 TRUE, 'design'),

('game-level-designer', 'Level designer',
 'Space that teaches without a tutorial: platformer acts, metroidvania maps, arenas, puzzle sequences, an open-world corner. A level pack in engine and a build, measured against a difficulty curve real players walked.',
 'game', ARRAY[]::TEXT[],
 ARRAY['levels', 'encounter', 'pacing', 'layout'],
 TRUE, 'design'),

('game-narrative-designer', 'Narrative designer',
 'Story that is played, not read: character arcs, compact worlds, branching quests, environmental storytelling, emergent systems. A document, dialogue trees, and where possible the branch running in engine.',
 'game', ARRAY['communication'],
 ARRAY['narrative', 'worldbuilding', 'branching', 'dialogue'],
 TRUE, 'design'),

('game-economy-designer', 'Economy designer',
 'Currencies, sinks and faucets, progression curves, battle passes — tuned so the game respects the player rather than the wallet. A simulation, a document, and the earn-and-spend flow that shows no dark pattern hides in it.',
 'game', ARRAY['leadership'],
 ARRAY['economy', 'f2p', 'currency', 'monetisation'],
 TRUE, 'design'),

('game-combat-designer', 'Combat designer',
 'The half-second that makes a hit land: recovery, stagger, i-frames, weapon roles, boss phases. A design document and a prototype whose feel a reviewer can put a controller to.',
 'game', ARRAY[]::TEXT[],
 ARRAY['combat', 'feel', 'balance', 'boss'],
 TRUE, 'design'),

('game-3d-modeler-hardsurface', 'Hard-surface 3D modeler',
 'Weapons, vehicles, props, modular kits — game-ready and within budget. Clean topology, sane UVs, PBR textures, LODs, and a polycount that stays under the limit the brief set.',
 'game', ARRAY['design'],
 ARRAY['3d', 'hard-surface', 'pbr', 'game-ready'],
 TRUE, 'art-animation'),

('game-3d-modeler-organic', 'Organic 3D modeler',
 'Characters and creatures a rigger can pick up: retopo, UVs, textures, customisation variants, LOD chains. Judged on whether it deforms, not only on whether it renders.',
 'game', ARRAY['design'],
 ARRAY['3d', 'character', 'creature', 'retopology'],
 TRUE, 'art-animation'),

('game-animator-2d', '2D animator',
 'Sprites that move with weight: cycles, full atlases, Spine and Rive rigs, UI micro-animation, frame-by-frame VFX. Delivered as sheets or a rig plus a preview a reviewer can loop.',
 'game', ARRAY['design'],
 ARRAY['2d', 'sprite', 'spine', 'frame-by-frame'],
 TRUE, 'art-animation'),

('game-animator-3d', '3D animator',
 'Locomotion, combat sets, reactions, cutscenes, facial rigs. Blend-space ready for Unity or Unreal, delivered with a preview render, judged on timing and weight before anything else.',
 'game', ARRAY['design'],
 ARRAY['3d', 'animation', 'locomotion', 'facial'],
 TRUE, 'art-animation'),

('game-vfx-artist', 'VFX artist',
 'Impacts, ambient weather, elemental spells, weapon trails, on-damage screen effects — inside a particle budget. VFX Graph, Niagara or Godot particles, with a preview and the cost it carries.',
 'game', ARRAY['design'],
 ARRAY['vfx', 'particles', 'niagara', 'stylised'],
 TRUE, 'art-animation'),

('game-technical-artist', 'Technical artist',
 'The bridge between art and engine: shader graphs, LOD generation, texture atlasing, reusable rig templates, the pipeline the art team runs on. Judged on tools and documentation the team adopts.',
 'game', ARRAY['code', 'design'],
 ARRAY['tech-art', 'shader-graph', 'pipeline', 'rigging'],
 TRUE, 'art-animation'),

('game-modder', 'Modder',
 'Changing a game that was not yours to change, within its rules: Skyrim, Minecraft, Fallout, Cities. Published on the platform that game uses, with the vendor''s terms respected and nothing proprietary re-hosted. The proof is a live URL and a download count, never the package on our servers.',
 'game', ARRAY[]::TEXT[],
 ARRAY['modding', 'nexus', 'curseforge', 'community'],
 TRUE, 'community'),

('game-mapmaker', 'Mapmaker',
 'Building inside someone else''s canvas: Fortnite Creative, Roblox, Dreams, Minecraft adventure maps, CS custom maps. Published, played, and judged on the engagement it earned, with the platform''s rules kept.',
 'game', ARRAY[]::TEXT[],
 ARRAY['mapmaking', 'fortnite-creative', 'roblox', 'level'],
 TRUE, 'community'),

('web3-game-dev', 'Web3 game developer',
 'Games where the chain does something the game needs — provable play, tamper-resistant leaderboards, interoperable cosmetics — and never pay-to-win. A contract, a client, and an audit by a senior reviewer before anything it touches counts. Experimental, and shown as such.',
 'game', ARRAY['code'],
 ARRAY['web3', 'smart-contract', 'nft', 'experimental'],
 TRUE, 'web3');

-- ═══════════════════════════════════════════════════════════════════
-- The five that are replaced
-- ═══════════════════════════════════════════════════════════════════

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'game-gameplay-programmer'),
       updated_at = NOW()
 WHERE slug = 'game-programmer';

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'game-systems-designer'),
       updated_at = NOW()
 WHERE slug = 'game-designer';

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'game-animator-2d'),
       updated_at = NOW()
 WHERE slug = 'game-artist-2d';

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'game-3d-modeler-organic'),
       updated_at = NOW()
 WHERE slug = 'game-artist-3d';

-- Sound for a game is the audio domain's trade now, not a game one: game does
-- not review audio. Pointed there rather than at a game orientation.
UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'audio-sound-designer'),
       updated_at = NOW()
 WHERE slug = 'game-sound-engineer';

-- Every curated game orientation now carries a review family. A curated
-- orientation with a null reviewer_group is one nobody can be granted review
-- rights for — visible rather than silently open, as the column's own comment
-- says — and this domain had five of them.
DO $$
DECLARE
    orphans INT;
    added INT;
BEGIN
    SELECT count(*) INTO orphans
      FROM orientations
     WHERE primary_domain = 'game'
       AND is_curated
       AND NOT is_archived
       AND reviewer_group IS NULL;
    IF orphans > 0 THEN
        RAISE EXCEPTION '% live game orientation(s) have no review family', orphans;
    END IF;

    SELECT count(*) INTO added
      FROM orientations
     WHERE primary_domain = 'game' AND is_curated AND NOT is_archived;
    IF added <> 21 THEN
        RAISE EXCEPTION 'expected 21 live curated game orientations, found %', added;
    END IF;
END $$;
