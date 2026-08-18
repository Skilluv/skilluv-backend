-- The skills the audio trades are made of.
--
-- ## What the catalogue held before this
--
-- Four nodes: `game-audio` and its three children, seeded in 0057 and filed
-- under `game`. That is the whole of audio in a skill graph of about three
-- hundred and fifty — enough to say a game programmer touched sound, not
-- enough to describe a single one of the five trades opened in 0401.
--
-- ## The four that already existed are moved, not copied
--
-- `game-audio`, `sfx-integration`, `music-loop-composition` and
-- `adaptive-music` describe audio work, and they are moved into the audio
-- domain rather than duplicated there. Migration 0209 refused to duplicate
-- `python` under `ai` for the reason that applies here: two nodes for one
-- skill give two answers to whether somebody has it, and both are read.
--
-- Moving keeps the ids, so anything anybody already proved against them
-- survives. `game-audio` stays a root and keeps its slug: it names the
-- game-facing corner of audio — middleware, buses, engine integration — which
-- is a real category and not a duplicate of any other root here.
--
-- ## Naming
--
-- Each node names a technique or a tool, never a level. "Bon mixeur" is a
-- label nobody can claim honestly; "mixage aux normes de loudness (LUFS)" is
-- something a person has either done or not.
--
-- ## Where the tree deliberately stays shallow
--
-- Two levels, like the rest of the catalogue.

-- ═══════════════════════════════════════════════════════════════════
-- The four that move
-- ═══════════════════════════════════════════════════════════════════
--
-- `display_category` is set explicitly: the trigger of 0116 only fires on
-- INSERT, so a row that changes domain keeps the category of the domain it
-- left unless somebody says otherwise.

UPDATE skill_nodes
   SET domain = 'audio',
       display_category = skill_nodes_default_display_category('audio'),
       display_name = 'Audio de jeu et intégration moteur',
       description = 'Bus, middleware, déclenchement depuis le moteur. Le trajet du son entre le fichier et le haut-parleur.',
       updated_at = NOW()
 WHERE slug = 'game-audio';

UPDATE skill_nodes
   SET domain = 'audio',
       display_category = skill_nodes_default_display_category('audio'),
       updated_at = NOW()
 WHERE slug IN ('sfx-integration', 'music-loop-composition', 'adaptive-music');

-- ═══════════════════════════════════════════════════════════════════
-- Roots
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain) VALUES
('music-composition',    'Composition musicale',
 'Écrire de la musique : harmonie, orchestration, thème et variation. Ce qui reste quand on enlève la production.', 'audio'),
('sound-design',         'Design sonore',
 'Fabriquer un son qui n''existe pas : synthèse, bruitage, montage. Et le faire servir à quelque chose.', 'audio'),
('voice-performance',    'Interprétation vocale',
 'Jouer avec la voix : personnage, narration, lecture commerciale. La direction d''acteur incluse.', 'audio'),
('audio-programming',    'Programmation audio',
 'Le son au niveau de l''échantillon : DSP, spatialisation, synthèse temps réel, contraintes de latence.', 'audio'),
('audio-post-production','Prise, mixage et mastering',
 'Capter proprement, équilibrer, livrer aux normes. La partie du métier qu''on n''entend que quand elle est ratée.', 'audio'),
('audio-rights',         'Droits, licences et attribution',
 'Ce qui rend un son utilisable : provenance des échantillons, licences, cessions, consentement.', 'audio');

