-- A hundred and thirty challenges, five per design trade.
--
-- ## Why they are drafts
--
-- The title and the intent come from the backlog; the full brief — the
-- constraints, the references, what is out of scope — needs an author who
-- knows the trade. A challenge nobody has reviewed must not be offered to
-- somebody learning, and `draft` is the state the workflow already has.
--
-- Seeding them anyway is the point: twenty-six trades with an empty catalogue
-- are twenty-six trades the platform claims to support and cannot. A designer
-- who arrives on a motion 3D profile and finds nothing to do leaves, and no
-- amount of roadmap fixes that afterwards.
--
-- ## Why the instructions are built here rather than written out
--
-- The variable part is what to do and what artefact comes out. The three
-- headings and the closing paragraph are the same every time, and writing
-- them a hundred and thirty times would be a hundred and thirty chances to
-- let one drift.
--
-- ## The paragraph every design brief ends on
--
-- Two sentences, and they are the two most common reasons a design submission
-- comes back: the deliverable has to be openable by somebody who was not
-- there, and the author has to be able to say why. A design that cannot be
-- explained is not finished, whatever it looks like.
--
-- ## Language
--
-- NULL, and deliberately. `challenge_templates.language` means a programming
-- language; a brand identity has none, and writing "figma" there would put a
-- tool in a column that means something else.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## Ce qu''il y a à faire' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Ce qui est attendu' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'Dans tous les cas : le livrable s''ouvre par quelqu''un qui n''était pas ' ||
    'là — sources, formats et droits d''usage compris — et son auteur peut ' ||
    'dire pourquoi ces choix plutôt que d''autres, en des termes qu''un ' ||
    'non-designer peut évaluer. Un travail qu''on ne peut pas expliquer n''est ' ||
    'pas fini, quelle que soit son allure.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de revue de la famille s''applique, et elle est publique : ' ||
    'tu peux la lire avant de soumettre.',
    'design', c.difficulty, NULL,
    'draft', TRUE,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'design' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'design' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

-- ── design-product (5) ─────────────────────────────────────────────
('design-product', 'Refonte d''un parcours d''inscription',
 'Reprendre un parcours d''inscription existant en réduisant le nombre d''écrans sans perdre d''information nécessaire',
 'Les écrans avant/après, la liste des champs supprimés avec la raison, et un prototype cliquable.', 2),
('design-product', 'Les quatre états d''un écran',
 'Dessiner vide, chargement, erreur et surcharge pour un écran de liste réel, pas seulement le cas nominal',
 'Les quatre états, et une note sur ce que le développement doit déclencher pour chacun.', 2),
('design-product', 'Un test d''utilisabilité sur cinq personnes',
 'Faire tester un parcours existant par cinq personnes et transformer ce qu''on observe en décisions de conception',
 'Le protocole, les observations brutes, et les trois changements retenus avec leur justification.', 3),
('design-product', 'Une fonctionnalité de bout en bout',
 'Concevoir une fonctionnalité complète depuis le problème jusqu''au prototype, en énonçant ce qui est hors périmètre',
 'Le problème formulé, les parcours, les écrans, le prototype, et la liste de ce qui a été écarté.', 4),
('design-product', 'Reprendre un écran sur retours utilisateurs',
 'Partir de retours utilisateurs contradictoires et arbitrer entre eux plutôt que de tous les satisfaire',
 'Les retours, l''arbitrage écrit, et l''écran qui en découle.', 3),

-- ── design-system (5) ──────────────────────────────────────────────
('design-system', 'Une échelle de couleurs sémantique',
 'Construire une palette où chaque couleur porte un rôle nommé, utilisable en clair et en sombre',
 'Les tokens au format W3C, la matrice de contraste, et un écran de démonstration dans les deux thèmes.', 3),
('design-system', 'Un composant et toutes ses variantes',
 'Concevoir un composant réel avec ses tailles, ses états et ses cas limites, documenté pour être repris',
 'Le composant, ses variantes, et la page de documentation qui dit quand ne pas l''utiliser.', 3),
('design-system', 'Une échelle typographique',
 'Poser une échelle typographique qui tient du mobile au grand écran sans exception ponctuelle',
 'L''échelle, les tokens, et une page de démonstration à trois largeurs.', 2),
