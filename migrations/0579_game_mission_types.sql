-- The game domain's mission types.
--
-- Rows in `mission_types` (0192), on the generic `missions` infrastructure —
-- no `game_missions` table, the same call security made in 0553. A paid game
-- engagement is a mission like any other; what it is a mission *for* is the
-- twelve rows below, and the marketplace filters on them.

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order) VALUES
('gameplay_prototype', 'game', 'Gameplay prototype',
 'A mechanic proven in a playable build — proof it feels right before a team commits to it.', 800),
('engine_feature', 'game', 'Engine feature',
 'A custom engine or tooling feature: a system, a plugin, a native extension.', 810),
('asset_3d_pack', 'game', '3D asset pack',
 'A commissioned set of 3D assets — game-ready, in budget, on brief.', 820),
('asset_2d_pack', 'game', '2D asset pack',
 'A commissioned set of sprites, tiles or UI art.', 830),
('animation_pack', 'game', 'Animation pack',
 'Commissioned animation: locomotion, combat, reactions, cutscenes.', 840),
('level_design', 'game', 'Level design',
 'Levels built to a brief — layout, pacing, encounters, playtested.', 850),
('narrative_content', 'game', 'Narrative content',
 'Quests, dialogue, worldbuilding written and, where asked, implemented.', 860),
('vfx_pack', 'game', 'VFX pack',
 'Commissioned real-time effects, within a particle budget.', 870),
('sound_pack', 'game', 'Sound pack',
 'Commissioned game audio — the crossover with the audio domain, credited to it.', 880),
('full_game_prototype', 'game', 'Full game prototype',
 'A complete vertical slice: a small game, playable end to end.', 890),
('mod_commissioned', 'game', 'Commissioned mod',
 'A mod built to order for a studio or a creator, the vendor''s terms respected.', 900),
('consulting_game_design', 'game', 'Game design consulting',
 'Design review, balance, systems advice — expertise rather than a deliverable to host.', 910);

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM mission_types WHERE skill_domain = 'game';
    IF n <> 12 THEN RAISE EXCEPTION 'expected 12 game mission types, found %', n; END IF;
END $$;
