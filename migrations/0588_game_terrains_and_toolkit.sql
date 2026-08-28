-- The game domain's terrains and toolkit.
--
-- Rows in external_resource_categories and external_resources (0458), the same
-- catalogue security filled in 0556: the places a game creator practises for
-- real (open engines, open-source games, modding scenes) and the resources
-- worth their time. Slugs are game-prefixed so none collides with a row another
-- domain already owns.

INSERT INTO external_resource_categories (slug, skill_domain, name, description, sort_order) VALUES
('game_engines', 'game', 'Open engines',
 'Engines you can build a whole game in without a licence fee or a cert — where most Skilluv game work starts.', 800),
('game_oss_games', 'game', 'Open-source games',
 'Real, shipped games whose source is open: somewhere a contribution is a merged pull request in a game people actually play.', 810),
('game_modding', 'game', 'Modding and mapmaking scenes',
 'The platforms where community work is published and judged — a live URL and a download count, never a re-hosted asset.', 820),
('game_learning', 'game', 'Learning',
 'Talks and channels that teach the craft rather than a single engine.', 830)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO external_resources
    (slug, display_name, category, domain, url, summary, access_note,
     orientation_slugs, sort_order)
VALUES
('game-godot', 'Godot Engine', 'game_engines', 'game', 'https://godotengine.org',
 'Open, MIT, and the domain''s first choice — GDScript or C#, 2D and 3D, exports to web.',
 'Free.', ARRAY['game-gameplay-programmer','game-engine-programmer','game-level-designer'], 800),
('game-bevy', 'Bevy', 'game_engines', 'game', 'https://bevyengine.org',
 'Rust-native, ECS-first, WASM-friendly — the engine the platform''s own canvas games lean on.',
 'Free.', ARRAY['game-engine-programmer','game-graphics-programmer'], 810),
('game-love2d', 'LÖVE', 'game_engines', 'game', 'https://love2d.org',
 'A tiny Lua framework for 2D games — the fastest path to a playable prototype.',
 'Free.', ARRAY['game-gameplay-programmer'], 820),
('game-0ad', '0 A.D.', 'game_oss_games', 'game', 'https://play0ad.com',
 'A free, open-source real-time strategy game under active development.',
 'Contribute upstream.', ARRAY['game-gameplay-programmer','game-ai-programmer'], 830),
('game-wesnoth', 'Battle for Wesnoth', 'game_oss_games', 'game', 'https://www.wesnoth.org',
 'A mature open-source turn-based strategy game with a large content ecosystem.',
 'Contribute upstream, or build a campaign.', ARRAY['game-narrative-designer','game-level-designer'], 840),
('game-openttd', 'OpenTTD', 'game_oss_games', 'game', 'https://www.openttd.org',
 'An open-source transport-simulation game, long-lived and moddable.',
 'Contribute upstream.', ARRAY['game-systems-designer','game-gameplay-programmer'], 850),
('game-endless-sky', 'Endless Sky', 'game_oss_games', 'game', 'https://endless-sky.github.io',
 'An open-source space-trading and combat game, welcoming to content contributors.',
 'Contribute content or code upstream.', ARRAY['game-narrative-designer','game-economy-designer'], 860),
('game-nexus', 'Nexus Mods', 'game_modding', 'game', 'https://www.nexusmods.com',
 'Where mods for Skyrim, Fallout, Cyberpunk and many more are published and rated.',
 'Respect each game''s modding terms.', ARRAY['game-modder'], 870),
('game-curseforge', 'CurseForge', 'game_modding', 'game', 'https://www.curseforge.com',
 'The hub for Minecraft and other moddable games.',
 'Respect the platform and game terms.', ARRAY['game-modder'], 880),
('game-fortnite-creative', 'Fortnite Creative', 'game_modding', 'game', 'https://create.fortnite.com',
 'Build and publish playable islands inside Fortnite, with a shareable code.',
 'Epic account and the Creative terms.', ARRAY['game-mapmaker'], 890),
('game-gdc-vault', 'GDC Talks', 'game_learning', 'game', 'https://www.youtube.com/@Gdconf',
 'Postmortems and deep dives from the people who shipped the games.',
 'Free on YouTube.', ARRAY[]::TEXT[], 900),
('game-gmtk', 'Game Maker''s Toolkit', 'game_learning', 'game', 'https://www.youtube.com/@GMTK',
 'Design analysis that names why a mechanic works.',
 'Free on YouTube.', ARRAY['game-systems-designer','game-level-designer'], 910),
('game-brackeys', 'Brackeys', 'game_learning', 'game', 'https://www.youtube.com/@Brackeys',
 'Approachable tutorials for getting a first game running.',
 'Free on YouTube.', ARRAY['game-gameplay-programmer'], 920)
ON CONFLICT (slug) DO NOTHING;

DO $$
DECLARE c INT; r INT;
BEGIN
    SELECT count(*) INTO c FROM external_resource_categories WHERE skill_domain = 'game';
    SELECT count(*) INTO r FROM external_resources WHERE domain = 'game';
    IF c < 4  THEN RAISE EXCEPTION 'expected 4 game resource categories, found %', c; END IF;
    IF r < 13 THEN RAISE EXCEPTION 'expected 13 game resources, found %', r; END IF;
END $$;