('design-system', 'Un audit d''incohérences',
 'Relever les écarts au système dans un produit existant et les classer par coût de correction',
 'Le relevé, le classement, et les trois corrections à faire en premier avec pourquoi.', 3),
('design-system', 'Un modèle de contribution',
 'Écrire comment une équipe extérieure propose un composant au système, et ce qui le fait accepter ou non',
 'Le document de contribution, et un exemple de proposition traitée de bout en bout.', 4),

-- ── design-ai-conversational (5) ───────────────────────────────────
('design-ai-conversational', 'Un flux de conversation et ses réparations',
 'Concevoir un échange assistant/utilisateur avec ses chemins de réparation quand le modèle se trompe',
 'Le flux nominal, les trois chemins de réparation, et ce que dit l''assistant quand il ne sait pas.', 3),
('design-ai-conversational', 'La persona d''un assistant',
 'Définir la voix d''un assistant produit : ce qu''il dit, ce qu''il ne dit jamais, comment il refuse',
 'Le document de persona, dix exemples de réponses, et cinq contre-exemples commentés.', 3),
('design-ai-conversational', 'Afficher l''incertitude',
 'Concevoir comment une interface montre qu''une réponse est incertaine sans la rendre inutilisable',
 'Les états d''incertitude, et le test qui montre qu''un lecteur les comprend.', 4),
('design-ai-conversational', 'Consentement et transparence',
 'Concevoir les surfaces où l''utilisateur apprend qu''une IA intervient et peut s''y opposer',
 'Les écrans de consentement, le réglage de retrait, et ce que devient la donnée déjà envoyée.', 3),
('design-ai-conversational', 'Une interface vocale courte',
 'Concevoir un échange vocal avec interruption, confirmation et sortie, pour une tâche unique',
 'Le script, les temporisations, et ce qui se passe quand la reconnaissance échoue deux fois.', 4),

-- ── design-web (5) ─────────────────────────────────────────────────
('design-web', 'Une page d''atterrissage qui convertit',
 'Concevoir une page dont l''action principale est évidente sans défilement sur mobile',
 'La page aux trois largeurs, la hiérarchie justifiée, et le budget de poids tenu.', 2),
('design-web', 'Une fiche produit e-commerce',
 'Concevoir une fiche produit qui répond aux objections d''achat avant qu''elles n''arrivent',
 'La fiche, la liste des objections traitées, et les états de rupture de stock.', 3),
('design-web', 'Un tunnel de commande',
 'Concevoir un tunnel où la friction restante est délibérée et justifiée',
 'Les étapes, ce qui a été retiré, et les écrans d''échec de paiement.', 4),
('design-web', 'Une refonte sous contrainte CMS',
 'Reprendre un site en restant intégralement réalisable dans un CMS ou un outil no-code visé',
 'Les maquettes, et la note qui dit comment chaque bloc est construit dans l''outil.', 3),
('design-web', 'Un budget de performance tenu',
 'Concevoir une page riche visuellement qui reste sous un budget de poids annoncé',
 'La page, le tableau des poids par ressource, et ce qui a été sacrifié.', 3),

-- ── design-editorial-web (5) ───────────────────────────────────────
('design-editorial-web', 'Un article long lisible',
 'Composer un article de six mille signes dont le rythme de lecture tient jusqu''au bout',
 'L''article composé, la mesure de ligne justifiée, et la version mobile.', 2),
('design-editorial-web', 'Une grille éditoriale',
 'Poser une grille qui accueille texte, images pleine largeur, citations et encadrés sans exception',
 'La grille, et cinq gabarits d''article qui l''utilisent différemment.', 3),
('design-editorial-web', 'Un récit au défilement',
 'Concevoir une page dont le défilement porte le récit, sans en faire un obstacle',
 'La séquence, les repères de progression, et la version sans animation.', 4),
('design-editorial-web', 'Une page d''accueil de magazine',
 'Hiérarchiser vingt articles sur une page d''accueil sans que tout se ressemble',
 'La page, les règles de hiérarchie, et le comportement quand un article manque d''image.', 3),
('design-editorial-web', 'Intégrer l''illustration au texte',
 'Faire cohabiter illustration et texte long sans que l''une interrompe l''autre',
 'La composition, et les trois positions d''image retenues avec leur raison.', 3),

