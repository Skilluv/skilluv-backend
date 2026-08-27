-- The game domain's guides and templates.
--
-- Rows in content_guides (0199): a domain welcome and one onboarding guide
-- per review family, a toolkit, seven brief templates keyed to the families
-- that use them, and two writeup templates. English, the repository's content
-- language. An editor loads a brief template into a challenge; a creator reads
-- the onboarding for their family before their first slice.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES
('game-welcome', 'onboarding', 'game', NULL, 'en',
 'Welcome, game creator', 'Where to start, whatever you make.',
 'Skilluv Game is compagnonnage for making games: real work on real projects, reviewed by people who ship, proven by attestations you can show.

Pick one of the twenty-one trades — programming, design, art and animation, modding and mapmaking, or web3 — start a seed challenge, and get three playtests on it. Playtesting is not a formality here: a game slice is not validated until real players have touched it. From there: a game jam, a shipped title, a paid mission, mentoring someone behind you.

You keep the rights to your work. Third-party assets are credited. The engine you use is yours to choose — Godot, Bevy, Unity, Unreal, whatever gets it playable.', 10),

('game-onboarding-programming', 'onboarding', 'game', 'programming', 'en',
 'Onboarding: game programming', 'For gameplay, engine, graphics, network, AI, tools and backend programmers.',
 'The proof of programming here is a build somebody else can pick up, not a diff. Start with a seed — a character controller, an inventory, a behaviour tree — get it running, and get it playtested. Your reviewer reads it against the programming grid: correctness, frame budget, whether it reads like the engine it is in, and whether the feature is demonstrated rather than described.', 20),

('game-onboarding-design', 'onboarding', 'game', 'design', 'en',
 'Onboarding: game design', 'For systems, level, narrative, economy and combat designers.',
 'Design is judged in play, not on paper. A document is welcome; a document with a prototype that makes it playable is the deliverable. The grid asks whether it is fun in the playtests, whether the intent is clear, whether it balances under real play, and whether it respects the player — including, for anything with money in it, no dark patterns.', 30),

('game-onboarding-art-animation', 'onboarding', 'game', 'art-animation', 'en',
 'Onboarding: art and animation', 'For 3D modelers, 2D and 3D animators, VFX and technical artists.',
 'Game art is judged as game-ready, not as a render. Clean topology and UVs, textures that read under the engine''s light, animation that shows weight and timing, everything inside the polycount and particle budget the brief sets. A rig is rig-ready when a rigger has tried it, not when the file says so.', 40),

('game-onboarding-community', 'onboarding', 'game', 'community', 'en',
 'Onboarding: modding and mapmaking', 'For modders and mapmakers.',
 'You work inside someone else''s game, within its rules. Publish on the platform that game uses — Nexus, CurseForge, Fortnite Creative, Roblox — and register the live URL here; Skilluv never hosts the package. The first thing a reviewer checks is that the vendor''s terms were kept and nothing proprietary was re-hosted. That line, broken, ends the relationship.', 50),

('game-onboarding-web3', 'onboarding', 'game', 'web3', 'en',
 'Onboarding: web3 game development', 'For the experimental web3 trade.',
 'This trade is experimental and shown as such. Two hard lines: the contract is audited by a senior reviewer before anything it touches counts, and nothing bought on-chain changes the outcome of play — cosmetics and access, never power. The chain has to do something the game needs, or it does not belong.', 60),

('game-toolkit', 'toolkit', 'game', NULL, 'en',
 'Game toolkit', 'What to install, by trade.',
 'Engines: Godot (open, the domain''s first choice) and Bevy (Rust, ECS, WASM-friendly) need nothing but a download; Unity and Unreal are free to start. Art: Blender for 3D, Aseprite or Krita for 2D, Spine or Rive for 2D rigs. VFX: Unity VFX Graph, Unreal Niagara, or Godot particles. Audio crosses into the audio domain. Modding: the toolchain the target game uses (Creation Kit, Forge/Fabric, the platform''s own editor). For web builds, target WebGL so a reviewer plays in the browser.', 70),

('game-brief-gameplay', 'brief_template', 'game', 'programming', 'en',
 'Brief: gameplay feature', 'A structured brief for a gameplay or systems programming challenge.',
 '## Context
The game and the moment this mechanic serves.

## Constraints
Engine, target platforms, frame budget.

## Deliverables
Source, a playable build, a short preview.

## Playtest
What to watch for, minimum three testers.

## Third-party assets
Anything not yours, credited.', 80),

('game-brief-engine-graphics', 'brief_template', 'game', 'programming', 'en',
 'Brief: engine or graphics feature', 'A structured brief for an engine or graphics challenge.',
 '## Context
The technical feature and where it sits.

## Targets
Performance targets, platforms, engine version.

## Deliverables
Source, a demo scene or build, the measured cost.

## What is looked at
The programming grid — correctness and frame budget first.', 90),

('game-brief-level', 'brief_template', 'game', 'design', 'en',
 'Brief: level', 'A structured brief for a level-design challenge.',
 '## Context
Genre and the experience the level is for.

## Encounter design
The difficulty curve, the beats.

## Deliverables
The level in engine, a build, a short design note.

## Playtest metrics
Completion, where players got stuck, minimum three testers.', 100),

('game-brief-narrative', 'brief_template', 'game', 'design', 'en',
 'Brief: narrative', 'A structured brief for a narrative challenge.',
 '## Context
The world and the character.

## Structure
Arc, dialogue shape, branching and endings.

## Deliverables
The document, the dialogue trees, and where possible the branch in engine.', 110),

('game-brief-3d-asset', 'brief_template', 'game', 'art-animation', 'en',
 'Brief: 3D asset', 'A structured brief for a 3D asset challenge.',
 '## Context
The asset and the world it belongs to.

## Budget
Polycount limit, texture size, rig-ready or not.

## Deliverables
The source file, textures, LODs, a render, and the technical specs.', 120),

('game-brief-vfx', 'brief_template', 'game', 'art-animation', 'en',
 'Brief: VFX', 'A structured brief for a VFX challenge.',
 '## Context
The visual style and the gameplay moment.

## Budget
Particle count, the engine''s VFX system.

## Deliverables
The project, a preview at gameplay distance and speed.', 130),

('game-brief-mod', 'brief_template', 'game', 'community', 'en',
 'Brief: mod', 'A structured brief for a modding challenge.',
 '## Context
The target game and the feature added.

## Rules
Vendor terms respected, nothing proprietary re-hosted.

## Distribution
The platform, the live URL, the install steps.

## Attribution
Every third-party asset credited.', 140),

('game-writeup-postmortem', 'writeup_template', 'game', NULL, 'en',
 'Template: game post-mortem', 'The write-up that turns a shipped game into something others learn from.',
 '## What it is
One paragraph.

## What went right
The decisions that held.

## What went wrong
Honestly.

## What you would change
Next time.

## Credits
Everyone who touched it, third-party assets included.', 150),

('game-writeup-playtest-report', 'writeup_template', 'game', NULL, 'en',
 'Template: playtest report', 'How to summarise what playtesters showed you.',
 '## Sessions
How many, how long.

## Fun and clarity
The scores, and the pattern in them.

## What broke
Bugs, confusion, where they stalled.

## What you changed
Because of it — this is the part that matters.', 160);

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM content_guides WHERE skill_domain = 'game';
    IF n < 16 THEN RAISE EXCEPTION 'expected 16 game guides, found %', n; END IF;
END $$;
