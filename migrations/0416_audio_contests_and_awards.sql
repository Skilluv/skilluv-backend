-- Audio contests, and the constraint that keeps eating contest kinds.
--
-- ## The one migration 0228 said would happen again
--
-- 0228 restored `code_golf` and `tdd_contest` after 0223 deleted them by
-- restating `tournaments_kind_check`, and wrote the warning out: "any
-- migration that restates this CHECK must restate *all* of it. There is no
-- way to add one value without rewriting the list, which means every addition
-- is an opportunity to silently delete somebody else's."
--
-- Audio is that addition. Rather than take the bet a fourth time, the kinds
-- become rows — the same move as 0400, 0404, 0406, 0408, 0413 and 0415.
--
-- The table also carries three things the CHECK could not, and that
-- `services::contest` and `services::tournament` currently hold in three
-- separate Rust constants that have to agree with each other and with the
-- database:
--
--   * whether the kind expects a submission at all (`KINDS_WITH_SUBMISSIONS`);
--   * whether it is ranked by a measured number or by a judgement
--     (`MEASURED_KINDS`);
--   * which keys its `rules` object must carry (`validate_rules`).
--
-- A new format is now a row plus, at most, a scoring branch.
--
-- ## The two audio formats
--
-- **`audio_sound_battle`** — the same brief, a short clock, and a community
-- vote. The backlog specified two designers head to head; the row does not
-- encode the number, because the interesting variant is the one it also
-- described — a sound designer paired with an illustrator against another
-- pair — and a format that hard-codes "two" cannot express it. The rules
-- object states the brief, the clock and how many enter.
--
-- **`audio_composition_contest`** — a theme, a duration bracket, a deadline.
-- Longer than a battle because writing music is not a sprint, and judged on
-- entries that are listened to in full.
--
-- Both are ranked by vote rather than by measurement. There is no audio
-- equivalent of counting characters in a code golf, and pretending otherwise
-- — loudest, longest, most downloaded — would rank the wrong thing.

CREATE TABLE tournament_kinds (
    slug VARCHAR(30) PRIMARY KEY,
    -- The domain the format belongs to. NULL for the ones that predate
    -- domains and work anywhere.
    skill_domain VARCHAR(30) REFERENCES skill_domains(slug) ON UPDATE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT NOT NULL,
    -- Whether entrants hand something in. The kinds that do not are scored
    -- from activity elsewhere, and asking for a link would be theatre.
    expects_submission BOOLEAN NOT NULL DEFAULT FALSE,
    -- Whether the ranking comes from a number the entrant measures rather
    -- than from an opinion.
    is_measured BOOLEAN NOT NULL DEFAULT FALSE,
    -- Which end of that number wins. Only meaningful when `is_measured`, and
    -- getting it wrong crowns the worst entry: a code golf sorted descending
    -- rewards the longest program.
    lower_is_better BOOLEAN NOT NULL DEFAULT FALSE,
    -- What the `rules` object must state before anybody can enter. Checked at
    -- creation: a contest missing its problem link is not a form with a gap,
    -- it is an announcement nobody can act on.
    required_rule_keys TEXT[] NOT NULL DEFAULT '{}',
    sort_order SMALLINT NOT NULL DEFAULT 100
);

COMMENT ON TABLE tournament_kinds IS
    'Every contest format. A table rather than the CHECK that 0189, 0223 and '
    '0228 rewrote — 0223 deleted two of 0189''s values doing it — and it '
    'carries what the Rust constants KINDS_WITH_SUBMISSIONS, MEASURED_KINDS '
    'and validate_rules held separately.';