-- ── design-mobile (5) ──────────────────────────────────────────────
('design-mobile', 'Un écran natif sur les deux plateformes',
 'Concevoir le même écran en respectant iOS et Material sans le dessiner deux fois de zéro',
 'Les deux versions, et la liste de ce qui diffère avec la raison de chaque écart.', 3),
('design-mobile', 'Une navigation à une main',
 'Concevoir une navigation dont toutes les actions fréquentes tombent sous le pouce',
 'La navigation, la carte des zones d''atteinte, et ce qui a été déplacé.', 2),
('design-mobile', 'Un écran qui survit au réseau',
 'Concevoir ce que l''écran affiche hors ligne, en 3G intermittente, et à la reconnexion',
 'Les trois états, la stratégie de cache assumée, et ce qui est perdu quand ça coupe.', 4),
('design-mobile', 'Un parcours de permissions',
 'Demander une permission système au bon moment, et prévoir l''écran quand elle est refusée',
 'Le parcours, le moment choisi justifié, et l''application utilisable sans la permission.', 3),
('design-mobile', 'Une fiche de mise en avant',
 'Concevoir les visuels de présentation d''une application pour sa fiche de téléchargement',
 'Les visuels aux formats demandés, et la promesse qu''ils portent en une phrase.', 2),

-- ── design-motion-ui (5) ───────────────────────────────────────────
('design-motion-ui', 'Une transition entre deux écrans',
 'Animer un passage d''écran qui dit d''où vient le contenu, sous 400 millisecondes',
 'La transition en Lottie ou Rive, les courbes utilisées, et le poids du fichier.', 2),
('design-motion-ui', 'Un état de chargement qui rassure',
 'Concevoir une attente qui informe au lieu de tourner en rond',
 'L''animation, ses trois paliers de durée, et ce qui s''affiche au-delà de dix secondes.', 3),
('design-motion-ui', 'Un logo animé',
 'Animer une identité existante sans trahir sa construction',
 'Le logo animé, sa version courte, et la variante en mouvement réduit.', 3),
('design-motion-ui', 'Un système de micro-interactions',
 'Poser les règles de mouvement d''un produit : durées, courbes, ce qui bouge et ce qui ne bouge pas',
 'Le document de règles, et cinq interactions qui les appliquent.', 4),
('design-motion-ui', 'Respecter le mouvement réduit',
 'Reprendre une interface animée pour qu''elle reste compréhensible sans mouvement',
 'Les deux versions, et la règle qui dit quoi désactiver plutôt que quoi ralentir.', 3),

-- ── design-motion-2d (5) ───────────────────────────────────────────
('design-motion-2d', 'Une séquence narrative de trente secondes',
 'Raconter une idée en trente secondes d''animation 2D, de l''animatique au rendu',
 'L''animatique, le rendu final, et la note de rythme qui explique les temps forts.', 3),
('design-motion-2d', 'De la typographie cinétique',
 'Animer un texte court de façon à ce que le mouvement serve le sens et non l''inverse',
 'La séquence, et la justification de chaque effet par ce qu''il dit.', 3),
('design-motion-2d', 'Une boucle sans raccord',
 'Produire une boucle courte dont la reprise est invisible',
 'La boucle, et la démonstration image par image du raccord.', 2),
('design-motion-2d', 'Une explication animée',
 'Expliquer un mécanisme abstrait en animation, sans voix off',
 'La séquence, et le test qui montre qu''un lecteur non initié a compris.', 4),
('design-motion-2d', 'Une déclinaison multi-format',
 'Décliner une même séquence en horizontal, carré et vertical sans la refaire',
 'Les trois formats, et ce qui a été recomposé plutôt que recadré.', 3),

-- ── design-motion-3d (5) ───────────────────────────────────────────
('design-motion-3d', 'Une animation produit',
 'Mettre en scène un objet 3D en dix secondes, avec un éclairage qui le sert',
 'Le rendu, la scène source, et les réglages d''éclairage et de caméra.', 3),
('design-motion-3d', 'Un habillage en 3D',
 'Produire un habillage animé pour un contenu court, cohérent d''un plan à l''autre',
 'Les plans, la charte de mouvement, et les fichiers de projet.', 4),
('design-motion-3d', 'Une simulation maîtrisée',
 'Utiliser une simulation de particules ou de tissu au service d''une idée, pas pour la démonstration',
 'Le rendu, les paramètres, et le temps de calcul assumé.', 4),
