-- What a reviewer looks at, per family of design trade.
--
-- Migration 0180 built the table and gave `code` its grids; 0211 gave `ai`
-- its own. Until this one, a design challenge with no hand-written rubric
-- reached the reviewer with the brief and nothing else.
--
-- ## Why design needs this more than code does
--
-- "It looks off" is the failure mode this table exists to prevent. A code
-- review that says nothing useful still leaves a diff and a test suite; a
-- design review that says nothing useful leaves a designer guessing which of
-- forty decisions was the wrong one. Named criteria are what turn a verdict
-- into something the person can act on, and what let two reviewers reach
-- comparable conclusions on the same work.
--
-- ## The common criteria are not the code ones
--
-- Correctness is not the first question. A layout is not correct or
-- incorrect; the questions that decide whether the work is worth anything are
-- whether it answers the brief, whether a stranger can read it, and whether
-- the designer can say why. Those sit at the top of the domain grid.
--
-- ## Accessibility is in the common grid, not in a specialist one
--
-- Putting contrast and focus states in a "product" grid would say they are a
-- product designer's concern. They are everyone's: an unreadable chart, an
-- animation that triggers vertigo and a caption nobody can hear are the same
-- failure. It costs one criterion in the default grid and closes the excuse.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('design', NULL, 'Design — critères communs', '[
  {"criterion": "Réponse au brief", "looks_like": "Chaque contrainte écrite dans le brief est traitée, et les écarts assumés sont expliqués. Un travail beau qui répond à une autre question est refusé."},
  {"criterion": "Hiérarchie", "looks_like": "Un lecteur qui découvre l''écran sait en trois secondes ce qui est important. Rien n''est mis en avant par défaut."},
  {"criterion": "Accessibilité", "looks_like": "Contraste, tailles, états de focus, alternatives non visuelles. Ce qui n''est pas lisible par tout le monde n''est pas fini."},
  {"criterion": "Justification", "looks_like": "L''auteur peut dire pourquoi ce choix plutôt qu''un autre, en termes qu''un non-designer peut évaluer. \"Je trouvais ça mieux\" n''est pas une justification."},
  {"criterion": "Exécution", "looks_like": "Alignements, espacements et détails tiennent au zoom. Le soin visible est ce qui distingue une proposition d''une esquisse."},
  {"criterion": "Droits sur les ressources", "looks_like": "Polices, images et sons utilisés sont licenciés pour cet usage. Un livrable public avec une police non licenciée est un risque juridique pour son auteur."},
  {"criterion": "Transparence sur l''IA", "looks_like": "L''usage d''un outil génératif est déclaré. Il est accepté ; le camoufler ne l''est pas."}
]'),

('design', 'product', 'Produit — grille de revue', '[
  {"criterion": "Parcours", "looks_like": "Le chemin principal se fait sans détour, et les chemins d''erreur existent. Un flux qui ne prévoit que le cas nominal n''a pas été conçu."},
  {"criterion": "Cohérence système", "looks_like": "Composants, tokens et espacements viennent du système existant. Une exception est justifiée, pas subie."},
  {"criterion": "États", "looks_like": "Vide, chargement, erreur, trop de données. Les quatre sont dessinés, pas laissés au développement."},
  {"criterion": "Responsive", "looks_like": "Le comportement entre les tailles est décidé, pas déduit. Les points de rupture sont montrés."},
  {"criterion": "Micro-interactions", "looks_like": "Chaque action donne un retour immédiat et proportionné. Un bouton qui ne dit rien pendant deux secondes est cassé."},
  {"criterion": "Passage au développement", "looks_like": "Un développeur trouve les valeurs sans demander : espacements, tokens, comportements. La maquette se lit toute seule."}
]'),

