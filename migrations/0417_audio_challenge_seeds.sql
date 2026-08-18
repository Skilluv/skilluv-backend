-- Twenty-four challenges, one set per audio trade.
--
-- ## Why they are drafts
--
-- Same reason as 0185 and 0219: the title and the intent come from the
-- backlog, and the full brief — the reference tracks, the exact deliverable,
-- what is out of scope — needs an author who knows the trade. A challenge
-- nobody has reviewed must not be offered to somebody learning, and `draft`
-- is the state the workflow already has.
--
-- Seeding them anyway is the point. Five trades with an empty catalogue are
-- five trades the platform claims to support and cannot.
--
-- ## Why the instructions are built rather than written out
--
-- 0185 wrote a hundred and thirty-eight briefs by hand and every one repeats
-- the same headings. The variable part — what to do, and what comes out — is
-- what the rows carry.
--
-- ## The paragraph every audio brief ends on
--
-- Sources declared, and loudness measured. Those are the two reasons an audio
-- submission comes back, and both are things the author can check before
-- anybody else has to.
--
-- ## `ai_policy`
--
-- `disclosure_required` on the twenty-one that are not voice work, which is
-- the platform default and means a generative tool is allowed and has to be
-- said. `human_verified` on the four voice challenges: a voice is an
-- attribute of a person, a cloned one is indistinguishable to a listener, and
-- a demo reel that might not be the performer's own voice is worth nothing to
-- anybody. This is the one place in the catalogue where the stricter policy
-- protects the entrant rather than the platform.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, ai_policy, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## Ce qu''il y a à faire' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Ce qui est attendu' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'Dans tous les cas : chaque source utilisée est déclarée avec sa licence, ' ||
    'ou tout est original et c''est écrit — une source non tracée rend la ' ||
    'livraison inutilisable quel que soit le reste. Le niveau est mesuré ' ||
    '(LUFS, crête vraie) et adapté à la destination. Les formats, le nommage ' ||
    'et l''usage sont documentés. Un travail sans documentation est refusé.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de revue de la famille s''applique, et elle est publique : ' ||
    'tu peux la lire avant de soumettre.',
    'audio', c.difficulty, c.language,
    'draft', TRUE, c.ai_policy,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'audio' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'audio' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── audio-composer (6) ─────────────────────────────────────────────
('audio-composer', 'Thème principal de jeu — 90 secondes',
 'Écrire le thème principal d''un jeu existant de la communauté, en quatre-vingt-dix secondes, avec une idée qu''on retient',
 'Le master, les stems, et une note qui dit quelle intention le thème porte et pourquoi ces choix.', 3, NULL, 'disclosure_required'),

('audio-composer', 'Score narratif à l''image — 60 secondes',
 'Mettre en musique une minute de montage imposée : points de synchronisation, respiration, silence',
 'Le master aligné sur la vidéo, les stems, et la liste des points de synchronisation visés.', 4, NULL, 'disclosure_required'),

('audio-composer', 'Jingle de marque et ses trois formats',
 'Écrire un jingle de quinze secondes et ses déclinaisons — 5 s, 3 s, une note isolée — qui restent reconnaissables',
 'Les quatre versions, cohérentes entre elles, et la logique de réduction écrite.', 3, NULL, 'disclosure_required'),

('audio-composer', 'Générique de podcast — entrée et sortie',
 'Une entrée de quinze secondes et une sortie de vingt, du même monde, utilisables sous une voix',
 'Les deux pièces, une version instrumentale allégée pour passer sous la parole, et les stems.', 2, NULL, 'disclosure_required'),

('audio-composer', 'Boucle d''ambiance de trois minutes',
 'Écrire une boucle longue dont personne ne peut entendre le raccord ni deviner la durée',
 'La boucle, la démonstration qu''elle reboucle proprement, et les stems.', 3, NULL, 'disclosure_required'),

('audio-composer', 'Bande originale de cinq morceaux',
 'Une identité tenue sur cinq morceaux : menu, exploration, ville, combat, boss',
 'Les cinq masters, les stems, et une note qui montre le matériau commun et comment il est varié.', 5, NULL, 'disclosure_required'),

-- ── audio-music-implementer (4) ────────────────────────────────────
('audio-music-implementer', 'Musique de combat adaptative — FMOD',
 'Intégrer une musique de combat à trois intensités qui monte et redescend sans casser la mesure',
 'Le projet FMOD, une build jouable où l''on peut déclencher les états, et la note d''intégration.', 4, NULL, 'disclosure_required'),

('audio-music-implementer', 'Remixage vertical à trois couches',
 'Un morceau dont trois couches s''activent séparément selon l''état du jeu, sans rupture',
 'Le projet middleware, les stems utilisés, et une démonstration des huit combinaisons.', 3, NULL, 'disclosure_required'),

('audio-music-implementer', 'Reséquencement horizontal',
 'Une musique qui enchaîne ses segments dans un ordre décidé par le jeu, avec des sorties musicales',
 'Le projet, la carte des transitions, et une build où l''on peut forcer chaque enchaînement.', 4, NULL, 'disclosure_required'),