('design-motion-3d', 'Une chaîne de rendu documentée',
 'Poser une chaîne de rendu reproductible : passes, débruitage, assemblage',
 'La chaîne écrite, les passes, et une image refaite à l''identique par un tiers.', 5),
('design-motion-3d', 'Un plan photoréaliste et sa version stylisée',
 'Produire le même plan en photoréaliste et en stylisé, et dire ce que chacun sert',
 'Les deux rendus, et la note qui les compare sur ce qu''ils racontent.', 4),

-- ── design-video (5) ───────────────────────────────────────────────
('design-video', 'Un montage narratif court',
 'Monter deux minutes de rushes en une histoire qui tient sans commentaire ajouté',
 'Le montage, la note d''intention, et la version sous-titrée.', 3),
('design-video', 'Un étalonnage cohérent',
 'Étalonner une séquence tournée dans des conditions inégales pour qu''elle paraisse d''un bloc',
 'La séquence avant/après, et la LUT ou les réglages utilisés.', 3),
('design-video', 'Un mixage propre',
 'Mixer voix, ambiance et musique en tenant une cible de niveau sonore annoncée',
 'La séquence mixée, la mesure de niveau, et le fichier de session.', 3),
('design-video', 'Un sous-titrage utilisable',
 'Sous-titrer une vidéo de façon lisible, y compris pour les sons qui ne sont pas de la parole',
 'Le fichier de sous-titres, la vidéo incrustée, et le choix de placement justifié.', 2),
('design-video', 'Une déclinaison verticale',
 'Reprendre une vidéo horizontale pour le format vertical sans perdre le sujet',
 'La version verticale, et ce qui a été recadré, recomposé ou refait.', 3),

-- ── design-brand-identity (5) ──────────────────────────────────────
('design-brand-identity', 'Un logotype qui tient aux deux extrêmes',
 'Concevoir un logotype lisible en favicon et tenu en très grand format',
 'Le logotype, ses versions de taille, et les tests aux deux extrêmes.', 3),
('design-brand-identity', 'Une identité complète pour une structure locale',
 'Livrer une identité utilisable par une association ou une coopérative, y compris en impression une couleur',
 'Les sources vectorielles, la palette, la typographie, et un document de guidelines court.', 4),
('design-brand-identity', 'Un système de déclinaison',
 'Poser les règles qui permettent à quelqu''un d''autre d''appliquer une identité sans redemander',
 'Les guidelines, et trois applications faites par une autre personne à partir d''elles.', 4),
('design-brand-identity', 'Une refonte qui garde la reconnaissance',
 'Moderniser une identité existante sans que ses usagers cessent de la reconnaître',
 'L''avant/après, la note de continuité, et le plan de transition.', 4),
('design-brand-identity', 'Une identité pour un support ingrat',
 'Concevoir une identité qui tient en broderie, en sérigraphie une couleur et sur fond sombre',
 'Les déclinaisons, et le test sur chaque support avec ses contraintes.', 3),

-- ── design-typography (5) ──────────────────────────────────────────
('design-typography', 'Un alphabet de base',
 'Dessiner un alphabet latin bas-de-casse cohérent, espacé et crénée',
 'Les glyphes, le fichier de production, et un texte composé de dix lignes.', 4),
('design-typography', 'Approche et crénage',
 'Reprendre l''espacement d''une police existante pour la rendre composable en texte courant',
 'Les paires corrigées, le avant/après en texte, et la méthode suivie.', 4),
('design-typography', 'Un axe variable',
 'Ajouter un axe de graisse à un caractère existant, avec des instances nommées',
 'Le fichier variable, les instances, et un spécimen qui montre l''axe.', 5),
('design-typography', 'Une couverture multi-écriture',
 'Étendre un caractère latin à une seconde écriture en tenant la cohérence de dessin',
 'Les glyphes ajoutés, et un texte bilingue composé où les deux tiennent ensemble.', 5),
('design-typography', 'Des fonctionnalités OpenType',
 'Ajouter ligatures et alternatives contextuelles à un caractère, et dire à quoi elles servent',
 'Le fichier, la liste des fonctionnalités, et un spécimen qui les démontre.', 4),

-- ── design-naming-verbal (5) ───────────────────────────────────────
('design-naming-verbal', 'Un nom et sa défense',
 'Proposer trois noms pour un produit réel et défendre celui qui est retenu',
 'Les trois pistes, la vérification de disponibilité, et l''argumentaire du nom retenu.', 3),
