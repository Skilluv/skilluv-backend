-- The audio guides, toolkit, briefs and writeup templates.
--
-- Migration 0199 built `content_guides` and said why these are rows: they have
-- to be translated and edited by somebody who is not deploying.
--
-- ## A fourth kind
--
-- `brief_template` joins onboarding, toolkit and writeup template. A brief is
-- written by the person *commissioning* the work, before it starts, and the
-- other three are written by or for the person doing it, during or after. That
-- is a different reader and a different moment, and filing briefs under
-- `writeup_template` would put them in the wrong list on the wrong page.
--
-- The distinction earns its keep immediately: the five briefs below are the
-- first thing to hand a company that has never commissioned audio and does not
-- know that "we need some music" is not a brief.
--
-- ## Four onboarding guides, not five
--
-- One per reviewer family, as 0199 established — the music implementer and the
-- audio programmer share `implementation` and share a guide, because what a
-- newcomer needs to know first is the same for both: which middleware, what a
-- memory budget is, and that everything is verified in a build.

ALTER TABLE content_guides DROP CONSTRAINT IF EXISTS content_guides_kind_check;
ALTER TABLE content_guides
    ADD CONSTRAINT content_guides_kind_check CHECK (kind IN (
        'onboarding',
        'toolkit',
        'writeup_template',
        -- Written by whoever commissions the work, before it starts.
        'brief_template'
    ));

-- ═══════════════════════════════════════════════════════════════════
-- Onboarding — one per family
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-audio-composition', 'onboarding', 'audio', 'composition', 'fr',
 'Débuter en composition',
 'Écrire de la musique pour autre chose que soi : ce que le brief impose, ce qu''il laisse, et par où commencer.',
$md$
# Débuter en composition

Composer pour un projet, ce n'est pas composer puis chercher un projet. La
contrainte arrive en premier — une durée, une ambiance, une scène, un budget
d'instruments — et le métier consiste à faire quelque chose de vivant à
l'intérieur.

## Les trente premiers jours

1. **Le premier morceau court.** Le générique de podcast ou le jingle : quinze
   secondes forcent à décider tout de suite ce que la musique dit.
2. **La boucle.** Trois minutes qui rebouclent sans qu'on l'entende. C'est un
   exercice technique autant que musical, et il t'apprend la forme.
3. **La variation.** Reprendre ton propre thème et en faire une deuxième
   version qui a la même identité et un autre rôle. C'est le geste que tout le
   reste du métier utilise.
4. **La mise à l'image.** Écrire sur un montage imposé.

## Ce qui fait revenir un travail

Trois choses, dans cet ordre de fréquence :

- **les sources ne sont pas déclarées.** Une boucle achetée, une banque, un
  échantillon Freesound : chacun se déclare avec sa licence. Une seule source
  non tracée rend la pièce inutilisable pour un client, quel que soit le reste.
- **le niveau n'est pas mesuré.** Écris ce que tu vises en LUFS et vérifie-le.
  « Ça sonne fort » n'est pas une mesure.
- **les stems manquent.** Sans pistes séparées, le client ne peut rien ajuster
  sans revenir vers toi. C'est une livraison incomplète, pas un service.

## Les outils

Reaper suffit et coûte soixante euros une fois. Ardour est libre. Les banques
gratuites ont largement dépassé le niveau où le matériel décide de la qualité
du résultat. Voir le [toolkit audio](/guides/toolkit-audio).

## Où sont les gens

`#audio-composer` et `#audio-general` sur le Discord. Le salon vocal
« Composition Feedback » est l'endroit où l'on fait écouter une esquisse avant
de la finir, ce qui est plus utile qu'un avis sur un morceau terminé.
$md$, 10),

