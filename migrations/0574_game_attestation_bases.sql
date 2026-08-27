-- The game domain's attestation bases, as rows.
--
-- Not a CHECK on `attestations.type` — 0406 made the bases a table precisely so
-- a domain opens by inserting rows, not by editing a constraint every other
-- domain shares. Eight bases, mirroring what F-10 asked for, and each one
-- declares the single thing that matters downstream: whether it rests on a
-- `deliverable`.
--
-- ## Why `requires_deliverable` splits these five from those three
--
-- The cross-domain rank counts deliverables. So the question each basis has to
-- answer is not "is this an achievement" but "is this a unit of shipped work
-- that should move a rank". Five are:
--
--   * game_artifact_validated  — a validated slice, the deliverable is the slice
--   * game_jam_winner          — the winning submission is the work
--   * game_shipped_title       — a title that reached players
--   * game_mod_published       — a mod that reached a platform, validated
--   * game_open_source_contribution — a pull request merged upstream
--
-- Three are recognition, not shipped output, and must not inflate a rank:
--
--   * game_jam_participant  — taking part is honest and attested, and that is all
--   * game_playtest_hero    — a contribution milestone (twenty playtests given)
--   * featured_game_creator — editorial, a person put forward by a human
--
-- This is the same line security drew between a confirmed finding (a
-- deliverable, moves the rank) and a captured flag (an attestation, does not).

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order)
VALUES
('game_artifact_validated', 'game', 'Game artefact validated',
 'A game deliverable — code, a build, a design document, an asset, an '
 'animation, a level — carried through review and at least three playtests to '
 'a validated slice.', TRUE, 700),

('game_jam_winner', 'game', 'Game jam winner',
 'A submission that placed in the top three of a Skilluv game jam, scored '
 'across the jam''s axes by community and jury.', TRUE, 710),

('game_jam_participant', 'game', 'Game jam participant',
 'A finished submission to a Skilluv game jam. Shipping something in a weekend '
 'is the achievement; it is attested and it does not move a rank.', FALSE, 720),

('game_shipped_title', 'game', 'Shipped title',
 'A game published where players can reach it — itch.io, GameJolt, a store — '
 'with the link and the credits, confirmed by a reviewer.', TRUE, 730),

('game_mod_published', 'game', 'Mod published',
 'A mod live on the platform its game uses, the vendor''s terms respected, '
 'attributed to its author, confirmed by a community reviewer. Skilluv hosts '
 'the proof and the metadata, never the package.', TRUE, 740),

('game_playtest_hero', 'game', 'Playtest hero',
 'Twenty playtests given to other creators, each with the questionnaire filled '
 'in. The domain runs on play, and this is the person who supplies it.', FALSE, 750),

('game_open_source_contribution', 'game', 'Open-source game contribution',
 'A pull request merged into an engine or an open-source game — Godot, Bevy, '
 '0 A.D., Wesnoth and their kind — with the commit and what it changed.', TRUE, 760),

('featured_game_creator', 'game', 'Featured game creator',
 'Put forward as the game creator of the week. Editorial, a human''s judgement, '
 'and it says so.', FALSE, 770);

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM attestation_bases WHERE skill_domain = 'game';
    IF n <> 8 THEN
        RAISE EXCEPTION 'expected 8 game attestation bases, found %', n;
    END IF;
END $$;