('design-naming-verbal', 'Un ton de voix documenté',
 'Définir la voix d''une marque : ce qu''elle dit, ce qu''elle évite, comment elle s''excuse',
 'Le document de ton, vingt exemples, et dix contre-exemples commentés.', 3),
('design-naming-verbal', 'Un récit de marque',
 'Écrire le récit d''une structure en une page, utilisable par ses équipes',
 'Le récit, sa version courte, et sa version d''une phrase.', 3),
('design-naming-verbal', 'Une accroche et ses variantes',
 'Écrire une accroche et ses déclinaisons par canal, sans qu''elle se dilue',
 'L''accroche, ses variantes, et la règle qui dit ce qui ne peut pas changer.', 2),
('design-naming-verbal', 'Un lexique produit',
 'Fixer le vocabulaire d''un produit : un objet, un mot, dans toute l''interface',
 'Le lexique, les synonymes bannis, et le relevé des endroits à corriger.', 3),

-- ── design-illustration (5) ────────────────────────────────────────
('design-illustration', 'Une illustration éditoriale',
 'Illustrer un article par une image qui dit ce que le texte ne dit pas',
 'L''illustration, les sources, et la note d''intention sur la métaphore choisie.', 3),
('design-illustration', 'Une série cohérente',
 'Produire cinq illustrations dont le style, la palette et le niveau de détail tiennent ensemble',
 'La série, les sources, et la règle de style écrite qui a permis de la tenir.', 3),
('design-illustration', 'Des états vides illustrés',
 'Illustrer les états vides d''un produit sans en faire des consolations',
 'Les illustrations, et le texte qui les accompagne.', 2),
('design-illustration', 'Une image explicative',
 'Expliquer un mécanisme par l''image seule, sans schéma technique',
 'L''illustration, et le test qui montre qu''un lecteur non initié a compris.', 4),
('design-illustration', 'Des valeurs qui tiennent en gris',
 'Produire une illustration dont la composition tient en niveaux de gris avant toute couleur',
 'La version en valeurs, la version couleur, et la comparaison des deux.', 3),

-- ── design-iconography (5) ─────────────────────────────────────────
('design-iconography', 'Un jeu de douze icônes',
 'Dessiner douze icônes sur une grille commune, avec une épaisseur de trait constante',
 'Le jeu en SVG optimisé, la grille, et un écran qui les utilise.', 3),
('design-iconography', 'Des métaphores lisibles ailleurs',
 'Choisir des métaphores d''icônes compréhensibles hors d''un seul contexte culturel',
 'Les icônes, les alternatives écartées, et le test de reconnaissance mené.', 3),
('design-iconography', 'Un jeu à trois tailles',
 'Décliner un jeu d''icônes en 16, 24 et 48 pixels sans le réduire mécaniquement',
 'Les trois tailles, et ce qui a été simplifié à la plus petite.', 4),
('design-iconography', 'Une livraison prête à intégrer',
 'Livrer un jeu d''icônes dans un format qu''un développement peut consommer sans retouche',
 'Le sprite ou la police d''icônes, la documentation d''usage, et un exemple d''intégration.', 3),
('design-iconography', 'Un audit d''équilibre optique',
 'Reprendre un jeu existant dont les icônes ne pèsent pas le même poids visuel',
 'Le relevé, les corrections, et la comparaison avant/après en contexte.', 3),

-- ── design-character (5) ───────────────────────────────────────────
('design-character', 'Une silhouette reconnaissable',
 'Concevoir un personnage identifiable à sa seule silhouette',
 'Les recherches de silhouette, le personnage retenu, et le test en ombre chinoise.', 3),
('design-character', 'Une planche d''expressions',
 'Produire les expressions d''un personnage sans qu''il cesse d''être lui-même',
 'La planche, et la note sur ce qui reste constant d''une expression à l''autre.', 3),
('design-character', 'Une tournette complète',
 'Produire une tournette exploitable par quelqu''un d''autre pour modéliser ou animer',
 'La tournette, les proportions cotées, et les détails vus de près.', 4),
('design-character', 'Une mascotte de marque',
 'Concevoir une mascotte qui porte une identité sans la caricaturer',
 'La mascotte, ses trois poses clés, et ses règles d''usage.', 3),
