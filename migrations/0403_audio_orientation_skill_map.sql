-- What each audio trade is actually made of.
--
-- ## Core and recommended
--
-- Core is what the trade cannot exist without: remove it and the person is
-- doing something else. Three to five per orientation. A trade where
-- everything is core says nothing about what to learn first, which is the
-- only thing this map is read for.
--
-- ## Why every one of the five points outside `audio`
--
-- More than in any other domain, and on purpose. An audio artefact is
-- delivered *into* something — a game build, a montage, a product — and the
-- rows that reach into `code`, `game` and `soft_skills` are what say so. A
-- music implementer who cannot read a Godot scene is not a music implementer,
-- and a voice actor who cannot take direction is a person with a microphone.
--
-- ## Rights are core for two trades and recommended for the rest
--
-- `sample-licence-hygiene` is core for the composer and the sound designer,
-- because both work from material they did not make and a single untraced
-- loop makes the whole delivery unusable to a client. `voice-consent-and-rights`
-- is core for the voice actor for the symmetric reason: it is their own
-- rights, not somebody else's, and nobody else will raise it for them.

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended)
SELECT o.id, s.id, m.is_core, NOT m.is_core
FROM (VALUES

-- ── audio-composer ─────────────────────────────────────────────────
('audio-composer', 'harmony-and-voicing', TRUE),
('audio-composer', 'melodic-writing', TRUE),
('audio-composer', 'theme-and-variation', TRUE),
('audio-composer', 'mixing-balance', TRUE),
('audio-composer', 'sample-licence-hygiene', TRUE),
('audio-composer', 'orchestration', FALSE),
('audio-composer', 'scoring-to-picture', FALSE),
('audio-composer', 'genre-pastiche', FALSE),
('audio-composer', 'midi-sequencing', FALSE),
('audio-composer', 'sample-library-writing', FALSE),
('audio-composer', 'seamless-looping', FALSE),
('audio-composer', 'music-loop-composition', FALSE),
('audio-composer', 'loudness-standards', FALSE),
('audio-composer', 'stem-delivery', FALSE),
('audio-composer', 'royalties-registration', FALSE),
('audio-composer', 'sync-licensing', FALSE),

-- ── audio-music-implementer ────────────────────────────────────────
('audio-music-implementer', 'adaptive-music', TRUE),
('audio-music-implementer', 'vertical-remixing', TRUE),
('audio-music-implementer', 'horizontal-resequencing', TRUE),
('audio-music-implementer', 'seamless-looping', TRUE),
('audio-music-implementer', 'fmod-integration', FALSE),
('audio-music-implementer', 'wwise-integration', FALSE),
('audio-music-implementer', 'engine-native-audio', FALSE),
('audio-music-implementer', 'audio-memory-budget', FALSE),
('audio-music-implementer', 'audio-middleware-debugging', FALSE),
('audio-music-implementer', 'stem-delivery', FALSE),
('audio-music-implementer', 'format-and-encoding', FALSE),
('audio-music-implementer', 'gameplay-programming', FALSE),
('audio-music-implementer', 'godot-fundamentals', FALSE),
('audio-music-implementer', 'technical-writing', FALSE),

-- ── audio-sound-designer ───────────────────────────────────────────
('audio-sound-designer', 'sound-layering', TRUE),
('audio-sound-designer', 'foley-recording', TRUE),
('audio-sound-designer', 'sound-pack-consistency', TRUE),
('audio-sound-designer', 'mixing-balance', TRUE),
('audio-sound-designer', 'sample-licence-hygiene', TRUE),
('audio-sound-designer', 'synthesis-subtractive', FALSE),
('audio-sound-designer', 'synthesis-fm-granular', FALSE),
('audio-sound-designer', 'ui-sound-design', FALSE),
('audio-sound-designer', 'ambience-design', FALSE),
('audio-sound-designer', 'sfx-naming-and-delivery', FALSE),
('audio-sound-designer', 'sfx-integration', FALSE),
('audio-sound-designer', 'microphone-technique', FALSE),
('audio-sound-designer', 'format-and-encoding', FALSE),
('audio-sound-designer', 'creative-commons-attribution', FALSE),
('audio-sound-designer', 'game-feel', FALSE),

-- ── audio-voice-actor ──────────────────────────────────────────────
('audio-voice-actor', 'character-voice-creation', TRUE),
('audio-voice-actor', 'narration-delivery', TRUE),
('audio-voice-actor', 'home-studio-voice-capture', TRUE),
('audio-voice-actor', 'voice-consent-and-rights', TRUE),
('audio-voice-actor', 'commercial-read', FALSE),
('audio-voice-actor', 'vocal-range-demonstration', FALSE),
('audio-voice-actor', 'accent-and-language-work', FALSE),
('audio-voice-actor', 'dialogue-editing', FALSE),
('audio-voice-actor', 'audio-restoration', FALSE),
('audio-voice-actor', 'microphone-technique', FALSE),
('audio-voice-actor', 'room-treatment', FALSE),
('audio-voice-actor', 'receiving-feedback', FALSE),
('audio-voice-actor', 'voice-direction', FALSE),

-- ── audio-programmer ───────────────────────────────────────────────
('audio-programmer', 'realtime-audio-constraints', TRUE),
('audio-programmer', 'dsp-filter-design', TRUE),
('audio-programmer', 'audio-latency-profiling', TRUE),
('audio-programmer', 'rust', TRUE),
('audio-programmer', 'spatial-audio-hrtf', FALSE),
('audio-programmer', 'procedural-audio', FALSE),
('audio-programmer', 'audio-plugin-development', FALSE),
('audio-programmer', 'audio-engine-bindings', FALSE),
('audio-programmer', 'engine-native-audio', FALSE),
('audio-programmer', 'audio-memory-budget', FALSE),
('audio-programmer', 'format-and-encoding', FALSE),
('audio-programmer', 'perf-profiling', FALSE),
('audio-programmer', 'technical-writing', FALSE)

) AS m(orientation_slug, skill_slug, is_core)
JOIN orientations o ON o.slug = m.orientation_slug
JOIN skill_nodes s ON s.slug = m.skill_slug
ON CONFLICT (orientation_id, skill_id) DO NOTHING;
