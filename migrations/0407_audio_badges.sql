-- Thirteen audio distinctions.
--
-- ## Twelve are counted, one is not
--
-- Migration 0212 set the standard: a rule that counts something else than
-- what the badge names awards it to people who never did the thing. Where a
-- rule can be written it is written, and where it cannot the row says a human
-- decides.
--
-- The bases created in 0406 are what make twelve of these countable. An
-- attestation resting on a delivered sound pack is a row with a value in it,
-- so the rule says so instead of asking an operator.
--
-- ## Where the backlog's thresholds moved, and why
--
-- Two of them described a career rather than a badge.
--
--   * **Twenty sound packs** was the ask for `audio-sfx-master`. A pack is
--     fifteen to thirty sounds designed, recorded, layered and mixed to hold
--     together; twenty of those is several years of full-time work. Ten is
--     already the mark of somebody who does this for a living, which is what
--     the badge is for.
--   * **Ten distinct characters voiced** was the ask for
--     `audio-voice-versatile`, and nothing counts a character: a reel is one
--     deliverable whether it holds two voices or nine. Counting reels instead
--     would have renamed the badge without saying so, so the badge is aimed at
--     what the platform can actually see — three validated reels, which for a
--     voice actor is three separate bodies of demonstrated range.
--
-- ## Where a badge was added
--
-- `audio-multi-trade`, on the model of `ai-multi-modal`. It is the one that
-- describes what this domain is unusual for: the same person composing, then
-- designing sound, then implementing it is the norm here rather than the
-- exception, and nothing else in the set would have shown it.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('audio-first-artifact', 'medal',
 'Premier artefact audio',
 'Un premier livrable audio vérifié. Le moment où le profil cesse d''être déclaratif.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "audio", "min_count": 1}', 'common'),

('audio-craft-master', 'medal',
 'Maître d''œuvre audio',
 'Vingt livrables audio vérifiés. La régularité, pas le coup d''éclat.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "audio", "min_count": 20}', 'epic'),

('audio-craft-legend', 'medal',
 'Légende de l''atelier audio',
 'Soixante livrables audio vérifiés.',
 '{"proof_types": ["deliverable_verified"], "skill_domain": "audio", "min_count": 60}', 'legendary'),

('audio-composer-published', 'medal',
 'Compositeur publié',
 'Cinq compositions originales livrées, écoutables, licences en règle.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "audio_composition_published", "min_count": 5}', 'rare'),

('audio-sfx-master', 'medal',
 'Maître du bruitage',
 'Dix packs sonores livrés — cohérents, nommés, utilisables tels quels.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "audio_soundpack_delivered", "min_count": 10}', 'epic'),

('audio-voice-versatile', 'medal',
 'Voix polyvalente',
 'Trois bandes démo validées, chacune démontrant un registre propre.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "audio_voice_reel_validated", "min_count": 3}', 'rare'),

('audio-adaptive-hero', 'medal',
 'Musique vivante',
 'Trois systèmes musicaux adaptatifs intégrés et vérifiés dans une build jouable.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "audio_adaptive_system_shipped", "min_count": 3}', 'epic'),

('audio-engine-contributor', 'medal',
 'Contributeur moteur audio',
 'Une fonctionnalité audio livrée dans un moteur ou une bibliothèque : DSP, spatialisation, synthèse.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "audio_programming_contribution", "min_count": 1}', 'rare'),

('audio-cross-project', 'medal',
 'Crédité cinq fois',
 'Cinq œuvres publiées portant un crédit audio. Le métier vit d''ordinaire à l''intérieur de celui d''un autre.',
 '{"proof_types": ["attestation_received"], "attestation_basis": "audio_project_credited", "min_count": 5}', 'epic'),

('audio-multi-trade', 'medal',
 'Polyvalent audio',
 'Du travail vérifié dans trois métiers audio différents.',
 '{"distinct_over": "orientation", "skill_domain": "audio", "min_count": 3}', 'epic'),

('audio-mission-veteran', 'medal',
 'Vétéran des missions audio',
 'Dix missions audio rémunérées menées à terme.',
 '{"proof_types": ["mission_completed"], "skill_domain": "audio", "min_count": 10}', 'legendary'),

('audio-oss-contributor', 'medal',
 'Contributeur audio open source',
 'Une contribution audio acceptée en amont, dans un dépôt qu''on ne contrôle pas.',
 '{"proof_types": ["slice_merged_upstream"], "skill_domain": "audio", "min_count": 1}', 'rare'),

('audio-featured', 'medal',
 'Mis en avant',
 'Un travail audio retenu par la rédaction pour son exemplarité.',
 '{"proof_types": ["deliverable_featured"], "skill_domain": "audio", "min_count": 1}', 'rare');

-- ═══════════════════════════════════════════════════════════════════
-- The one a human decides
-- ═══════════════════════════════════════════════════════════════════
--
-- A jam soundtrack is judged by the people who played the game, at an event
-- the platform did not run and does not hold results for. There is nothing
-- honest to count, and inventing a proxy — most-listened track that month,
-- say — would name one thing and reward another.

INSERT INTO badge_rules (slug, output_type, display_name, description, conditions, rarity) VALUES

('audio-community-jam-winner', 'medal',
 'Bande-son de jam',
 'La musique ou les sons d''un jeu primé en game jam.',
 '{"manual": true}', 'legendary');