('audio-music-implementer', 'Ambiance adaptative — Wwise',
 'Une ambiance qui répond à la météo, à l''heure et au biome sans qu''on entende les bascules',
 'Le projet Wwise, les RTPC et states documentés, le budget mémoire mesuré, et une build.', 4, NULL, 'disclosure_required'),

-- ── audio-sound-designer (5) ───────────────────────────────────────
('audio-sound-designer', 'Pack d''interface de jeu — 15 sons',
 'Quinze sons d''interface cohérents : survol, clic, succès, erreur, ouverture, fermeture, transitions',
 'Les quinze sons nommés selon une convention, une feuille d''usage, et une démonstration en contexte.', 2, NULL, 'disclosure_required'),

('audio-sound-designer', 'Pack de combat — 20 sons',
 'Vingt sons de combat qui appartiennent au même monde : mêlée, distance, magie, impacts, morts',
 'Les vingt sons, la logique de famille écrite, et une démonstration superposée à une scène.', 3, NULL, 'disclosure_required'),

('audio-sound-designer', 'Cinq ambiances distinctes',
 'Forêt, grotte, ville, désert, océan — cinq lieux qui existent à l''oreille et ne se ressemblent pas',
 'Cinq boucles longues, leur découpage en couches, et la note qui dit ce que chaque couche apporte.', 3, NULL, 'disclosure_required'),

('audio-sound-designer', 'Pack d''interface pour un produit — 10 sons',
 'Dix sons discrets pour une application : notification, validation, erreur, navigation. Supportables à la centième écoute',
 'Les dix sons, la démonstration de leur discrétion, et la feuille d''intégration.', 2, NULL, 'disclosure_required'),

('audio-sound-designer', 'Pack de bruitage — 30 sons',
 'Trente bruitages enregistrés : pas, tissus, objets, portes. Du foley, pas des banques',
 'Les trente sons, les photos ou la description du dispositif de prise, et la feuille d''usage.', 4, NULL, 'disclosure_required'),

-- ── audio-voice-actor (5) ──────────────────────────────────────────
('audio-voice-actor', 'Bande démo — 90 secondes',
 'Une bande démo qui montre cinq registres distincts et une narration, sans temps mort',
 'La bande montée, la liste des extraits avec leur contexte, et les prises brutes d''au moins un extrait.', 3, NULL, 'human_verified'),

('audio-voice-actor', 'Cinq personnages de jeu de rôle',
 'Héros, antagoniste, mentor, comique, énigmatique — cinq voix qu''on distingue les yeux fermés',
 'Cinq extraits sur les mêmes répliques, et une note sur ce qui différencie chaque voix.', 4, NULL, 'human_verified'),

('audio-voice-actor', 'Narration de livre audio — 5 minutes',
 'Cinq minutes de narration tenue : rythme, respiration, personnages incarnés dans le dialogue',
 'La prise montée, propre, sans claquements ni respirations gênantes, et la version brute.', 4, NULL, 'human_verified'),

('audio-voice-actor', 'Voix commerciale — 30 secondes',
 'Une lecture commerciale de trente secondes, avec deux interprétations différentes du même texte',
 'Les deux versions montées, et la note qui dit à quel positionnement de marque chacune répond.', 2, NULL, 'human_verified'),

('audio-voice-actor', 'Casting communautaire — audition et livraison',
 'Répondre à un casting ouvert de la plateforme : audition, sélection, puis livraison des répliques',
 'L''audition, puis les prises finales retenues, avec l''étendue d''usage écrite.', 3, NULL, 'human_verified'),

-- ── audio-programmer (4) ───────────────────────────────────────────
('audio-programmer', 'Spatialisation binaurale dans un moteur',
 'Intégrer une spatialisation HRTF avec occlusion dans Bevy, Godot ou un moteur maison',
 'Le code, une démonstration jouable où l''on entend la source tourner et se faire masquer, et la mesure de coût CPU.', 5, 'rust', 'disclosure_required'),

('audio-programmer', 'Générateur de musique procédurale',
 'Écrire un générateur qui compose à l''exécution selon des règles, et qui ne se répète pas au bout de deux minutes',
 'Le code, une démonstration audio de dix minutes, et la note sur les règles employées.', 4, NULL, 'disclosure_required'),

('audio-programmer', 'Greffon de traitement',
 'Écrire un traitement — réverbération, égaliseur, distorsion — en greffon VST3/CLAP ou natif moteur',
 'Le code, un binaire ou une build, la réponse mesurée, et le coût par bloc.', 5, NULL, 'disclosure_required'),

('audio-programmer', 'Enveloppe de middleware',
 'Encapsuler FMOD ou Wwise derrière une interface propre dans un moteur qui n''en a pas',
 'Le code, l''exemple minimal qui tourne, et la documentation des cas d''erreur — banque absente, voix saturées.', 5, 'rust', 'disclosure_required')

) AS c(orientation_slug, title, description, expected, difficulty, language, ai_policy)
JOIN orientations o ON o.slug = c.orientation_slug;