('onboarding-audio-sound-design', 'onboarding', 'audio', 'sound-design', 'fr',
 'Débuter en design sonore',
 'Fabriquer des sons qui servent : la fonction avant la matière, et la cohérence avant les deux.',
$md$
# Débuter en design sonore

Un son de design sonore n'est presque jamais écouté seul. Il arrive au milieu
d'une action, superposé à trois autres, pour dire quelque chose à quelqu'un qui
regarde ailleurs. C'est ce contexte qui décide s'il est bon.

## Les trente premiers jours

1. **Dix sons d'interface.** Court, discret, différenciable. La contrainte
   « supportable à la centième écoute » élimine la moitié des idées.
2. **Un empilement.** Fabrique un impact en trois couches — claquement, corps,
   queue — et écoute ce que chacune apporte quand on la retire.
3. **Une ambiance.** Un lieu à l'oreille : un fond, des événements épars, un
   mouvement lent. C'est là qu'on apprend la patience.
4. **Un pack.** Vingt sons qui appartiennent visiblement au même monde. La
   cohérence est ce qui distingue un pack d'une collection.

## Le bruitage plutôt que la banque

Enregistrer soi-même n'est pas une question de pureté : un son de banque a été
utilisé mille fois et un auditeur le reconnaît sans savoir pourquoi. Un micro
d'entrée de gamme et une pièce calme suffisent à faire mieux que la banque sur
ce qui compte, c'est-à-dire l'unicité.

## Ce qui fait revenir un travail

- **le nommage.** Vingt fichiers appelés `final_2_ok.wav` sont vingt fichiers
  qu'un intégrateur ne peut pas utiliser. Une convention, appliquée.
- **la feuille d'usage.** Quel son pour quelle situation, à quel niveau. Sans
  elle, le pack sera mal employé et c'est ton travail qui aura l'air raté.
- **les sources.** Comme partout ici : déclarées, avec leur licence.

## Où sont les gens

`#audio-sound-designer`, et `#audio-battles` pour les duels de quarante-huit
heures — le format le plus rapide pour apprendre, parce qu'on entend
immédiatement une autre réponse au même brief.
$md$, 20),

('onboarding-audio-voice', 'onboarding', 'audio', 'voice', 'fr',
 'Débuter en voix',
 'La prise, la direction, et les droits — dans cet ordre, parce que le troisième est celui qu''on oublie.',
$md$
# Débuter en voix

Le métier tient en trois compétences qui n'ont rien à voir entre elles : jouer,
enregistrer proprement, et savoir ce que tu cèdes. La troisième est celle qui
coûte le plus cher quand elle manque.

## Les trente premiers jours

1. **La pièce avant le micro.** Une armoire pleine de vêtements est un meilleur
   studio qu'un salon vide avec un micro à mille euros. Traite d'abord.
2. **Une narration de cinq minutes.** Tenir un rythme sur la durée est ce qui
   distingue une voix professionnelle d'une belle voix.
3. **Cinq personnages sur les mêmes répliques.** C'est le seul exercice qui
   montre un registre plutôt que de le déclarer.
4. **La bande démo.** Elle vient en dernier, pas en premier : une démo faite
   avant d'avoir travaillé montre ce qu'on croit savoir faire.

## Tes droits, en une page

- **Ta voix t'appartient.** C'est un attribut de ta personne, pas un fichier.
  Aucune utilisation ne va de soi.
- **Écris l'étendue.** Support, territoire, durée, exclusivité. Quatre
  questions, quatre réponses, avant l'enregistrement.
- **Le portfolio.** Garde le droit de montrer ce que tu as fait, sauf raison
  précise et payée. Sans lui, tu ne peux pas prouver ton propre travail.
- **Le clonage.** Skilluv interdit d'entraîner une voix synthétique sur la
  tienne sans ton accord écrit et explicite. Si on te le demande ailleurs,
  c'est une négociation à part entière, pas une clause de détail.

## Les castings

Les castings de la plateforme sont à l'aveugle par défaut : le créateur entend
les prises sans les noms. C'est fait pour que la première fois soit possible.

## Où sont les gens

`#audio-voice-actor`, et le salon vocal « Voice Casting Sessions ».
$md$, 30),

