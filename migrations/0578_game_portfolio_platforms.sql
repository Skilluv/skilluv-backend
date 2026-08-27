-- The external platforms a game creator links a profile from.
--
-- Rows in `portfolio_platforms` (0415), one per place game work actually lives.
-- Every one carries `synced_by = NULL`: `has_public_api` says whether figures
-- could be fetched, but no worker fetches them yet, and the two are different
-- claims — one about the platform, one about this codebase. Stamping a sweep
-- here would put a platform in a set the sweep reads and reads nothing from.
-- The declared-not-fetched state is what the schema is for; a fetcher is later
-- work, and until it exists the honest value is NULL.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label,
     has_public_api, synced_by, sort_order)
VALUES
('itch', 'game', 'itch.io',
 'https://{handle}.itch.io', 'games', 'downloads', TRUE, NULL, 800),
('gamejolt', 'game', 'GameJolt',
 'https://gamejolt.com/@{handle}', 'games', 'plays', TRUE, NULL, 810),
('sketchfab', 'game', 'Sketchfab',
 'https://sketchfab.com/{handle}', '3D models', 'views', TRUE, NULL, 820),
('artstation', 'game', 'ArtStation',
 'https://www.artstation.com/{handle}', 'projects', 'likes', TRUE, NULL, 830),
('nexusmods', 'game', 'Nexus Mods',
 'https://www.nexusmods.com/users/{handle}', 'mods', 'downloads', TRUE, NULL, 840),
('curseforge', 'game', 'CurseForge',
 'https://www.curseforge.com/members/{handle}/projects', 'mods', 'downloads', TRUE, NULL, 850),
('moddb', 'game', 'ModDB',
 'https://www.moddb.com/members/{handle}', 'mods', 'visits', FALSE, NULL, 860),
('newgrounds', 'game', 'Newgrounds',
 'https://{handle}.newgrounds.com', 'games', 'views', FALSE, NULL, 870);

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM portfolio_platforms WHERE skill_domain = 'game';
    IF n < 8 THEN RAISE EXCEPTION 'expected 8 game portfolio platforms, found %', n; END IF;
END $$;