('design-character', 'Un personnage prêt à rigger',
 'Concevoir un personnage dont la construction permet une animation propre',
 'Le personnage, la note de contraintes de rig, et les articulations pensées.', 4),

-- ── design-dataviz (5) ─────────────────────────────────────────────
('design-dataviz', 'Le bon graphique pour la question',
 'Reprendre une visualisation existante dont la forme ne répond pas à la question posée',
 'L''avant/après, la question explicitée, et la raison du changement de forme.', 2),
('design-dataviz', 'Un tableau de bord de six indicateurs',
 'Concevoir un tableau de bord dense mais lisible, chaque chiffre avec sa définition',
 'Le tableau de bord, les définitions, et le titre qui dit la conclusion.', 3),
('design-dataviz', 'Une infographie narrative',
 'Raconter une série de données en une image qui se lit du haut vers le bas',
 'L''infographie, les sources, et la conclusion qu''elle porte en une phrase.', 3),
('design-dataviz', 'Une visualisation accessible',
 'Reprendre une visualisation pour qu''elle reste lisible en daltonisme et en niveaux de gris',
 'Les versions, l''échelle choisie, et le test de lisibilité mené.', 3),
('design-dataviz', 'Une exploration interactive',
 'Concevoir une visualisation explorable dont les états intermédiaires restent compréhensibles',
 'Le prototype, les états, et ce qui se passe quand un filtre ne renvoie rien.', 4),

-- ── design-ux-writing (5) ──────────────────────────────────────────
('design-ux-writing', 'Réécrire dix messages d''erreur',
 'Reprendre dix messages d''erreur réels pour qu''ils disent la cause, la conséquence et l''action',
 'L''avant/après, et la règle générale qui en ressort.', 2),
('design-ux-writing', 'Des états vides qui servent',
 'Écrire les états vides d''un produit pour qu''ils orientent au lieu de constater',
 'Les textes, et ce que chacun propose de faire ensuite.', 2),
('design-ux-writing', 'Un premier lancement',
 'Écrire les textes du premier lancement d''une application, sans tutoriel imposé',
 'Les textes, leur ordre, et ce qui a été retiré pour ne pas retarder l''usage.', 3),
('design-ux-writing', 'Un guide de style éditorial',
 'Écrire le guide qui permet à plusieurs personnes d''écrire le même produit',
 'Le guide, le lexique, et cinq cas d''arbitrage tranchés.', 4),
('design-ux-writing', 'Des textes qui survivent à la traduction',
 'Reprendre une interface pour que ses textes se traduisent sans casser la mise en page',
 'L''avant/après, les concaténations supprimées, et le test sur une langue plus longue.', 3),

-- ── design-marketing (5) ───────────────────────────────────────────
('design-marketing', 'Une campagne sur trois supports',
 'Décliner une même idée sur trois supports sans qu''elle se dilue',
 'Les trois déclinaisons, l''idée en une phrase, et ce qui reste constant.', 3),
('design-marketing', 'Un système de gabarits sociaux',
 'Poser des gabarits qu''une équipe non-design peut remplir sans casser la marque',
 'Les gabarits, les règles, et trois exemples remplis par quelqu''un d''autre.', 3),
('design-marketing', 'Un e-mail qui tient partout',
 'Concevoir un e-mail lisible dans les clients qui ne suivent pas les standards',
 'La maquette, le rendu dans trois clients, et ce qui a été simplifié.', 3),
('design-marketing', 'Une présentation qui se tient sans orateur',
 'Concevoir une présentation compréhensible par quelqu''un qui la lit seul',
 'La présentation, et la version commentée pour l''oral.', 2),
('design-marketing', 'Deux variantes testables',
 'Produire deux variantes d''une création qui ne diffèrent que sur une seule chose',
 'Les deux variantes, la variable isolée, et ce que le test mesurera.', 2),

-- ── design-game-ui (5) ─────────────────────────────────────────────
('design-game-ui', 'Un HUD lisible en action',
 'Concevoir un HUD dont l''information vitale se lit pendant le jeu, pas à l''arrêt',
 'Le HUD, une capture en pleine action, et ce qui disparaît quand rien ne se passe.', 3),
