-- The game domain's seed challenges: 97 drafts across the twenty-one trades.
--
-- Same construction as the code seeds in 0185. Each row is a draft
-- `challenge_templates` row in the game domain; the orientation slug drives two
-- things by join — the difficulty (from the review family) and the evaluation
-- rubric (copied from that family's review grid) — but is not stored, because a
-- challenge belongs to a domain and a family, not to a single trade.
--
-- Drafts, `is_training` TRUE. A person turns a draft into a live challenge; the
-- seed is a starting point, not a published challenge, the same caution every
-- domain's seeds carry.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, evaluation_rubric)
SELECT
    c.title, c.description, c.instructions, 'game', d.difficulty, NULL,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'game' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'game' AND g.reviewer_group IS NULL)
    )
FROM (VALUES
    ('game-gameplay-programmer', 'Third-person character controller', 'A 3D controller with camera, jump and dash, and collisions that feel right.', '## What to build

A 3D controller with camera, jump and dash, and collisions that feel right.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-gameplay-programmer', 'RPG inventory system', 'A grid inventory with drag-and-drop, stacking, tooltips and persistence.', '## What to build

A grid inventory with drag-and-drop, stacking, tooltips and persistence.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-gameplay-programmer', 'Branching dialogue system', 'A dialogue tree with choices, variables and save/load.', '## What to build

A dialogue tree with choices, variables and save/load.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-gameplay-programmer', 'Enemy behaviour tree', 'An enemy with idle, patrol and attack states and line-of-sight.', '## What to build

An enemy with idle, patrol and attack states and line-of-sight.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-gameplay-programmer', 'Multi-slot save system', 'Save games across three slots, with versioning and migration.', '## What to build

Save games across three slots, with versioning and migration.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-gameplay-programmer', 'Physics puzzle mechanic', 'A physics puzzle mechanic — portals, gravity or magnets.', '## What to build

A physics puzzle mechanic — portals, gravity or magnets.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-engine-programmer', 'Reusable Bevy plugin', 'A Bevy plugin published as a crate with documentation.', '## What to build

A Bevy plugin published as a crate with documentation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-engine-programmer', 'Godot native extension', 'A native module extending Godot, in Rust or C++.', '## What to build

A native module extending Godot, in Rust or C++.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-engine-programmer', 'ECS from scratch', 'An entity-component-system implemented from first principles.', '## What to build

An entity-component-system implemented from first principles.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-engine-programmer', 'Memory profiling tool', 'A profiling tool for the target engine.', '## What to build

A profiling tool for the target engine.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-graphics-programmer', 'Stylised toon shader', 'A cel-shading shader with rim light and outline.', '## What to build

A cel-shading shader with rim light and outline.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-graphics-programmer', 'Realistic water shader', 'A water shader with waves and refraction.', '## What to build

A water shader with waves and refraction.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-graphics-programmer', 'Post-processing pipeline', 'A custom pipeline: bloom, tone mapping, film grain.', '## What to build

A custom pipeline: bloom, tone mapping, film grain.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-graphics-programmer', 'GPU particle system', 'Compute-shader particles at a hundred thousand and up.', '## What to build

Compute-shader particles at a hundred thousand and up.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-graphics-programmer', 'Custom lighting model', 'An original, non-PBR lighting model.', '## What to build

An original, non-PBR lighting model.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-network-programmer', 'Two-player peer-to-peer', 'Simple P2P networking for two players.', '## What to build

Simple P2P networking for two players.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-network-programmer', 'Authoritative server sync', 'Server-authoritative movement with lag compensation.', '## What to build

Server-authoritative movement with lag compensation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-network-programmer', 'Rollback netcode', 'GGPO-style rollback for a fighting game.', '## What to build

GGPO-style rollback for a fighting game.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-network-programmer', 'Rank-based matchmaking', 'A basic matchmaking service keyed on rank.', '## What to build

A basic matchmaking service keyed on rank.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-ai-programmer', 'NPC crowd simulation', 'A hundred-plus NPCs with pathfinding, collision and group behaviour.', '## What to build

A hundred-plus NPCs with pathfinding, collision and group behaviour.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-ai-programmer', 'Utility AI system', 'Utility-based decision making rather than a behaviour tree.', '## What to build

Utility-based decision making rather than a behaviour tree.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-ai-programmer', 'Procedural dungeon generator', 'Procedural generation steered by design rules.', '## What to build

Procedural generation steered by design rules.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-ai-programmer', 'LLM-driven NPC', 'An NPC whose dialogue is driven by an LLM, with a persona.', '## What to build

An NPC whose dialogue is driven by an LLM, with a persona.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-tools-programmer', 'Godot editor plugin', 'A useful editor plugin — asset organiser, quick actions.', '## What to build

A useful editor plugin — asset organiser, quick actions.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-tools-programmer', 'Custom level editor', 'An in-game level editor for a project.', '## What to build

An in-game level editor for a project.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-tools-programmer', 'Asset pipeline automation', 'A script automating asset import.', '## What to build

A script automating asset import.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-tools-programmer', 'Localisation tool', 'A dedicated i18n tool: string extraction and a translation UI.', '## What to build

A dedicated i18n tool: string extraction and a translation UI.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-backend-programmer', 'Real-time leaderboard service', 'A leaderboard API: top hundred plus friends.', '## What to build

A leaderboard API: top hundred plus friends.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-backend-programmer', 'Achievement system backend', 'Unlockable, persistent achievements with push.', '## What to build

Unlockable, persistent achievements with push.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-backend-programmer', 'Cloud saves service', 'Cross-device save sync with conflict resolution.', '## What to build

Cross-device save sync with conflict resolution.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-backend-programmer', 'Live-ops event scheduler', 'Schedulable in-game events — double-XP weekends and the like.', '## What to build

Schedulable in-game events — double-XP weekends and the like.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-systems-designer', 'Full crafting system', 'A crafting GDD — ingredients, tiers, discovery — and a prototype.', '## What to build

A crafting GDD — ingredients, tiers, discovery — and a prototype.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-systems-designer', 'Combat balance across classes', 'A three-class combat system, balanced, with a testing report.', '## What to build

A three-class combat system, balanced, with a testing report.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-systems-designer', 'Ethical free-to-play progression', 'Progression without pay-to-win and monetisation that respects players.', '## What to build

Progression without pay-to-win and monetisation that respects players.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-systems-designer', 'Roguelike run structure', 'A twenty-minute run structure with variety.', '## What to build

A twenty-minute run structure with variety.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-systems-designer', 'Three multiplayer modes', 'Deathmatch, capture-the-flag and king-of-the-hill, coherent.', '## What to build

Deathmatch, capture-the-flag and king-of-the-hill, coherent.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-systems-designer', 'Balanced card game', 'Sixty cards, deck-building rules, and a playtest balance pass.', '## What to build

Sixty cards, deck-building rules, and a playtest balance pass.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-level-designer', 'Three-act platformer level', 'A platformer level in three sections with a difficulty curve.', '## What to build

A platformer level in three sections with a difficulty curve.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-level-designer', 'Metroidvania map', 'A small metroidvania map — rooms, interconnections, backtracking.', '## What to build

A small metroidvania map — rooms, interconnections, backtracking.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-level-designer', 'Multiplayer FPS arena', 'A balanced arena for five to eight players.', '## What to build

A balanced arena for five to eight players.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-level-designer', 'Puzzle level series', 'Ten puzzles with a progression.', '## What to build

Ten puzzles with a progression.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-level-designer', 'Open-world region', 'A five-hundred-metre-square region — points of interest, biomes, secrets.', '## What to build

A five-hundred-metre-square region — points of interest, biomes, secrets.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-narrative-designer', 'Three-act character arc', 'A protagonist''s full arc: goal, obstacle, transformation.', '## What to build

A protagonist''s full arc: goal, obstacle, transformation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-narrative-designer', 'Compact worldbuilding', 'A coherent universe — history, factions, geography — in twenty pages.', '## What to build

A coherent universe — history, factions, geography — in twenty pages.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-narrative-designer', 'Branching quest', 'A quest with three meaningful branches and endings.', '## What to build

A quest with three meaningful branches and endings.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-narrative-designer', 'Environmental storytelling', 'Narration through space, without text.', '## What to build

Narration through space, without text.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-narrative-designer', 'Emergent narrative system', 'A system that generates stories, Dwarf-Fortress-like.', '## What to build

A system that generates stories, Dwarf-Fortress-like.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-economy-designer', 'Ethical free-to-play economy', 'A balanced mobile F2P economy with no dark patterns.', '## What to build

A balanced mobile F2P economy with no dark patterns.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-economy-designer', 'Dual-currency system', 'Soft and hard currency with exchange and earn-and-spend flow.', '## What to build

Soft and hard currency with exchange and earn-and-spend flow.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-economy-designer', 'MMORPG progression curve', 'An XP curve from level one to sixty, with rewards.', '## What to build

An XP curve from level one to sixty, with rewards.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-economy-designer', 'Battle-pass season', 'A ninety-day battle pass with FOMO mitigated.', '## What to build

A ninety-day battle pass with FOMO mitigated.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-combat-designer', 'Weight-heavy melee combat', 'Souls-like feel: attack recovery, stagger, i-frames.', '## What to build

Souls-like feel: attack recovery, stagger, i-frames.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-combat-designer', 'Eight-weapon arsenal', 'Eight distinct weapons with roles and balance.', '## What to build

Eight distinct weapons with roles and balance.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-combat-designer', 'Deep turn-based combat', 'Five-plus interacting mechanics — elements, buffs, positioning.', '## What to build

Five-plus interacting mechanics — elements, buffs, positioning.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-combat-designer', 'Three-phase boss', 'A boss across three phases with tells and escalation.', '## What to build

A boss across three phases with tells and escalation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-hardsurface', 'Sci-fi weapon pack', 'Three coherent weapons, low-poly and game-ready, with PBR textures.', '## What to build

Three coherent weapons, low-poly and game-ready, with PBR textures.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-hardsurface', 'Drivable vehicle', 'A modelled, textured vehicle with rig-ready wheels.', '## What to build

A modelled, textured vehicle with rig-ready wheels.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-hardsurface', 'Environment prop set', 'Ten coherent low-poly props.', '## What to build

Ten coherent low-poly props.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-hardsurface', 'Modular building kit', 'A modular kit — walls, floors, roofs — that assembles.', '## What to build

A modular kit — walls, floors, roofs — that assembles.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-hardsurface', 'Weapon customisation art', 'A weapon with eight swappable attachments.', '## What to build

A weapon with eight swappable attachments.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-organic', 'Rig-ready hero character', 'A low-poly hero, game-ready, with UVs, textures and a rig.', '## What to build

A low-poly hero, game-ready, with UVs, textures and a rig.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-organic', 'Stylised creature', 'A stylised fantasy creature with textures and subtle animation.', '## What to build

A stylised fantasy creature with textures and subtle animation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-organic', 'Character customisation set', 'A character with four heads and four body types, swappable.', '## What to build

A character with four heads and four body types, swappable.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-organic', 'Character LOD pack', 'A character with three LODs and a texture atlas.', '## What to build

A character with three LODs and a texture atlas.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-3d-modeler-organic', 'Rig-ready biped animal', 'A biped animal with a rig ready for animation.', '## What to build

A biped animal with a rig ready for animation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-2d', 'Idle, run and attack', 'A 2D character with three coherent cycles.', '## What to build

A 2D character with three coherent cycles.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-2d', 'Full sprite atlas', 'A character with ten animations.', '## What to build

A character with ten animations.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-2d', 'Spine hero rig', 'A Spine-rigged character with six-plus animations.', '## What to build

A Spine-rigged character with six-plus animations.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-2d', 'UI micro-animation pack', 'Ten UI micro-animations — buttons, notifications, transitions.', '## What to build

Ten UI micro-animations — buttons, notifications, transitions.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-2d', 'Pixel-art VFX pack', 'Five frame-by-frame pixel VFX — explosions, magic, sparkle.', '## What to build

Five frame-by-frame pixel VFX — explosions, magic, sparkle.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-3d', 'Locomotion pack', 'Walk, run and sprint cycles with a blend space.', '## What to build

Walk, run and sprint cycles with a blend space.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-3d', 'Melee combat set', 'Five melee attacks with cancels and follow-through.', '## What to build

Five melee attacks with cancels and follow-through.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-3d', 'Character reactions pack', 'Eight reactions — hit, death, stun, celebrate and more.', '## What to build

Eight reactions — hit, death, stun, celebrate and more.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-3d', 'Cinematic cutscene', 'A fifteen-second stylised cutscene with camera and narrative timing.', '## What to build

A fifteen-second stylised cutscene with camera and narrative timing.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-animator-3d', 'Facial blendshapes', 'A facial rig with blendshapes and phoneme mapping.', '## What to build

A facial rig with blendshapes and phoneme mapping.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-vfx-artist', 'Combat impact VFX', 'A coherent hit, slash, block and crit pack.', '## What to build

A coherent hit, slash, block and crit pack.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-vfx-artist', 'Ambient environmental VFX', 'Wind, smoke, dust and volumetric fog.', '## What to build

Wind, smoke, dust and volumetric fog.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-vfx-artist', 'Elemental spell VFX', 'Four coherent stylised spells — fire, ice, lightning, earth.', '## What to build

Four coherent stylised spells — fire, ice, lightning, earth.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-vfx-artist', 'Weapon trail VFX', 'Five distinct trails — sword, axe, bow, gun, staff.', '## What to build

Five distinct trails — sword, axe, bow, gun, staff.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-vfx-artist', 'On-damage screen effects', 'A damage vignette, chromatic aberration and shake, orchestrated.', '## What to build

A damage vignette, chromatic aberration and shake, orchestrated.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-technical-artist', 'Shader-graph pipeline', 'A custom shader-graph pipeline with documentation for the art team.', '## What to build

A custom shader-graph pipeline with documentation for the art team.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-technical-artist', 'LOD auto-generation tool', 'A tool generating LODs for props and characters.', '## What to build

A tool generating LODs for props and characters.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-technical-artist', 'Texture optimisation pipeline', 'Automatic compression and atlasing.', '## What to build

Automatic compression and atlasing.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-technical-artist', 'Rigging templates library', 'Reusable rig templates — biped, quadruped, insect.', '## What to build

Reusable rig templates — biped, quadruped, insect.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-modder', 'Skyrim quality-of-life mod', 'A QoL mod for Skyrim SE/AE, published on Nexus.', '## What to build

A QoL mod for Skyrim SE/AE, published on Nexus.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-modder', 'Minecraft utility mod', 'A utility mod on Forge or Fabric, published on CurseForge.', '## What to build

A utility mod on Forge or Fabric, published on CurseForge.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-modder', 'Fallout 4 quest mod', 'A quest mod for Fallout 4, radiant or scripted.', '## What to build

A quest mod for Fallout 4, radiant or scripted.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-modder', '7 Days to Die overhaul', 'A small overhaul — new items and mechanics.', '## What to build

A small overhaul — new items and mechanics.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-modder', 'Cities: Skylines asset pack', 'A custom building or prop asset pack.', '## What to build

A custom building or prop asset pack.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-mapmaker', 'Fortnite Creative island', 'A Creative island with a game mode, published, with a code.', '## What to build

A Creative island with a game mode, published, with a code.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-mapmaker', 'Roblox experience', 'A complete Roblox experience with ethical monetisation.', '## What to build

A complete Roblox experience with ethical monetisation.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-mapmaker', 'Dreams creation', 'A Dreams narrative or game.', '## What to build

A Dreams narrative or game.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-mapmaker', 'Minecraft adventure map', 'An adventure map with quests and custom mechanics.', '## What to build

An adventure map with quests and custom mechanics.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('game-mapmaker', 'CS2 custom map', 'A balanced competitive custom map with community feedback.', '## What to build

A balanced competitive custom map with community feedback.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('web3-game-dev', 'NFT-gated content demo', 'A demo where content is gated by NFT ownership — visual perks only, no pay-to-win.', '## What to build

A demo where content is gated by NFT ownership — visual perks only, no pay-to-win.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('web3-game-dev', 'On-chain leaderboard', 'A leaderboard synced on-chain: proof of play, tamper-resistant.', '## What to build

A leaderboard synced on-chain: proof of play, tamper-resistant.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.'),
    ('web3-game-dev', 'Cross-game asset interop', 'Two games sharing a transferable cosmetic via ERC-721/1155.', '## What to build

Two games sharing a transferable cosmetic via ERC-721/1155.

## What is expected

The source, a build or preview a reviewer can open, and — for anything playable — three playtests. Third-party assets credited.

## What is looked at

The review grid of the family applies, and it is public: read it before you submit.')
) AS c(orientation_slug, title, description, instructions)
JOIN orientations o ON o.slug = c.orientation_slug
CROSS JOIN LATERAL (
    SELECT CASE o.reviewer_group
        WHEN 'web3'      THEN 4
        WHEN 'programming' THEN 4
        ELSE 3
    END AS difficulty
) AS d;

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM challenge_templates
     WHERE skill_domain = 'game' AND is_training = TRUE AND status = 'draft';
    IF n < 97 THEN
        RAISE EXCEPTION 'expected at least 97 game challenge seeds, found %', n;
    END IF;
END $$;
