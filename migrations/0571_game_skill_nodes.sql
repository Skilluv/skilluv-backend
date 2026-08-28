-- The game domain's skill forest.
--
-- Family roots and their children, the vocabulary the skill map (0572) and the
-- profile draw on. Idempotent by slug, so the migration is a no-op on a second
-- run. `display_category` places each node on the profile's six-axis wheel.

-- Roots first (a child references its parent, so order matters).

INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-gameplay-systems', 'Gameplay systems', 'The mechanics a player touches, made to feel right.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-engine-architecture', 'Engine architecture', 'The layer under the game: ECS, loops, plugins, the pieces others build on.', 'game', 'understand')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-rendering', 'Real-time rendering', 'Shaders and the pipeline that runs them within a frame budget.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-networking', 'Game networking', 'Two machines agreeing on one game state.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-ai-behaviour', 'Game AI', 'Behaviour a player reads as intent.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-tooling', 'Game tooling', 'Editor extensions and pipelines the team runs on.', 'game', 'operate')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-live-services', 'Game backend services', 'Leaderboards, saves, achievements, live-ops.', 'game', 'operate')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-systems-design', 'Systems design', 'How the parts multiply, made playable.', 'game', 'create')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-level-design', 'Level design', 'Space that teaches without a tutorial.', 'game', 'create')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-narrative-design', 'Narrative design', 'Story that is played, not read.', 'game', 'create')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-economy-design', 'Economy design', 'Currencies and curves that respect the player.', 'game', 'create')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-combat-design', 'Combat design', 'The half-second that makes a hit land.', 'game', 'create')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-3d-hardsurface', 'Hard-surface modeling', 'Weapons, vehicles, props — game-ready and in budget.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-3d-organic', 'Organic modeling', 'Characters and creatures a rigger can pick up.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-2d-animation', '2D animation', 'Sprites that move with weight.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-3d-animation', '3D animation', 'Locomotion, combat, reactions, cutscenes.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-vfx', 'Real-time VFX', 'Impacts and ambience inside a particle budget.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-technical-art', 'Technical art', 'The bridge between art and engine.', 'game', 'operate')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-modding', 'Modding', 'Changing a game within its rules and its terms.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-mapmaking', 'Mapmaking', 'Building inside someone else''s canvas.', 'game', 'craft')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-onchain', 'On-chain game systems', 'Provable play and interop, never pay-to-win.', 'game', 'create')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-playtesting', 'Playtesting', 'Reading real players, and iterating on what they show.', 'game', 'share')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-engine-godot', 'Godot', 'The open engine the domain leans on first.', 'game', 'operate')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-engine-bevy', 'Bevy', 'Rust-native, ECS-first, WASM-friendly.', 'game', 'operate')
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category)
VALUES ('game-web-builds', 'Web (WASM) builds', 'A build a reviewer plays in the browser.', 'game', 'operate')
ON CONFLICT (slug) DO NOTHING;