('onboarding-audio-implementation', 'onboarding', 'audio', 'implementation', 'fr',
 'Débuter en intégration et programmation audio',
 'Le son au moment de l''exécution : middleware, budget, et la règle du fil audio.',
$md$
# Débuter en intégration et programmation audio

Ici, un son n'existe pas tant qu'il ne se déclenche pas au bon moment dans une
build. Tout ce qui sonne bien dans l'éditeur et pas dans le jeu ne compte pas.

## Deux chemins qui se rejoignent

- **Intégration.** FMOD, Wwise, ou le moteur nu. Tu ne écris pas la musique,
  tu écris son comportement.
- **Programmation.** Le niveau en dessous : DSP, spatialisation, synthèse. Tu
  écris ce que le middleware appelle.

Les deux partagent une famille de relecture parce qu'ils partagent la même
question : est-ce que ça arrive juste, à temps, dans le budget.

## Les trente premiers jours

1. **Une intégration simple, vérifiée en build.** Un son déclenché par un
   événement du jeu. Prends l'habitude de tester dans la build, pas ailleurs.
2. **Un remixage vertical.** Trois couches qui s'activent sans casser la
   mesure. C'est le premier endroit où la musique devient un système.
3. **Le budget.** Compte les voix, la mémoire, le streaming. Sur la cible, pas
   sur ta machine.
4. **Un cas dégradé.** Ce qui se passe quand la banque manque ou quand tout se
   déclenche en même temps. Un silence choisi vaut mieux qu'une saturation
   subie.

## La règle du fil audio

Pas d'allocation, pas de verrou, pas d'entrée-sortie dans le thread audio. Ce
n'est pas un conseil de performance : un blocage de deux millisecondes produit
un clic que tout le monde entend. Si tu ne devais retenir qu'une chose de ce
guide, c'est celle-là.

## Où sont les gens

`#audio-music-implementer` et `#audio-programmer`.
$md$, 40);

-- ═══════════════════════════════════════════════════════════════════
-- Toolkit
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('toolkit-audio', 'toolkit', 'audio', NULL, 'fr',
 'Outils audio',
 'Ce qu''il faut installer par métier, ce que ça coûte, et l''équivalent libre quand il existe.',
$md$
# Outils audio

Ce guide est écrit avec une contrainte : quelqu'un qui commence avec zéro euro
doit pouvoir faire tout ce que la plateforme demande. Les logiciels payants
sont listés parce qu'ils existent dans l'industrie, jamais parce qu'ils sont
nécessaires.

## Stations de travail (DAW)

| Outil | Coût | Remarque |
|---|---|---|
| **Reaper** | 60 $ licence perso, évaluation illimitée | Le standard indépendant. Léger, scriptable, tourne sur une vieille machine. |
| **Ardour** | Libre | Complet, multiplateforme. Le choix par défaut à budget nul. |
| **Audacity** / **OcenAudio** | Libre | Montage et nettoyage, pas de composition. |
| Logic Pro, FL Studio, Ableton, Cubase, Pro Tools | 200 à 600 € | Répandus dans l'industrie. Aucun n'est requis ici. |

## Middleware de jeu

| Outil | Coût | Remarque |
|---|---|---|
| **FMOD Studio** | Gratuit sous 200 000 $ de revenus | Le plus simple à apprendre. |
| **Wwise** | Gratuit sous 200 000 $ | Plus puissant, plus verbeux. |
| **Godot AudioStreamPlayer**, **bevy_audio** | Libre | Sans dépendance externe. Suffisant pour beaucoup de projets, et le seul chemin si le jeu vise une plateforme que le middleware ne couvre pas. |

## Traitements

Les suites gratuites de **Melda Production** et de **Voxengo** couvrent
l'égalisation, la compression et la mesure de loudness. **Youlean Loudness
Meter** (gratuit) donne les LUFS que la grille de revue demande.

Pour la voix, **iZotope RX** est l'outil du métier pour le nettoyage ; son
équivalent gratuit est la patience et un montage propre.

## Prise de son

Un micro dynamique à cent euros dans une pièce traitée bat un micro à mille
euros dans une pièce vide. Traite d'abord : couvertures, matelas, une armoire
ouverte. La chaîne complète utilisable commence autour de cent cinquante euros.

## Banques et sources

- **Freesound**, **OpenGameArt** — libres, avec des licences à lire.
- **Airwindows** — traitements libres, de qualité professionnelle.
- Splice, Kontakt, EastWest — payants, et à déclarer dans les licences dès
  qu'un extrait apparaît dans un rendu.

## Programmation

**JUCE** (C++) pour les greffons, **cpal** et **fundsp** (Rust), **Web Audio
API** pour le navigateur, les API FMOD et Wwise pour l'intégration.
$md$, 100);