('design', 'web', 'Web — grille de revue', '[
  {"criterion": "Hiérarchie de conversion", "looks_like": "Une action principale par écran, visible sans défilement sur mobile. Trois appels à l''action de même poids n''en font aucun."},
  {"criterion": "Rythme de lecture", "looks_like": "Longueur de ligne, interlignage et respiration tiennent sur un long texte. Un mur de texte n''est pas lu."},
  {"criterion": "Budget de performance", "looks_like": "Poids des images, nombre de polices et médias au-dessus de la ligne de flottaison sont connus. Une page magnifique à 8 Mo ne s''ouvre pas ici."},
  {"criterion": "Contraintes de la plateforme", "looks_like": "Ce qui est réalisable dans le CMS ou l''outil no-code visé. Une maquette impossible à intégrer n''est pas livrée."},
  {"criterion": "Structure", "looks_like": "Titres hiérarchisés, texte alternatif prévu, ordre de lecture cohérent. Le référencement et le lecteur d''écran lisent la même chose."},
  {"criterion": "Cohérence de gabarit", "looks_like": "Les pages d''un même type se ressemblent. Un site est un système, pas une collection d''affiches."}
]'),

('design', 'mobile', 'Mobile — grille de revue', '[
  {"criterion": "Conventions de plateforme", "looks_like": "Navigation, retour arrière et gestes suivent iOS ou Material. S''en écarter demande une raison écrite."},
  {"criterion": "Zones d''atteinte", "looks_like": "Les cibles font au moins 44 points et tombent sous le pouce. Une action fréquente en haut de l''écran est une erreur."},
  {"criterion": "Découpe de l''écran", "looks_like": "Encoches, barres système et pliables sont pris en compte. Le contenu ne passe pas sous une barre."},
  {"criterion": "Hors ligne et bas débit", "looks_like": "Ce que l''écran affiche quand le réseau tombe est dessiné. C''est le cas courant sur les marchés visés, pas un cas limite."},
  {"criterion": "Poids des ressources", "looks_like": "Images et animations dimensionnées pour un appareil d''entrée de gamme, pas pour un téléphone de démonstration."},
  {"criterion": "États", "looks_like": "Vide, chargement, erreur, permission refusée. Une permission refusée sans écran prévu bloque l''application."}
]'),

('design', 'motion', 'Motion — grille de revue', '[
  {"criterion": "Rythme", "looks_like": "Les durées servent l''émotion visée. Une transition d''interface au-delà de 400 ms se sent comme une lenteur."},
  {"criterion": "Courbes", "looks_like": "Les accélérations correspondent à une physique plausible. Le linéaire se remarque et se remarque mal."},
  {"criterion": "Intention", "looks_like": "Le mouvement dit quelque chose — d''où ça vient, où ça va, ce qui a changé. Un mouvement décoratif est du bruit."},
  {"criterion": "Poids et performance", "looks_like": "Taille de fichier, nombre de calques et fluidité mesurés sur la cible. Une animation qui saccade est un défaut, pas un détail."},
  {"criterion": "Boucle et raccords", "looks_like": "Si ça boucle, la reprise est invisible. Si ça s''enchaîne, le raccord est pensé."},
  {"criterion": "Respect du mouvement réduit", "looks_like": "Une alternative existe quand le système demande moins de mouvement. Ignorer ce réglage rend le produit inutilisable pour certains."}
]'),

('design', 'brand', 'Marque — grille de revue', '[
  {"criterion": "Distinction", "looks_like": "Posé à côté de trois concurrents, on reconnaît lequel c''est. Une identité interchangeable n''en est pas une."},
  {"criterion": "Passage à l''échelle", "looks_like": "Lisible en favicon et tenue en très grand. Les deux extrêmes sont montrés, pas supposés."},
  {"criterion": "Adéquation", "looks_like": "Ce que la marque prétend être et ce que le système visuel raconte disent la même chose."},
  {"criterion": "Applicabilité", "looks_like": "Décliné sur les supports promis au brief, y compris les ingrats : impression une couleur, fond sombre, broderie."},
  {"criterion": "Exécution", "looks_like": "Courbes, approche et alignements optiques tiennent l''agrandissement. Un logo se juge à 400 %."},
  {"criterion": "Transmissibilité", "looks_like": "Quelqu''un d''autre peut appliquer l''identité à partir des guidelines, sans redemander à l''auteur."}
]'),

