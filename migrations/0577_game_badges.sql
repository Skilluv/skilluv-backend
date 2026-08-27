-- The game domain's twenty badges.
--
-- Rows in `badge_rules`, evaluated by the proof engine (0090). Each names the
-- proof types and count that earn it. Most map onto conditions the engine
-- already knows — `attestation_received` filtered by basis, `deliverable_verified`
-- by domain, `tournament_podium` for jams, `mentorship_mentees_led`,
-- `slice_merged_upstream`.
--
-- Five family-expert badges and four composition badges name proof types the
-- engine learns with the domain's services (game_family_reviews, game_solo_ship,
-- game_team_ship, game_multi_artefact_ship, game_jam_organized) — the same way
-- security's engine learned `min_severity`. A condition the engine does not yet
-- evaluate simply does not award; it never mis-awards. The badge is defined
-- correctly here and lights up when its query lands, which is how a domain built
-- migrations-first stays honest.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

-- ── Cross-game (10) ─────────────────────────────────────────────────
('game-first-ship', 'medal', 'First ship',
 'A first game artefact carried through review and playtests. The moment a game profile stops being a list of tutorials.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "game_artifact_validated", "min_count": 1}', 'common'),
('game-craft-apprentice', 'medal', 'Craft apprentice',
 'Five validated game deliverables. The loop is working.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "game", "min_count": 5}', 'rare'),
('game-craft-master', 'medal', 'Craft master',
 'Thirty validated game deliverables across the craft.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "game", "min_count": 30}', 'epic'),
('game-craft-legend', 'medal', 'Craft legend',
 'A hundred validated game deliverables. A career of shipping.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "game", "min_count": 100}', 'legendary'),
('game-jam-winner', 'medal', 'Jam winner',
 'A game jam placed in the top three of its field.',
 '{"proof_types": ["tournament_podium"], "skill_domain": "game", "min_count": 1}', 'rare'),
('game-jam-champion', 'medal', 'Jam champion',
 'Three game jams placed. Doing it once is luck twice is a habit three times is a craft.',
 '{"proof_types": ["tournament_podium"], "skill_domain": "game", "min_count": 3}', 'epic'),
('game-shipped-title', 'medal', 'Shipped title',
 'A game published where players can reach it, confirmed.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "game_shipped_title", "min_count": 1}', 'rare'),
('game-playtest-hero', 'medal', 'Playtest hero',
 'Twenty playtests given to other creators. The person the domain runs on.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "game_playtest_hero", "min_count": 1}', 'rare'),
('game-mod-viral', 'medal', 'Mod that spread',
 'A published mod that reached players on its platform. How far it spread is in the craft score; that it shipped at all is here.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "game_mod_published", "min_count": 1}', 'epic'),
('game-mentor-active', 'medal', 'Active game mentor',
 'Three game creators guided to completion.',
 '{"proof_types": ["mentorship_mentees_led"], "skill_domain": "game", "min_count": 3}', 'rare'),

-- ── Family expert (5) — engine learns game_family_reviews ────────────
('game-family-programming-expert', 'medal', 'Programming family expert',
 'Twenty game programming artefacts reviewed. The reviewer other reviewers ask.',
 '{"proof_types": ["game_family_reviews"], "reviewer_group": "programming", "min_count": 20}', 'epic'),
('game-family-design-expert', 'medal', 'Design family expert',
 'Twenty game design artefacts reviewed.',
 '{"proof_types": ["game_family_reviews"], "reviewer_group": "design", "min_count": 20}', 'epic'),
('game-family-art-animation-expert', 'medal', 'Art and animation family expert',
 'Twenty game art and animation artefacts reviewed.',
 '{"proof_types": ["game_family_reviews"], "reviewer_group": "art-animation", "min_count": 20}', 'epic'),
('game-family-community-expert', 'medal', 'Community family expert',
 'Twenty mods and maps reviewed, terms and attribution checked each time.',
 '{"proof_types": ["game_family_reviews"], "reviewer_group": "community", "min_count": 20}', 'epic'),
('game-family-web3-expert', 'medal', 'Web3 family expert',
 'Twenty web3 game artefacts reviewed, contracts audited.',
 '{"proof_types": ["game_family_reviews"], "reviewer_group": "web3", "min_count": 20}', 'epic'),

-- ── Special (5) — engine learns the composition and jam proof types ──
('game-multi-artefact-shipper', 'medal', 'Multi-artefact shipper',
 'A game project shipped whole across more than one facet — code, art and design together.',
 '{"proof_types": ["game_multi_artefact_ship"], "min_count": 1}', 'epic'),
('game-open-source-contributor', 'medal', 'Open-source game contributor',
 'A pull request merged into an engine or an open-source game.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "game", "min_count": 1}', 'rare'),
('game-solo-shipper', 'medal', 'Solo shipper',
 'A whole game shipped alone — every role, one person.',
 '{"proof_types": ["game_solo_ship"], "min_count": 1}', 'epic'),
('game-collaborative-shipper', 'medal', 'Collaborative shipper',
 'A game shipped as one role of a team of three or more.',
 '{"proof_types": ["game_team_ship"], "min_count": 1}', 'rare'),
('game-community-jam-organizer', 'medal', 'Jam organizer',
 'A community game jam organised and run to its close.',
 '{"proof_types": ["game_jam_organized"], "min_count": 1}', 'epic');

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM badge_rules WHERE slug LIKE 'game-%';
    IF n <> 20 THEN RAISE EXCEPTION 'expected 20 game badges, found %', n; END IF;
END $$;