('design-game-ui', 'Un menu navigable à la manette',
 'Concevoir un menu où tout est atteignable à la manette, dans un ordre prévisible',
 'Le menu, la carte de navigation, et le comportement au retour arrière.', 3),
('design-game-ui', 'Une interface diégétique',
 'Intégrer une information de jeu dans le monde plutôt que par-dessus',
 'L''interface, sa version classique en comparaison, et ce que chacune coûte au joueur.', 4),
('design-game-ui', 'Des retours qui donnent de la sensation',
 'Concevoir les retours visuels d''une action répétée pour qu''elle reste satisfaisante',
 'Les retours, leur temporisation, et la version sans effets pour comparaison.', 3),
('design-game-ui', 'Des options de confort',
 'Concevoir les options d''accessibilité d''un jeu : taille de texte, contraste, secousses',
 'L''écran d''options, les réglages, et l''effet de chacun montré.', 3),

-- ── design-game-environment (5) ────────────────────────────────────
('design-game-environment', 'Des recherches de décor',
 'Explorer un décor en vignettes avant de choisir, et dire pourquoi celle-là',
 'Les vignettes, la piste retenue peinte, et la justification.', 3),
('design-game-environment', 'Un kit modulaire',
 'Concevoir un kit d''éléments qui se recombinent en plusieurs lieux crédibles',
 'Le kit, trois assemblages différents, et le budget de polygones tenu.', 4),
('design-game-environment', 'Une planche de matériaux',
 'Produire des matériaux répétables sans que la répétition se voie',
 'Les matériaux, une surface étendue rendue, et les réglages.', 4),
('design-game-environment', 'Un décor sous budget',
 'Produire un décor jouable dans un budget de polygones et de textures annoncé',
 'Le décor, le relevé de budget, et ce qui a été sacrifié pour tenir.', 4),
('design-game-environment', 'Un langage visuel de monde',
 'Poser les règles visuelles d''un univers, applicables par quelqu''un d''autre',
 'Le document, et deux décors faits par une autre personne à partir de lui.', 5),

-- ── design-arch-interior-viz (5) ───────────────────────────────────
('design-arch-interior-viz', 'Une vue extérieure crédible',
 'Produire une vue d''architecture avec un cadrage et une focale qui ne mentent pas sur les proportions',
 'Le rendu, la focale annoncée, et la comparaison avec les plans.', 3),
('design-arch-interior-viz', 'Un intérieur en lumière naturelle',
 'Éclairer un intérieur à une heure identifiable, sans lumière ajoutée invisible',
 'Le rendu, le réglage d''éclairage, et la version brute avant post-production.', 4),
('design-arch-interior-viz', 'Des matériaux avec leurs défauts',
 'Produire des matériaux crédibles en assumant usure et imperfections',
 'Les rendus de détail, et la comparaison avec des références réelles.', 4),
('design-arch-interior-viz', 'Une mise en scène habitée',
 'Meubler un espace pour qu''il se projette, sans le saturer',
 'Le rendu, et la note sur ce que chaque objet raconte de l''usage.', 3),
('design-arch-interior-viz', 'Un travelling architectural',
 'Produire un déplacement de caméra qui fait comprendre un espace en trente secondes',
 'La séquence, le tracé de caméra, et ce que chaque plan montre.', 5),

-- ── design-ar-vr-spatial (5) ───────────────────────────────────────
('design-ar-vr-spatial', 'Une interaction à la main',
 'Concevoir une prise en main d''objet virtuel apprenable en dix secondes',
 'Le prototype, la démonstration, et ce qui se passe quand le suivi décroche.', 4),
('design-ar-vr-spatial', 'Un confort mesuré',
 'Concevoir un déplacement en réalité virtuelle qui ne provoque pas de nausée',
 'Le prototype, les options de confort, et le retour de trois testeurs.', 4),
('design-ar-vr-spatial', 'Un ancrage en réalité augmentée',
 'Poser un contenu dans un espace réel de façon stable et compréhensible',
 'Le prototype, le comportement en cas de perte de suivi, et les repères d''échelle.', 4),
('design-ar-vr-spatial', 'De la typographie dans l''espace',
 'Rendre un texte lisible en trois dimensions, à distance variable',
 'Les réglages, les tests à trois distances, et la règle de taille retenue.', 3),
('design-ar-vr-spatial', 'Une entrée en expérience',
 'Concevoir le calibrage, les limites de sécurité et la sortie avant le contenu',
 'La séquence d''entrée, les limites, et la sortie d''urgence.', 3),