('design', 'illustration', 'Illustration — grille de revue', '[
  {"criterion": "Lecture", "looks_like": "La silhouette et la composition se lisent en vignette. Ce qui n''existe qu''au zoom n''existe pas."},
  {"criterion": "Tenue du style", "looks_like": "Sur une série, le trait, la palette et le niveau de détail restent les mêmes. Une image qui dépareille casse le lot."},
  {"criterion": "Justesse du propos", "looks_like": "L''image dit ce que le texte ne dit pas, sans le contredire. Une illustration qui redit le titre ne sert à rien."},
  {"criterion": "Lumière et couleur", "looks_like": "Les valeurs tiennent en niveaux de gris. Une couleur qui compense une valeur ratée ne tient pas à l''impression."},
  {"criterion": "Livraison", "looks_like": "Formats, résolutions et sources fournis pour l''usage prévu. Un PNG seul n''est pas un livrable."},
  {"criterion": "Référence et emprunt", "looks_like": "Les références sont assumées et transformées. La ressemblance à une œuvre existante est signalée avant que quelqu''un d''autre la trouve."}
]'),

('design', 'dataviz', 'Data visualisation — grille de revue', '[
  {"criterion": "Intégrité de l''encodage", "looks_like": "Axes non tronqués, surfaces proportionnelles, échelles honnêtes. Un graphique qui exagère est un mensonge, même joli."},
  {"criterion": "Choix de la forme", "looks_like": "La forme répond à la question posée. Un camembert à douze parts répond à une question que personne n''a posée."},
  {"criterion": "Densité", "looks_like": "Assez de données pour décider, assez peu pour lire. Le rapport encre/information est arbitré consciemment."},
  {"criterion": "Couleur", "looks_like": "Échelle adaptée à la nature de la donnée, et lisible en daltonisme. La couleur porte du sens, pas de la décoration."},
  {"criterion": "Étiquetage", "looks_like": "Unités, périodes et sources présentes sur le graphique lui-même. Une légende ailleurs se perd au partage."},
  {"criterion": "Ce que le lecteur doit retenir", "looks_like": "Le titre dit la conclusion, pas le sujet. \"Ventes par trimestre\" n''apprend rien."}
]'),

('design', 'ux-writing', 'UX writing — grille de revue', '[
  {"criterion": "Clarté", "looks_like": "Un mot par idée, pas de jargon interne. Ce qui se lit deux fois est à réécrire."},
  {"criterion": "Messages d''erreur", "looks_like": "Cause, conséquence, action. Un message qui dit \"une erreur est survenue\" laisse la personne bloquée."},
  {"criterion": "Ton", "looks_like": "Le même caractère d''un écran à l''autre, adapté au moment. On ne plaisante pas sur un écran de paiement refusé."},
  {"criterion": "Traduisibilité", "looks_like": "Pas de concaténation, pas d''idiome, pas de largeur fixe supposée. Le texte est écrit pour être traduit."},
  {"criterion": "Inclusion", "looks_like": "Ni genre présumé, ni référence qui exclut, ni métaphore intraduisible."},
  {"criterion": "Cohérence terminologique", "looks_like": "Un objet porte un seul nom dans tout le produit. Deux mots pour la même chose créent deux objets dans la tête du lecteur."}
]'),

('design', 'marketing', 'Marketing — grille de revue', '[
  {"criterion": "Une idée", "looks_like": "La campagne tient en une phrase, et chaque support la sert. Trois idées font zéro campagne."},
  {"criterion": "Déclinaison", "looks_like": "Le système tient sur tous les formats demandés, y compris les plus contraints. Un format sacrifié est un format perdu."},
  {"criterion": "Appel à l''action", "looks_like": "Une action, visible, à un endroit prévisible. Le reste du visuel y conduit."},
  {"criterion": "Respect de la marque", "looks_like": "Les guidelines existantes sont suivies, ou l''écart est argumenté auprès de qui les possède."},
  {"criterion": "Contraintes de diffusion", "looks_like": "Poids, formats et règles de la plateforme visée respectés. Une création refusée par la régie ne sert à rien."},
  {"criterion": "Testabilité", "looks_like": "Les variantes diffèrent sur une seule chose. Deux variantes qui changent tout ne mesurent rien."}
]'),