-- ═══════════════════════════════════════════════════════════════════
-- Brief templates — for whoever commissions the work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('brief-audio-composition', 'brief_template', 'audio', 'composition', 'fr',
 'Brief — composition',
 'Ce qu''il faut écrire avant de commander une musique, pour que ce qui arrive soit ce qu''on attendait.',
$md$
# Brief — composition

Copie ce modèle et remplis-le. Chaque ligne vide est un aller-retour de plus.

## Le projet
- De quoi s'agit-il, et à quoi sert la musique dedans ?
- Où sera-t-elle entendue : casque, téléphone, salle, mélangée à de la parole ?

## La pièce
- **Durée** attendue, et si elle doit boucler.
- **Ambiance** en trois adjectifs, et un contre-exemple : « pas triomphal ».
- **Références** : deux ou trois morceaux existants, avec ce qui te plaît dans
  chacun. Une référence sans commentaire dit seulement « fais pareil ».
- **Instrumentation** souhaitée ou exclue.

## La livraison
- **Formats** : WAV 48 kHz / 24 bits par défaut, plus les versions compressées.
- **Stems** : oui par défaut. Si tu dis non, tu ne pourras plus rien ajuster.
- **Loudness** visé, selon la destination.
- **Date**.

## Les droits
- **Étendue** : synchronisation seule, commercial limité, mondial, exclusif ?
- **Durée** et **territoire**.
- Qui garde la propriété, et qui peut montrer le travail en portfolio ?

## Les révisions
Le nombre de tours est de cinq par défaut sur cette plateforme. Dis à quoi tu
comptes les employer.
$md$, 200),

('brief-audio-sound-pack', 'brief_template', 'audio', 'sound-design', 'fr',
 'Brief — pack sonore',
 'Commander des bruitages : la liste, l''usage, et le format d''intégration.',
$md$
# Brief — pack sonore

## L'usage
- Où ces sons se déclenchent-ils, et pour dire quoi à qui ?
- Sur quoi seront-ils superposés ? Un son parfait seul peut disparaître dans le
  mixage final.
- Plateforme cible et contraintes : taille, format, mémoire.

## La liste
Un tableau, une ligne par son : nom, situation, durée approximative, remarque.
Une liste précise vaut mieux qu'un adjectif : « vingt sons de combat » se
livre de vingt façons.

## Le style
- Réaliste, stylisé, rétro ? Une référence par famille.
- Faut-il des variations pour les sons répétés fréquemment ?

## La livraison
- **Formats** et fréquence d'échantillonnage.
- **Convention de nommage** : donne la tienne si tu en as une.
- **Feuille d'usage** attendue.

## Les droits
Étendue, exclusivité, portfolio — les mêmes quatre questions que partout.
$md$, 210),

('brief-audio-voice', 'brief_template', 'audio', 'voice', 'fr',
 'Brief — voix',
 'Commander une voix : le personnage, les répliques d''essai, et l''étendue d''usage.',
$md$
# Brief — voix

## Le personnage
- Qui est-ce ? Âge, situation, ce qu'il veut, ce qu'il cache.
- Comment parle-t-il quand tout va bien, et quand rien ne va ?
- **Contre-exemple** : quelle voix serait fausse pour ce rôle ?

## La langue
Langue et variante — le français de Cotonou, de Montréal et de Lyon ne sont pas
interchangeables, et le dire évite un malentendu coûteux.

## Les répliques d'essai
Trois à cinq lignes, tirées du texte réel, qui couvrent des registres
différents. Tout le monde lit les mêmes, sinon les prises ne sont pas
comparables.

## Le volume
- Nombre de répliques final, et rythme de livraison.
- Y aura-t-il des reprises quand le texte changera ?

## La livraison
- Format, montée ou brute, avec ou sans respirations.
- Nommage des fichiers, surtout au-delà de cinquante répliques.

## Les droits — la partie à ne pas laisser vide
- **Support, territoire, durée, exclusivité.**
- **Portfolio** : le comédien peut-il montrer un extrait ? La réponse est oui
  sauf raison précise.
- **Voix de synthèse** : entraîner un modèle sur ces prises exige un accord
  écrit distinct. Sans lui, c'est interdit sur cette plateforme.
- **Suites et reprises** : une réutilisation dans une version ultérieure
  se prévoit ici ou se renégocie plus tard.
$md$, 220),