-- ── design-sound (5) ───────────────────────────────────────────────
('design-sound', 'Une palette de sons d''interface',
 'Concevoir les sons de confirmation, d''erreur et de notification d''un produit',
 'Les sons, leurs variantes de durée, et la règle qui dit quand ne rien jouer.', 3),
('design-sound', 'Une ambiance en couches',
 'Construire une ambiance qui tient plusieurs minutes sans lasser',
 'L''ambiance, ses couches séparées, et la boucle sans raccord audible.', 3),
('design-sound', 'Du bruitage enregistré',
 'Enregistrer et monter ses propres bruitages pour une séquence courte',
 'Les sons bruts, les sons montés, et la séquence sonorisée.', 3),
('design-sound', 'Une identité sonore',
 'Concevoir la signature sonore d''une marque, déclinable en trois durées',
 'Les trois durées, la note d''intention, et un usage en contexte.', 4),
('design-sound', 'Une livraison aux normes',
 'Livrer un ensemble sonore à une cible de niveau annoncée, avec ses alternatives non sonores',
 'Les fichiers, la mesure de niveau, et ce que voit quelqu''un qui n''entend pas.', 3),

-- ── design-service (5) ─────────────────────────────────────────────
('design-service', 'Un blueprint de service',
 'Cartographier un service existant, scène et coulisses, jusqu''aux systèmes',
 'Le blueprint, les sources d''observation, et les points de rupture identifiés.', 4),
('design-service', 'Une carte des parties prenantes',
 'Identifier qui décide, qui exécute et qui subit dans un service réel',
 'La carte, les entretiens qui la fondent, et les tensions relevées.', 3),
('design-service', 'Un parcours multi-canal',
 'Suivre un usager d''un canal à l''autre et relever où le service perd le fil',
 'Le parcours, les ruptures, et les trois corrections prioritaires.', 3),
('design-service', 'Un atelier de co-conception',
 'Animer un atelier avec des personnes concernées et en tirer des décisions, pas des post-it',
 'Le protocole, la matière produite, et les décisions retenues avec leur porteur.', 4),
('design-service', 'Un prototype de service',
 'Tester une idée de service par le jeu de rôle avant de rien construire',
 'Le protocole, ce qui a été observé, et ce que le test a fait abandonner.', 4),

-- ── design-ops (5) ─────────────────────────────────────────────────
('design-ops', 'Une convention de fichiers',
 'Poser une organisation de fichiers qu''une équipe peut tenir sans y penser',
 'La convention, un fichier réel réorganisé, et ce qui a été supprimé.', 2),
('design-ops', 'Un passage au développement',
 'Décrire le passage design/développement de façon à ce qu''aucune valeur ne se redemande',
 'Le processus, un exemple complet, et la liste de ce qui est fourni à chaque fois.', 3),
('design-ops', 'Un rituel de revue',
 'Poser un rituel de critique régulier qui produit des décisions et pas des avis',
 'Le format, le compte rendu de trois séances, et ce qui a changé grâce à elles.', 3),
('design-ops', 'Des droits d''usage au clair',
 'Auditer les polices, images et sons d''un produit et statuer sur chaque licence',
 'L''inventaire, les risques identifiés, et les remplacements proposés.', 3),
('design-ops', 'Mesurer un effet de conception',
 'Instrumenter une décision de conception pour savoir si elle a produit ce qu''on attendait',
 'La décision, la mesure posée avant, et la lecture du résultat.', 4)

) AS c(orientation_slug, title, description, expected, difficulty)
JOIN orientations o ON o.slug = c.orientation_slug
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Every trade got its set
-- ═══════════════════════════════════════════════════════════════════
--
-- The JOIN above drops silently on a mistyped slug, and a trade whose
-- catalogue is empty is exactly the failure this migration exists to prevent.

DO $$
DECLARE
    seeded INT;
BEGIN
    SELECT count(*) INTO seeded
      FROM challenge_templates
     WHERE skill_domain = 'design' AND is_training = TRUE AND status = 'draft';

    IF seeded < 130 THEN
        RAISE EXCEPTION
            'expected 130 design challenge drafts, found % — a mistyped '
            'orientation slug drops its whole set silently', seeded;
    END IF;
END $$;