-- ═══════════════════════════════════════════════════════════════════
-- Children
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id)
SELECT c.slug, c.display_name, c.description, 'audio', p.id
FROM (VALUES

-- ── Composition musicale ───────────────────────────────────────────
('harmony-and-voicing',        'Harmonie et conduite des voix',      'Accords, tensions, renversements. Pourquoi un enchaînement fonctionne.', 'music-composition'),
('melodic-writing',            'Écriture mélodique',                 'Un thème qu''on retient et qu''on peut varier sans le perdre.', 'music-composition'),
('orchestration',              'Orchestration',                       'Répartir une idée entre les timbres, acoustiques ou virtuels.', 'music-composition'),
('theme-and-variation',        'Thème et variations',                 'Tenir une identité musicale sur cinq morceaux qui ne se ressemblent pas.', 'music-composition'),
('scoring-to-picture',         'Mise en musique à l''image',          'Écrire sur un montage : points de synchronisation, respiration, silence.', 'music-composition'),
('genre-pastiche',             'Écriture stylistique',                'Écrire crédiblement dans un genre imposé — chiptune, orchestral, lo-fi — sans le caricaturer.', 'music-composition'),
('midi-sequencing',            'Séquençage MIDI et humanisation',     'Vélocités, timing, articulations. Faire qu''une maquette virtuelle ne sonne pas comme une grille.', 'music-composition'),
('sample-library-writing',     'Écriture pour banques virtuelles',    'Kontakt, Spitfire, SINE. Composer en connaissant les limites des articulations disponibles.', 'music-composition'),
('seamless-looping',           'Bouclage sans couture',               'Une boucle de trois minutes dont on ne peut pas entendre le point de raccord.', 'music-composition'),

-- ── Design sonore ──────────────────────────────────────────────────
('foley-recording',            'Bruitage (foley)',                    'Enregistrer des objets pour fabriquer un pas, un tissu, une arme.', 'sound-design'),
('synthesis-subtractive',      'Synthèse soustractive',               'Oscillateurs, filtres, enveloppes. Le vocabulaire de base d''un son fabriqué.', 'sound-design'),
('synthesis-fm-granular',      'Synthèse FM et granulaire',           'Textures qu''un échantillon ne donne pas : métalliques, évolutives, impossibles.', 'sound-design'),
('sound-layering',             'Empilement de couches',               'Un impact = un claquement, un corps, une queue. Le découpage qui rend un son lisible.', 'sound-design'),
('ui-sound-design',            'Sons d''interface',                    'Court, discret, cohérent, non fatigant à la centième écoute.', 'sound-design'),
('ambience-design',            'Ambiances et nappes',                 'Un lieu qui existe à l''oreille : fond, événements épars, mouvement lent.', 'sound-design'),
('sound-pack-consistency',     'Cohérence d''un pack',                 'Vingt sons qui appartiennent visiblement au même monde.', 'sound-design'),
('sfx-naming-and-delivery',    'Nommage et livraison de bruitages',   'Convention de noms, formats, feuille d''usage. Un pack qu''un intégrateur peut utiliser sans poser de question.', 'sound-design'),

-- ── Interprétation vocale ──────────────────────────────────────────
('character-voice-creation',   'Création de voix de personnage',      'Construire une voix tenable sur des heures d''enregistrement sans se blesser.', 'voice-performance'),
('narration-delivery',         'Narration',                           'Livre audio, documentaire, tutoriel. Le rythme qui tient sur trente minutes.', 'voice-performance'),
('commercial-read',            'Lecture commerciale',                 'Trente secondes, un ton de marque, une intention claire.', 'voice-performance'),
('vocal-range-demonstration',  'Démonstration de registre',           'Une bande démo qui montre l''étendue réelle plutôt que le meilleur passage.', 'voice-performance'),
('accent-and-language-work',   'Accents et langues',                  'Jouer dans une langue ou un accent sans le trahir.', 'voice-performance'),
('home-studio-voice-capture',  'Prise de voix en home studio',        'Traitement de pièce, distance au micro, bruit de fond. Ce qui distingue une prise utilisable d''une prise à refaire.', 'voice-performance'),
('voice-direction',            'Direction d''acteur voix',             'Diriger une session : indication utile, alternative demandée, prise gardée.', 'voice-performance'),
('dialogue-editing',           'Montage de dialogue',                 'Respirations, claquements de bouche, alternance de prises. iZotope RX et la patience.', 'voice-performance'),

-- ── Audio de jeu et intégration moteur ─────────────────────────────
('fmod-integration',           'Intégration FMOD',                     'Événements, paramètres, instruments multi-son. Le projet et son branchement au moteur.', 'game-audio'),
('wwise-integration',          'Intégration Wwise',                    'Switches, states, RTPC, SoundBanks. Et la mémoire que tout cela coûte.', 'game-audio'),
('engine-native-audio',        'Audio natif moteur',                   'Godot, Bevy, Unity sans middleware. Ce qu''on peut faire sans dépendance externe.', 'game-audio'),
('vertical-remixing',          'Remixage vertical',                    'Des couches qui s''activent et se coupent sans que la mesure se casse.', 'game-audio'),
('horizontal-resequencing',    'Reséquencement horizontal',            'Des segments qui s''enchaînent selon l''état du jeu, avec des points de sortie musicaux.', 'game-audio'),
('audio-memory-budget',        'Budget mémoire et streaming',          'Ce qui est chargé, ce qui est diffusé, ce qui est compressé et à quel prix.', 'game-audio'),
('audio-middleware-debugging', 'Débogage audio en jeu',                'Profilage des voix, sons coupés, priorité, virtualisation.', 'game-audio'),

-- ── Programmation audio ────────────────────────────────────────────
('dsp-filter-design',          'Conception de filtres DSP',            'Biquads, retards, convolution. Écrire un traitement plutôt que le brancher.', 'audio-programming'),
('realtime-audio-constraints', 'Contraintes du temps réel',            'Pas d''allocation ni de verrou dans le thread audio. La règle qui définit le métier.', 'audio-programming'),
('spatial-audio-hrtf',         'Audio spatial et HRTF',                'Binaural, ambisonique, occlusion. Faire venir un son d''un endroit.', 'audio-programming'),
('procedural-audio',           'Audio procédural',                     'Synthétiser le son à l''exécution plutôt que le lire. Moteurs, vent, pas.', 'audio-programming'),
('audio-plugin-development',   'Développement de greffons',            'VST3, CLAP, AU. JUCE ou à la main.', 'audio-programming'),
('audio-engine-bindings',      'Liaisons de moteur audio',             'Envelopper FMOD, Wwise, cpal ou miniaudio proprement depuis Rust ou C++.', 'audio-programming'),
('audio-latency-profiling',    'Profilage de latence',                 'Taille de tampon, sous-alimentations, chemin d''entrée-sortie. Mesuré, pas ressenti.', 'audio-programming'),

-- ── Prise, mixage et mastering ─────────────────────────────────────
('microphone-technique',       'Technique de prise',                   'Choix et placement de micro, distance, axe. La moitié du résultat se joue là.', 'audio-post-production'),
('room-treatment',             'Traitement acoustique',                'Réflexions précoces, résonances. Ce qu''on peut corriger avant d''enregistrer et ce qu''on ne rattrapera pas après.', 'audio-post-production'),
('mixing-balance',             'Équilibre de mixage',                  'Niveaux, panoramique, espace fréquentiel. Que chaque élément ait sa place.', 'audio-post-production'),
('loudness-standards',         'Normes de loudness',                   'LUFS, crête vraie, plage dynamique. Livrer à la norme de la plateforme visée.', 'audio-post-production'),
('stem-delivery',              'Livraison en stems',                   'Découper une pièce en pistes séparées pour que le client puisse remixer sans revenir vers le compositeur.', 'audio-post-production'),
('audio-restoration',          'Restauration',                         'Débruitage, dé-clic, dé-réverbération. Sauver une prise plutôt que la refaire.', 'audio-post-production'),
('format-and-encoding',        'Formats et encodage',                  'WAV, FLAC, OGG, MP3. Fréquence, profondeur, débit, et ce que chaque choix coûte.', 'audio-post-production'),

-- ── Droits, licences et attribution ────────────────────────────────
('sample-licence-hygiene',     'Hygiène des licences d''échantillons', 'Savoir d''où vient chaque échantillon et sous quelle licence. Un pack non tracé rend tout le morceau inutilisable.', 'audio-rights'),
('creative-commons-attribution','Attribution Creative Commons',        'BY, BY-SA, BY-NC. Créditer correctement, et savoir ce qui est interdit malgré la gratuité.', 'audio-rights'),
('sync-licensing',             'Licences de synchronisation',          'Autoriser un usage à l''image sans céder l''œuvre. Étendue, durée, exclusivité.', 'audio-rights'),
('royalties-registration',     'Dépôt et droits d''auteur',             'SACEM, ASCAP, BMI. Ce que le dépôt protège et ce qu''il ne protège pas.', 'audio-rights'),
('voice-consent-and-rights',   'Consentement et droits de la voix',    'La voix est un attribut de la personne. Ce qu''un contrat doit dire avant l''enregistrement.', 'audio-rights')

) AS c(slug, display_name, description, parent_slug)
JOIN skill_nodes p ON p.slug = c.parent_slug;