('brief-audio-adaptive', 'brief_template', 'audio', 'implementation', 'fr',
 'Brief — musique adaptative',
 'Commander une intégration : les états du jeu, les transitions, et le budget.',
$md$
# Brief — musique adaptative

## Le jeu
- Moteur et version. Middleware déjà en place, ou à choisir ?
- Qui, côté équipe, branche les événements ? C'est la question qui décide de la
  moitié du travail.

## Les états
Une liste des situations que la musique doit distinguer, et ce qui les
déclenche dans le code. « Combat » n'est pas un état tant que personne ne sait
quel événement l'annonce.

## Les transitions
- Quelles bascules doivent être imperceptibles, lesquelles peuvent être
  franches ?
- Quel délai est acceptable entre l'événement et la réaction musicale ?

## Le budget
- Mémoire disponible pour l'audio, et nombre de voix simultanées.
- Plateforme la plus contrainte visée.

## La livraison
- Projet middleware, build de démonstration, documentation des paramètres
  exposés.
- Qui maintient l'intégration après la livraison ?
$md$, 230),

('brief-audio-programming', 'brief_template', 'audio', 'implementation', 'fr',
 'Brief — développement audio',
 'Commander du code audio : le problème, la cible, le budget de performance.',
$md$
# Brief — développement audio

## Le problème
Décris ce qui ne marche pas ou ce qui manque, pas la solution que tu imagines.
« Il faut de la HRTF » est une solution ; « on ne sait pas d'où vient un son
derrière soi » est un problème, et il a peut-être trois réponses.

## La cible
- Moteur, langage, plateformes.
- Taille du tampon et fréquence d'échantillonnage en production.
- Ce qui existe déjà et qu'il ne faut pas casser.

## Le budget
- Coût CPU acceptable, en pourcentage d'un cœur ou en millisecondes par bloc.
- Mémoire.
- Latence maximale tolérée.

## La livraison
- Code, licence, et où il vit.
- Exemple minimal qui tourne, et ce qui compte comme démonstration.
- Documentation attendue : intégration, cas d'erreur, limites connues.
$md$, 240);

-- ═══════════════════════════════════════════════════════════════════
-- Writeup templates — written by the person who did the work
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('writeup-composition-notes', 'writeup_template', 'audio', 'composition', 'fr',
 'Notes de composition',
 'Ce qui accompagne une pièce livrée : l''intention, les choix, ce qui a été écarté.',
$md$
# Notes de composition — {titre}

## L'intention
Ce que la musique doit faire ressentir, en deux phrases. Pas ce qu'elle
contient : ce qu'elle vise.

## Comment elle est construite
Le matériau principal, où il revient, comment il est varié. Forme générale.

## Les choix, et ce qu'ils écartent
Deux ou trois décisions, avec l'alternative qui a été essayée et abandonnée. Un
relecteur juge mieux un choix quand il sait ce qu'il exclut.

## Les références
Ce qui a servi de point de départ, et où le morceau s'en écarte volontairement.

## Technique
Loudness intégré mesuré, crête vraie, plage dynamique. Fréquence et profondeur.
Liste des stems.

## Les sources
Chaque échantillon, boucle ou banque, avec sa licence — ou « tout original ».
$md$, 300),

('writeup-sound-pack-usage', 'writeup_template', 'audio', 'sound-design', 'fr',
 'Feuille d''usage d''un pack',
 'Le document sans lequel un pack est mal employé et paraît raté.',
$md$
# Feuille d'usage — {nom du pack}

## Ce que contient le pack
Nombre de sons, familles, convention de nommage expliquée en une ligne.

## Quel son pour quelle situation
Un tableau : fichier, situation, niveau conseillé, remarque. C'est la partie
que l'intégrateur lira, et souvent la seule.

## Comment il est fait
Le principe commun : quelles couches, quel espace, quel grain. Ce qui donne au
pack son unité.

## Intégration
Format livré et pourquoi. Sons qui gagnent à être variés aléatoirement.
Précautions de mixage — ce avec quoi ces sons entrent en conflit.

## Les sources
Chaque enregistrement, échantillon ou banque, avec sa licence.
$md$, 310),

('writeup-voice-reel-notes', 'writeup_template', 'audio', 'voice', 'fr',
 'Description d''une bande démo',
 'Ce qui accompagne une démo : le contenu, les conditions, l''étendue d''usage.',
$md$
# Bande démo — {nom}

## Contenu
Pour chaque extrait : personnage ou registre, contexte, durée, langue et
variante.

## Ce que la démo montre
Trois lignes honnêtes sur l'étendue réellement démontrée. Une démo qui promet
plus que ce qu'elle contient est découverte à la première séance.

## Conditions d'enregistrement
Micro, interface, traitement de la pièce. Montage appliqué, le cas échéant.

## Technique
Loudness, crête vraie, format.

## Droits
Ce que quelqu'un qui écoute cette démo est autorisé à en faire. Les extraits
tirés de travaux commandés le sont avec l'accord du client, ou pas du tout.
$md$, 320),