INSERT INTO tournament_kinds
    (slug, skill_domain, name, description, expects_submission, is_measured,
     lower_is_better, required_rule_keys, sort_order) VALUES
    -- Migration 0030
    ('individual', NULL, 'Tournoi individuel',
     'Un classement personnel sur une période.', FALSE, FALSE, FALSE, '{}', 10),
    ('guild_war', NULL, 'Guerre de guildes',
     'Deux guildes ou plus, comparées sur ce qu''elles produisent.', FALSE, FALSE, FALSE, '{}', 20),
    ('hackathon', NULL, 'Hackathon',
     'Un thème, une horloge, un projet livré à la fin.', TRUE, FALSE, FALSE, '{theme}', 30),
    -- Migration 0114
    ('marathon', NULL, 'Marathon',
     'La régularité sur une longue période plutôt que la pointe.', FALSE, FALSE, FALSE, '{target_merged_prs}', 40),
    ('defi_solitaire', NULL, 'Défi solitaire',
     'Une épreuve individuelle, hors classement collectif.', FALSE, FALSE, FALSE, '{}', 50),
    -- Migration 0189
    ('code_golf', 'code', 'Code golf',
     'La solution qui marche en le moins de caractères, une langue à la fois.', TRUE, TRUE, TRUE,
     '{language,problem_url}', 60),
    ('tdd_contest', 'code', 'Concours TDD',
     'Le même problème pour tout le monde, jugé autant sur les tests que sur le code.', TRUE, FALSE, FALSE,
     '{problem_url,judging_criteria}', 70),
    -- Migration 0223
    ('benchmark_rush', 'ai', 'Course au banc',
     'Un banc public, une horloge courte, un score que quelqu''un d''autre doit pouvoir rejouer.', TRUE, TRUE, FALSE,
     '{benchmark_url,metric}', 80),
    ('prompt_battle', 'ai', 'Duel d''invites',
     'La même tâche, des invites différentes, jugées sur ce qu''elles obtiennent.', TRUE, FALSE, FALSE,
     '{task_description}', 90),
    -- Migration 0235. Design was on its own branch when this table was
    -- written; both formats are domain-agnostic, and leaving them out would
    -- have made every duel and every brief contest fail the foreign key that
    -- replaces the CHECK below.
    ('duel', NULL, 'Duel',
     'Deux personnes, une tâche, un vote. Le nombre de participants est dans '
     'les règles du format, pas dans le genre.', TRUE, FALSE, FALSE,
     '{task_description}', 100),
    ('brief_contest', NULL, 'Concours sur brief',
     'Un brief écrit, N réponses, un jury qui les classe. Le brief est la '
     'moitié de l''épreuve : sans lui il n''y a rien à juger.', TRUE, FALSE, FALSE,
     '{brief}', 105),
    -- Audio
    ('audio_sound_battle', 'audio', 'Duel de design sonore',
     'Un brief surprise, quarante-huit heures, et la communauté qui écoute. '
     'Le nombre de participants est dans les règles et non dans le format : la '
     'variante la plus intéressante fait s''affronter des paires — un designer '
     'sonore avec un illustrateur — et un format qui code « deux » ne peut pas '
     'l''exprimer.', TRUE, FALSE, FALSE,
     '{brief,duration_hours,entrants}', 210),
    ('audio_composition_contest', 'audio', 'Concours de composition',
     'Un thème, une fourchette de durée, une date. Plus long qu''un duel : '
     'écrire de la musique n''est pas un sprint, et les entrées sont écoutées '
     'en entier.', TRUE, FALSE, FALSE,
     '{theme,duration_bracket}', 220);

ALTER TABLE tournaments
    DROP CONSTRAINT IF EXISTS tournaments_kind_check,
    ADD CONSTRAINT tournaments_kind_fkey
        FOREIGN KEY (kind) REFERENCES tournament_kinds(slug) ON UPDATE CASCADE;

COMMENT ON CONSTRAINT tournaments_kind_fkey ON tournaments IS
    'Points at `tournament_kinds`. Replaces the CHECK whose rewrites cost '
    'migration 0189 two of its values — a new format is now an INSERT.';

-- ═══════════════════════════════════════════════════════════════════
-- Six audio categories, in the ceremony that already exists
-- ═══════════════════════════════════════════════════════════════════
--
-- Same reasoning as 0303: one ceremony, categories from every domain. A
-- composer and a library author named on the same evening is what makes the
-- audio categories visible to people who would never have looked for them.

INSERT INTO award_categories (slug, name, description, subject_type, sort_order)
VALUES
    ('best-original-composition',
     'Best Original Composition of the Year',
     'Une musique originale qui tient seule et qui sert ce à quoi elle est attachée. Jugée à l''écoute, pas au nombre de pistes.',
     'deliverable', 300),

    ('best-sound-design',
     'Best Sound Design of the Year',
     'Un ensemble de sons qui fait exister quelque chose. La catégorie où le meilleur travail est celui qu''on ne remarque pas séparément.',
     'deliverable', 310),

    ('best-voice-performance',
     'Best Voice Performance of the Year',
     'Une interprétation qui donne à quelqu''un une voix qu''on reconnaît la fois suivante.',
     'deliverable', 320),

    ('best-adaptive-music-system',
     'Best Adaptive Music System of the Year',
     'Une musique qui répond au jeu sans qu''on entende la mécanique. Récompense l''intégration autant que l''écriture.',
     'project', 330),

    ('best-audio-engineering',
     'Best Audio Engineering of the Year',
     'Du code audio — DSP, spatialisation, synthèse — que d''autres ont repris. Jugé sur ce qu''il rend possible.',
     'project', 340),

    ('audio-community-contribution',
     'Community Audio Contribution of the Year',
     'Des sons ou des morceaux versés en licence libre, et repris par des gens que leur auteur ne connaît pas.',
     'user', 350);
