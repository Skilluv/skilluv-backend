-- What a reviewer listens for, per family of audio trade.
--
-- Migration 0180 built the table; 0211 gave `ai` its grids and stated the
-- rule: a domain with no default sends work to the verifier with the
-- instructions alone — the model is asked whether the work is good with no
-- statement of what good means, and answers anyway.
--
-- ## The common criteria are not the code ones, and not the AI ones either
--
-- Audio is judged first on *fitness*: a sound is not good or bad on its own,
-- it is right or wrong for the thing it is attached to. A beautiful pad that
-- buries the dialogue is a failure, and a three-note sting that nobody
-- notices can be the best work in the delivery. That is why "service du
-- propos" sits at the top rather than craft.
--
-- Second on *technique that can be measured*: loudness, headroom, noise
-- floor, sample rate. These are the reasons a delivery gets sent back, they
-- are numbers rather than opinions, and putting them in the grid means the
-- author checks them before a reviewer has to.
--
-- Third on *provenance*, which is unique to this domain in one respect: an
-- untraced sample or a voice recorded without a written consent does not make
-- the work weaker, it makes it unusable, and no amount of craft repairs it.
--
-- ## The one about AI is not the same sentence as elsewhere
--
-- Every other domain's grid says an assistant is accepted and hiding it is
-- not. That still holds, and audio adds the part that is specific to it:
-- generated *voice* is a separate question from generated code, because the
-- voice belongs to somebody. The criterion names both.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('audio', NULL, 'Audio — critères communs', '[
  {"criterion": "Service du propos", "looks_like": "Le son sert ce à quoi il est attaché — le jeu, l''image, l''interface, le récit. Un travail qu''on remarque au détriment de ce qu''il accompagne a raté sa cible, même bien fait."},
  {"criterion": "Qualité technique de la prise", "looks_like": "Pas de saturation, pas de souffle de fond audible, pas de résonance de pièce non voulue. Une prise qu''il faut réparer au montage est une prise à refaire."},
  {"criterion": "Mixage et niveaux", "looks_like": "Loudness mesuré (LUFS) et adapté à la destination, crête vraie sous le plafond, marge suffisante pour ce qui viendra se poser dessus."},
  {"criterion": "Cohérence de l''ensemble", "looks_like": "Les pièces d''une même livraison appartiennent au même monde : même traitement, même espace, même vocabulaire sonore."},
  {"criterion": "Provenance et licences", "looks_like": "Chaque échantillon, boucle ou banque utilisée est déclaré avec sa licence, ou tout est original et c''est écrit. Une source non tracée rend la livraison inutilisable, quel que soit le reste."},
  {"criterion": "Documentation de livraison", "looks_like": "Formats, fréquence, profondeur, nommage, et comment utiliser ce qui est livré. Un lecteur sait quoi faire du dossier sans écrire."},
  {"criterion": "Transparence sur l''IA", "looks_like": "L''usage d''un outil génératif est déclaré — musique, bruitage ou voix. Il est accepté à l''apprentissage ; une voix clonée sans le consentement écrit de la personne ne l''est jamais."}
]'),

('audio', 'composition', 'Composition — grille de revue', '[
  {"criterion": "Adéquation au brief", "looks_like": "L''ambiance, le genre, la durée et l''instrumentation demandés sont ceux livrés. Un très bon morceau hors brief est hors brief."},
  {"criterion": "Écriture", "looks_like": "Un thème identifiable, une harmonie qui tient, une forme qui va quelque part. On peut fredonner l''idée après une écoute."},
  {"criterion": "Développement", "looks_like": "Le matériau est varié plutôt que répété : une même identité tenue sur plusieurs morceaux ou plusieurs sections."},
  {"criterion": "Mixage et mastering", "looks_like": "Équilibre fréquentiel, plage dynamique conservée, loudness à la norme de la destination. Pas de compression qui écrase pour paraître fort."},
  {"criterion": "Bouclage et découpe", "looks_like": "Quand une boucle est demandée, le raccord est inaudible. Les points de sortie et d''entrée sont musicaux."},
  {"criterion": "Stems", "looks_like": "Les pistes séparées sont fournies, nommées, alignées et exploitables sans le projet d''origine."}
]'),

('audio', 'sound-design', 'Design sonore — grille de revue', '[
  {"criterion": "Intention", "looks_like": "Chaque son a une fonction énoncée : informer, récompenser, alerter, situer. Un son décoratif dans un pack fonctionnel est un défaut."},
  {"criterion": "Lisibilité", "looks_like": "Le son reste identifiable dans le mixage réel, superposé aux autres, pas seulement dans le silence."},
  {"criterion": "Fabrication", "looks_like": "Couches, montage, traitement. On comprend comment le son est construit et pourquoi il tient."},
  {"criterion": "Cohérence du pack", "looks_like": "Vingt sons d''un même ensemble partagent un espace, un grain et une famille de timbres. Un intrus s''entend."},
  {"criterion": "Fatigue à la répétition", "looks_like": "Un son d''interface joué cent fois par session reste supportable. Variations ou aléa quand c''est nécessaire."},
  {"criterion": "Livraison", "looks_like": "Nommage systématique, formats adaptés à la plateforme visée, feuille d''usage. Un intégrateur travaille sans poser de question."}
]'),

('audio', 'voice', 'Voix — grille de revue', '[
  {"criterion": "Crédibilité du personnage", "looks_like": "La voix appartient à quelqu''un : une intention, un âge, une origine, une situation. Pas un effet de voix tenu trente secondes."},
  {"criterion": "Registre démontré", "looks_like": "La bande montre une étendue réelle et distincte — pas la même voix cinq fois avec des hauteurs différentes."},
  {"criterion": "Diction et rythme", "looks_like": "Intelligible sans effort, phrasé qui respire, accentuation qui sert le sens. Tenu sur toute la durée, pas seulement au début."},
  {"criterion": "Qualité de la prise", "looks_like": "Pas d''écrêtage, plancher de bruit bas, pièce neutre, claquements de bouche et respirations traités. La prise est utilisable telle quelle."},
  {"criterion": "Respect de la direction", "looks_like": "Les indications du brief sont suivies, et des alternatives sont proposées quand elles ont un sens."},
  {"criterion": "Droits et consentement", "looks_like": "L''usage autorisé est écrit : support, territoire, durée, exclusivité. La voix est un attribut de la personne, pas un fichier."}
]'),

('audio', 'implementation', 'Intégration et programmation — grille de revue', '[
  {"criterion": "Justesse à l''exécution", "looks_like": "Le son se déclenche au bon moment, au bon endroit, dans le bon état. Vérifié dans une build, pas dans l''éditeur."},
  {"criterion": "Transitions", "looks_like": "Les passages entre états musicaux ou entre ambiances ne cassent ni la mesure ni l''illusion. Les cas limites — bascule rapide, aller-retour — sont testés."},
  {"criterion": "Budget", "looks_like": "Voix simultanées, mémoire, streaming et charge CPU mesurés sur la cible, pas sur la machine de développement."},
  {"criterion": "Propreté de l''intégration", "looks_like": "Les paramètres exposés sont nommés et documentés. Un développeur qui n''est pas l''auteur peut brancher un nouvel événement sans deviner."},
  {"criterion": "Robustesse", "looks_like": "Ce qui se passe quand la banque manque, quand le fichier est absent, quand tout se déclenche en même temps. Dégradation choisie plutôt que silence ou saturation."},
  {"criterion": "Réutilisabilité", "looks_like": "Le système sert au prochain projet : dépendances explicites, licence claire, exemple minimal qui tourne."}
]');
