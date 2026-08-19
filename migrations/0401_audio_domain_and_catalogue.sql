-- The audio domain opens, with five trades.
--
-- ## Why audio is a domain and not a corner of `game`
--
-- The catalogue held exactly one audio trade — `game-sound-engineer`, seeded
-- in 0088 — and it was filed under `game`. That said something false about
-- the work: a composer scores a film, a documentary and a game with the same
-- craft, a voice actor narrates an audiobook one week and a character the
-- next, and a sound designer builds a UI kit for a SaaS product the same way
-- they build one for a menu screen. Filing all of that under `game` makes
-- four fifths of the field invisible, and tells somebody who does it that the
-- platform thinks they are a game developer who also does sound.
--
-- Audio is the first domain here that is defined by the medium rather than by
-- the destination, and it crosses `game`, `design`, `code` and `ops`. That is
-- why `secondary_domains` is populated for every one of the five: the trades
-- reach outward by nature, and the reach is the point.
--
-- ## Five trades, four review families
--
-- The backlog asked for one review capability per trade. Migration 0176
-- settled why that is wrong — review rights are granted by family, because
-- nobody reviews at trade granularity — and the same reasoning applies here
-- with one nuance: the families are not one-per-trade *and* not one-for-all.
--
--   * `composition` — writing music. Judged on mood, mix and coherence across
--     a set.
--   * `sound-design` — SFX, foley, ambiences. Judged on whether a sound
--     serves the thing it is attached to.
--   * `voice` — performance. Judged on delivery, range and recording quality,
--     and by somebody who can hear the difference between a bad take and a
--     bad room.
--   * `implementation` — the music implementer and the audio programmer.
--     Both are judged on whether the sound arrives correctly at runtime,
--     within budget, and both read a middleware project or a DSP graph. One
--     family, two trades.
--
-- ## `design-sound` does not exist
--
-- The backlog listed it as a legacy orientation to deprecate. It was never
-- seeded — the catalogue has `game-sound-engineer` and nothing else — so
-- there is nothing here to archive, and a migration that archived it anyway
-- would have written a `replaced_by` lineage for a trade nobody ever held.

UPDATE skill_domains
   SET is_active = TRUE, updated_at = NOW()
 WHERE slug = 'audio';

-- ═══════════════════════════════════════════════════════════════════
-- The five trades
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations
    (slug, name, description, primary_domain, secondary_domains, tags, is_curated, reviewer_group)
VALUES

('audio-composer', 'Compositeur',
 'Écrire de la musique originale : thème de jeu, score narratif, jingle de marque, générique de podcast. Une intention tenue sur une série, pas un morceau réussi.',
 'audio', ARRAY['game', 'design'], ARRAY['musique', 'composition', 'mixage'], TRUE, 'composition'),

('audio-music-implementer', 'Intégrateur musical adaptatif',
 'Faire réagir la musique au jeu : couches d''intensité, transitions, reséquencement. FMOD, Wwise ou le moteur nu. La partition est écrite, le comportement se programme.',
 'audio', ARRAY['game', 'code'], ARRAY['fmod', 'wwise', 'musique-adaptative'], TRUE, 'implementation'),

('audio-sound-designer', 'Designer sonore',
 'Bruitages, sons d''interface, ambiances. Un son qui sert ce à quoi il est attaché — et qu''on remarque seulement quand il manque.',
 'audio', ARRAY['game', 'design'], ARRAY['sfx', 'foley', 'ambiance'], TRUE, 'sound-design'),

('audio-voice-actor', 'Comédien voix',
 'Interpréter : personnages, narration, voix commerciale. La performance, et la prise qui la rend utilisable.',
 'audio', ARRAY['game', 'soft_skills'], ARRAY['voix', 'narration', 'doublage'], TRUE, 'voice'),

('audio-programmer', 'Programmeur audio',
 'Le son au niveau du code : DSP, spatialisation, synthèse procédurale, moteurs. Là où la latence et la charge CPU décident de ce qui est possible.',
 'audio', ARRAY['code', 'game'], ARRAY['dsp', 'spatial', 'procedural'], TRUE, 'implementation');

-- ═══════════════════════════════════════════════════════════════════
-- English
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientation_translations (orientation_id, locale, name, description)
SELECT o.id, 'en', t.name, t.description
FROM (VALUES
    ('audio-composer', 'Composer',
     'Writing original music: game themes, narrative scores, brand jingles, podcast idents. An intention held across a set, not one lucky track.'),
    ('audio-music-implementer', 'Adaptive Music Implementer',
     'Making music answer the game: intensity layers, transitions, resequencing. FMOD, Wwise, or the bare engine. The score is written; the behaviour is programmed.'),
    ('audio-sound-designer', 'Sound Designer',
     'Effects, interface sounds, ambiences. A sound that serves what it is attached to — and that you notice only when it is missing.'),
    ('audio-voice-actor', 'Voice Actor',
     'Performance: characters, narration, commercial reads. The take, and the recording that makes it usable.'),
    ('audio-programmer', 'Audio Programmer',
     'Sound at the code level: DSP, spatialisation, procedural synthesis, engines. Where latency and CPU budget decide what is possible.')
) AS t(slug, name, description)
JOIN orientations o ON o.slug = t.slug
ON CONFLICT (orientation_id, locale) DO UPDATE
    SET name = EXCLUDED.name,
        description = EXCLUDED.description,
        updated_at = NOW();

-- ═══════════════════════════════════════════════════════════════════
-- The one legacy trade
-- ═══════════════════════════════════════════════════════════════════
--
-- `game-sound-engineer` covered sound design and music implementation at
-- once, which is why the backlog could not decide which of the two replaces
-- it. `replaced_by` takes one, and it takes `audio-sound-designer`: that is
-- what the overwhelming majority of the work under that name actually was,
-- and the trade it points at is the one a reader should be sent to.
--
-- Nobody loses anything. `user_orientations` holds the orientation id, an
-- archived orientation keeps its rows, and the lineage is what lets a search
-- for the new trade find people who claimed the old one.

UPDATE orientations
   SET is_archived = TRUE,
       replaced_by = (SELECT id FROM orientations WHERE slug = 'audio-sound-designer'),
       updated_at = NOW()
 WHERE slug = 'game-sound-engineer';
