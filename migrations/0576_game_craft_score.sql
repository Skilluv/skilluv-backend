-- The game craft score: the formula terms and the tier names.
--
-- Storage is already there — `craft_scores` (0204) holds one row per person per
-- domain, and `game` was in 0204's domain list, so it inherited the generic
-- tiers copied from `code`. This migration does two things: it writes the terms
-- the score is made of (`craft_score_weights`), and it replaces the inherited
-- tiers with names that mean something to a game creator.
--
-- No column is added to `users`. 0204 moved craft scores off `users` into their
-- own table precisely so a new domain is a set of rows, not an ALTER — the W-01
-- ticket's `ADD COLUMN craft_score_game` predates that and is stale.
--
-- ## The terms
--
-- Counts, mostly, each worth what a studio would weigh it at: a shipped title
-- above a jam win above a published mod. One offset-scaled term for the review
-- grids, counted from three out of five so an average grid adds nothing and
-- only work above the bar pays. The service (game_craft_score) measures each
-- term; a term with no measurement scores zero rather than guessing, which is
-- the same contract every other domain's weights have.

DELETE FROM craft_score_weights WHERE skill_domain = 'game';

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
('game', 'attestations', 5.00, 'count', NULL,
 'Each validated game attestation.', 100),
('game', 'jam_wins', 100.00, 'count', NULL,
 'Each game jam won (top three of its field).', 110),
('game', 'jam_top3', 50.00, 'count', NULL,
 'Each jam placement — counted alongside a win, so a win is worth both.', 120),
('game', 'shipped_titles', 150.00, 'count', NULL,
 'Each title published where players can reach it, confirmed.', 130),
('game', 'mods_published', 30.00, 'count', NULL,
 'Each mod validated on the platform its game uses.', 140),
('game', 'mods_viral', 80.00, 'count', NULL,
 'Each published mod past a thousand downloads.', 150),
('game', 'open_source_contributions', 40.00, 'count', NULL,
 'Each pull request merged into an engine or open-source game.', 160),
('game', 'missions_completed', 100.00, 'count', NULL,
 'Each paid game mission carried through and accepted.', 170),
('game', 'review_grid_average', 200.00, 'offset_scaled', 3.00,
 'The average of the review grids received, counted from 3 out of 5.', 180),
('game', 'playtests_contributed', 3.00, 'count', NULL,
 'Each playtest given to another creator, questionnaire filled in.', 190),
('game', 'portfolio_projects', 5.00, 'count', NULL,
 'Each project linked from an external portfolio (itch, ArtStation, Nexus…).', 200),
('game', 'published_writeups', 20.00, 'count', NULL,
 'Each published post-mortem or game write-up.', 210),
('game', 'years_active', 25.00, 'count', NULL,
 'Each year since the first validated game artefact.', 220),
('game', 'featured_times', 200.00, 'count', NULL,
 'Each featuring as game creator of the week.', 230);

-- The tiers. The inherited ones carry code's names; game ships things, so its
-- ladder is named for that. Replaced wholesale rather than renamed, because the
-- thresholds move too (a game creator is multi-facet — art, code, design — so
-- the ceiling sits higher than code's).
DELETE FROM craft_score_tiers WHERE skill_domain = 'game';

INSERT INTO craft_score_tiers
    (skill_domain, slug, name, min_score, max_score, description, sort_order)
VALUES
('game', 'apprentice', 'Apprentice', 0, 99,
 'Learning the craft on prototypes. Nothing shipped to players yet, and that is where everyone starts.', 10),
('game', 'prototyper', 'Prototyper', 100, 499,
 'Making things that run and play, iterating on what playtesters show. The loop is working.', 20),
('game', 'shipper', 'Shipper', 500, 1499,
 'Finishing and publishing — a title on itch, a mod with a download count, a jam that placed. Things reach players.', 30),
('game', 'craftsman', 'Craftsman', 1500, 3499,
 'A body of shipped work across more than one facet, and reviews that stay above the bar.', 40),
('game', 'legend', 'Legend', 3500, NULL,
 'A career of it — shipped titles, jam wins, mods people run, and a name other creators know.', 50);

DO $$
DECLARE w INT; t INT;
BEGIN
    SELECT count(*) INTO w FROM craft_score_weights WHERE skill_domain = 'game';
    SELECT count(*) INTO t FROM craft_score_tiers WHERE skill_domain = 'game';
    IF w <> 14 THEN RAISE EXCEPTION 'expected 14 game weights, found %', w; END IF;
    IF t <> 5  THEN RAISE EXCEPTION 'expected 5 game tiers, found %', t; END IF;
END $$;