-- Children.
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-character-controller', 'Character controller', 'Movement that reads as intent: ground feel, air control, coyote time.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-gameplay-systems'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-camera-systems', 'Camera systems', 'Framing the action without fighting the player.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-gameplay-systems'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-save-load', 'Save and load', 'State that survives a crash and a version bump.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-gameplay-systems'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-inventory-systems', 'Inventory and items', 'Grids, stacks, and the rules that make them legible.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-gameplay-systems'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-ecs', 'Entity-component-system', 'Data-oriented composition over inheritance.', 'game', 'understand', p.id
  FROM skill_nodes p WHERE p.slug = 'game-engine-architecture'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-engine-extension', 'Engine extension', 'Godot modules, Bevy crates, native bindings.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-engine-architecture'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-shaders', 'Shaders', 'Vertex and fragment work: lighting, water, toon, post.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-rendering'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-gpu-particles', 'GPU particles', 'Compute-driven effects at a hundred thousand a frame.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-rendering'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-frame-budget', 'Frame budget', 'Knowing what a technique costs before shipping it.', 'game', 'operate', p.id
  FROM skill_nodes p WHERE p.slug = 'game-rendering'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-authoritative-sync', 'Authoritative sync', 'Server truth, lag compensation, reconciliation.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-networking'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-rollback', 'Rollback netcode', 'Deterministic prediction for fighting-game latency.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-networking'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-matchmaking', 'Matchmaking', 'Pairing players by rank without long queues.', 'game', 'operate', p.id
  FROM skill_nodes p WHERE p.slug = 'game-networking'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-behaviour-trees', 'Behaviour trees and utility AI', 'States, blackboards, utility scoring.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-ai-behaviour'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-pathfinding', 'Pathfinding and crowds', 'Navigation meshes, steering, group movement.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-ai-behaviour'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-procgen', 'Procedural generation', 'Content from rules a designer can steer.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-ai-behaviour'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-editor-plugins', 'Editor plugins', 'In-editor tools that give hours back.', 'game', 'operate', p.id
  FROM skill_nodes p WHERE p.slug = 'game-tooling'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-asset-pipeline', 'Asset pipeline', 'Import, atlas, compress — automatically.', 'game', 'operate', p.id
  FROM skill_nodes p WHERE p.slug = 'game-tooling'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-progression-design', 'Progression design', 'Curves, unlocks, the shape of getting better.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-systems-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-balance', 'Balance', 'Numbers tuned against real play, not a hunch.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-systems-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-gdd', 'Game design documents', 'Writing the intent down so a team can build it.', 'game', 'share', p.id
  FROM skill_nodes p WHERE p.slug = 'game-systems-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-encounter-design', 'Encounter and pacing', 'Where the pressure rises and eases.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-level-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-open-world-design', 'Open-world layout', 'Points of interest, sightlines, secrets.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-level-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-branching', 'Branching and choice', 'Consequence that a player can feel.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-narrative-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-worldbuilding', 'Worldbuilding', 'A coherent place with history and factions.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-narrative-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-environmental-story', 'Environmental storytelling', 'Narration through space, without text.', 'game', 'create', p.id
  FROM skill_nodes p WHERE p.slug = 'game-narrative-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-monetisation-ethics', 'Ethical monetisation', 'Revenue without dark patterns.', 'game', 'understand', p.id
  FROM skill_nodes p WHERE p.slug = 'game-economy-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-combat-feel', 'Combat feel', 'Recovery, stagger, i-frames, hit-stop.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-combat-design'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-topology-uv', 'Topology and UVs', 'Clean meshes that deform and texture.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-3d-hardsurface'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-pbr-texturing', 'PBR texturing', 'Materials that read right under the engine''s light.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-3d-hardsurface'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-lods', 'LODs and budgets', 'Polycount and texture size within the limit.', 'game', 'operate', p.id
  FROM skill_nodes p WHERE p.slug = 'game-3d-hardsurface'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-spine-rigging', '2D rigging', 'Spine and Rive rigs over frame-by-frame.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-2d-animation'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-anim-principles', 'Timing and weight', 'The principles a reviewer judges first.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-3d-animation'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-facial-anim', 'Facial animation', 'Blendshapes and phoneme mapping.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-3d-animation'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-rigging', 'Rigging', 'Skeletons and templates ready to animate.', 'game', 'craft', p.id
  FROM skill_nodes p WHERE p.slug = 'game-technical-art'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-vendor-tos', 'Vendor terms of service', 'Respecting the platform, re-hosting nothing.', 'game', 'understand', p.id
  FROM skill_nodes p WHERE p.slug = 'game-modding'
ON CONFLICT (slug) DO NOTHING;
INSERT INTO skill_nodes (slug, display_name, description, domain, display_category, parent_id)
SELECT 'game-contract-audit', 'Contract safety', 'Audited before anything it touches counts.', 'game', 'understand', p.id
  FROM skill_nodes p WHERE p.slug = 'game-onchain'
ON CONFLICT (slug) DO NOTHING;

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM skill_nodes WHERE domain = 'game'
      AND slug LIKE 'game-%';
    IF n < 61 THEN
        RAISE EXCEPTION 'expected at least 61 game skill nodes, found %', n;
    END IF;
END $$;