('design', 'game', 'Jeu — grille de revue', '[
  {"criterion": "Lisibilité sous charge", "looks_like": "L''information vitale se lit pendant l''action, pas à l''arrêt. Un HUD jugé sur une capture d''écran n''est pas jugé."},
  {"criterion": "Navigation", "looks_like": "Manette et clavier atteignent tout, dans un ordre prévisible. Une option accessible uniquement à la souris est inaccessible."},
  {"criterion": "Cohérence de l''univers", "looks_like": "L''interface appartient au monde qu''elle sert, ou s''en détache exprès."},
  {"criterion": "Contraintes du moteur", "looks_like": "Résolutions, atlas et budgets tenables dans Unity, Unreal ou Godot. Une maquette hors budget n''est pas intégrée."},
  {"criterion": "Retour au joueur", "looks_like": "Chaque action a une réponse visuelle ou sonore immédiate. La sensation est une fonctionnalité."},
  {"criterion": "Confort", "looks_like": "Taille de texte, options de contraste et de secousse d''écran prévues. Le confort n''est pas une option de fin de production."}
]'),

('design', '3d-viz', 'Visualisation 3D — grille de revue', '[
  {"criterion": "Cadrage", "looks_like": "La focale et le point de vue correspondent à un regard humain plausible. Un grand angle qui agrandit la pièce ment sur le projet."},
  {"criterion": "Lumière", "looks_like": "Une direction dominante crédible, une heure identifiable, des ombres qui tiennent. La lumière raconte le lieu."},
  {"criterion": "Matériaux", "looks_like": "Échelle des textures juste, usure et imperfections présentes. Le parfait absolu se lit comme du faux."},
  {"criterion": "Mise en scène", "looks_like": "Les objets racontent un usage sans encombrer. Une pièce vide ne se projette pas, une pièce saturée ne se lit pas."},
  {"criterion": "Post-production", "looks_like": "Étalonnage discret et cohérent sur toute la série. Un rendu retouché doit rester le même projet."},
  {"criterion": "Fidélité au projet", "looks_like": "Les proportions et les matériaux correspondent aux plans. Une image plus belle que le bâtiment est un problème, pas un service."}
]'),

('design', 'immersive', 'Immersif — grille de revue', '[
  {"criterion": "Confort", "looks_like": "Pas de mouvement de caméra imposé, cadence tenue, horizon stable. Une expérience qui donne la nausée est un échec technique."},
  {"criterion": "Affordances spatiales", "looks_like": "Ce qui est saisissable se voit sans explication. La profondeur est signalée par autre chose que la taille."},
  {"criterion": "Interaction", "looks_like": "Main, regard ou contrôleur : le geste attendu est apprenable en dix secondes et pardonne l''imprécision."},
  {"criterion": "Entrée et sortie", "looks_like": "Limites de sécurité, calibrage et sortie d''expérience prévus avant le contenu."},
  {"criterion": "Son", "looks_like": "La spatialisation aide à s''orienter. Un son non directionnel dans un espace 3D désoriente."},
  {"criterion": "Alternatives", "looks_like": "Une personne assise, malentendante ou à mobilité réduite peut faire l''expérience. Sinon, c''est écrit."}
]'),

('design', 'service', 'Service et process — grille de revue', '[
  {"criterion": "Complétude du blueprint", "looks_like": "Scène et coulisses, systèmes et personnes. Un blueprint qui s''arrête à l''écran décrit une interface, pas un service."},
  {"criterion": "Preuve terrain", "looks_like": "Les étapes viennent d''observations ou d''entretiens, pas d''un atelier entre collègues."},
  {"criterion": "Points de rupture", "looks_like": "Les endroits où le service casse sont identifiés et priorisés. Un parcours sans friction n''a pas été regardé."},
  {"criterion": "Faisabilité", "looks_like": "Les contraintes réelles — équipe, budget, système existant — sont dans le document."},
  {"criterion": "Mesure", "looks_like": "Ce qui dira que ça marche est décidé avant la mise en œuvre, et calculable."},
  {"criterion": "Transmissibilité", "looks_like": "Quelqu''un qui n''était pas à l''atelier peut agir à partir du livrable. Un post-it photographié n''est pas un livrable."}
]');
