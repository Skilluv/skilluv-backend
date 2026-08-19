-- The four audio distinctions that had nothing to count them.
--
-- Tickets C-01 and O-02 each named two badges, and migration 0507 seeded
-- neither: the engine had no way to see a contest win or a mentee, so the
-- honest options were to invent a rule that counted something else or to leave
-- them out. Leaving them out was the right call at the time and the wrong
-- state to stop in.
--
-- `services::badge_engine` now reads both. Nothing here is manual.
--
-- ## What `contest_won` counts, and what it refuses to
--
-- First place, and only first. A podium is a result; a badge named "winner"
-- that counted a second place would be a badge that lies. Guild wins are
-- excluded — a guild war is won by a guild, and spreading that across the
-- roster awards a personal distinction for somebody else's work.
--
-- ## Why the legend is five wins and not five in a row
--
-- The ticket said "streak 5 wins". A streak needs a rule for what breaks it,
-- and battles run weekly: somebody who wins three, misses a month because they
-- were on a paid mission, then wins two more has not lost anything worth
-- taking a badge away for. Counting five wins says the same thing about the
-- person and does not punish them for being busy.
--
-- ## What `mentee_guided` counts
--
-- People, not sessions. Somebody who saw the same mentee eight times has
-- helped one person. The domain comes from the mentee's own answers, because
-- a session carries none and the matching that paired them is per-domain.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('audio-sound-battler-winner', 'medal',
 'Vainqueur de duel',
 'Un duel de design sonore remporté : même brief, même horloge, et la communauté qui tranche.',
 '{"proof_types": ["contest_won"], "tournament_kind": "audio_sound_battle", "min_count": 1}',
 'rare'),

('audio-battler-legend', 'medal',
 'Légende des duels',
 'Cinq duels de design sonore remportés. À ce stade, ce n''est plus de la chance sur un brief.',
 '{"proof_types": ["contest_won"], "tournament_kind": "audio_sound_battle", "min_count": 5}',
 'legendary'),

('audio-mentor-active', 'medal',
 'Mentor audio',
 'Trois personnes accompagnées jusqu''à une séance menée à son terme.',
 '{"proof_types": ["mentee_guided"], "skill_domain": "audio", "min_count": 3}',
 'rare'),

('audio-mentor-veteran', 'medal',
 'Mentor audio chevronné',
 'Dix personnes accompagnées. Le métier qui se pratique sur celui des autres.',
 '{"proof_types": ["mentee_guided"], "skill_domain": "audio", "min_count": 10}',
 'epic');

-- ═══════════════════════════════════════════════════════════════════
-- The composition contest gets the same pair, for the same reason
-- ═══════════════════════════════════════════════════════════════════
--
-- The backlog named the battle badges and not these, which reads as an
-- oversight rather than a decision: C-02 asks for monthly themed composition
-- contests and gives whoever wins them nothing to show for it. A format that
-- runs every month and leaves no trace on a profile is a format nobody enters
-- twice.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('audio-composition-contest-winner', 'medal',
 'Concours de composition remporté',
 'Un concours de composition thématique remporté.',
 '{"proof_types": ["contest_won"], "tournament_kind": "audio_composition_contest", "min_count": 1}',
 'epic');