('writeup-adaptive-implementation', 'writeup_template', 'audio', 'implementation', 'fr',
 'Rapport d''intégration adaptative',
 'Comment le système est construit, ce qu''il coûte, et où il casse.',
$md$
# Intégration — {projet}

## Le système
Les états, ce qui les déclenche, et la carte des transitions autorisées. Un
schéma vaut une page.

## Les couches
Ce qui compose chaque état, et ce qui reste quand on en enlève une.

## Les paramètres exposés
Chacun avec son nom, son domaine de valeurs et son effet. C'est ce qui permet à
quelqu'un d'autre de brancher un nouvel événement sans te demander.

## Le budget
Voix simultanées mesurées, mémoire, streaming, coût CPU. Sur la cible.

## Les cas limites
Bascule rapide, aller-retour, banque manquante, tout en même temps. Ce que le
système fait, et pourquoi c'est le comportement choisi.

## Ce qui reste à faire
Les limites connues, écrites par toi plutôt que découvertes par le relecteur.
$md$, 330),

('writeup-audio-programming-breakdown', 'writeup_template', 'audio', 'implementation', 'fr',
 'Décomposition d''un développement audio',
 'Le problème, la méthode, les mesures, les limites.',
$md$
# {nom du système}

## Le problème
Ce qui manquait, et pourquoi les solutions existantes ne convenaient pas.

## L'approche
L'algorithme ou l'architecture, en termes qu'un développeur non spécialiste
peut suivre. Les compromis pris.

## Les mesures
Coût par bloc, allocation dans le fil audio (idéalement : aucune), latence,
mémoire. Sur quel matériel, avec quel tampon.

## La validation
Comment tu sais que c'est correct : tests, comparaison à une référence,
signaux d'essai, écoute.

## Les limites
Ce sur quoi ça échoue, et ce qui n'a pas été traité. Écrit par toi.

## Réutilisation
Licence, dépendances, exemple minimal qui tourne.
$md$, 340),

('writeup-audio-licensing', 'writeup_template', 'audio', NULL, 'fr',
 'Déclaration de sources et de licences',
 'Le document qui rend une livraison audio utilisable. Sans lui, rien d''autre ne compte.',
$md$
# Sources et licences — {livraison}

## Déclaration
> Je déclare que la liste ci-dessous est complète et exacte, et qu'aucune autre
> source tierce n'entre dans cette livraison.

## Sources tierces
Un tableau : source, où elle a été obtenue, licence exacte, mention
d'attribution requise verbatim, usage commercial autorisé ou non.

Une ligne par source. Une banque commerciale se déclare une fois, avec le
numéro de licence si elle en a un.

## Éléments originaux
Ce qui a été créé pour cette livraison, et à qui cela appartient à l'arrivée.

## Mentions à afficher
Le bloc de crédits à recopier tel quel, s'il y en a un. Les licences Creative
Commons BY ne sont gratuites qu'à cette condition.

## Voix
Pour tout enregistrement de voix : nom de l'interprète, étendue d'usage
accordée, et la mention explicite de l'accord ou du refus concernant
l'entraînement d'une voix de synthèse.
$md$, 350),

('writeup-audio-post-mortem', 'writeup_template', 'audio', NULL, 'fr',
 'Post-mortem audio',
 'Ce qu''on a appris, écrit pendant qu''on s''en souvient encore.',
$md$
# Post-mortem — {projet}

## Le cadre
Durée, budget, rôle exact, avec qui.

## Ce qui a marché
Deux ou trois choses, et pourquoi — la cause, pas le résultat.

## Ce qui n'a pas marché
Deux ou trois choses, formulées sans chercher de responsable. Le brief flou et
la révision de trop en font partie plus souvent que la technique.

## Les révisions
Combien de tours, sur quoi, et ce qui les aurait évités. C'est la section la
plus utile pour la commande suivante.

## Ce que je ferai différemment
Trois phrases concrètes.
$md$, 360);
