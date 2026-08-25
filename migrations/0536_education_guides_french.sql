-- The French half of the education guides.
--
-- Same gap as 0535 closes for communication: 0530 seeded fifteen rows in
-- English and none in French, and the fallback chain served English to a
-- French reader without anything reporting that the translation did not
-- exist. Ticket education/G-01 asks for both.
--
-- ## The vocabulary decisions
--
-- "Learner" becomes *apprenant* rather than *élève* or *étudiant*: the people
-- in these cohorts are adults in reconversion as often as they are students,
-- and both other words pick a side. "Facilitator notes" becomes *notes de
-- l'animateur*, which is what the people who use them call them.
--
-- The named references stay in English — Julie Dirksen's book, Cathy Moore's
-- action mapping, cognitive load theory — because that is how somebody would
-- search for them, and a translated title finds nothing.

-- ═══════════════════════════════════════════════════════════════════
-- Onboarding
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-education-trainer', 'onboarding', 'education', 'teaching', 'fr',
 'Débuter comme formateur technique',
 'Animer pour une salle arrivée avec un niveau annoncé : par où commencer, et ce qui fait revenir un travail.',
$md$
# Débuter comme formateur technique

Former, ce n'est pas présenter. Une conférence réussit si la salle a compris ;
une formation réussit si la salle sait maintenant faire la chose. Ce sont deux
cibles différentes et elles demandent deux préparations différentes.

## La règle qui décide de tout le reste

Les participants doivent passer plus de temps à faire qu'à regarder. Un
atelier de trois heures avec vingt minutes de pratique est une conférence à
laquelle on a accroché des exercices. Concevez les exercices d'abord et calez
l'explication autour, pas l'inverse — c'est le changement qui améliore le plus
une séance.

## Les trente premiers jours

1. **Une séance de quatre-vingt-dix minutes, un seul concept.** Assez petit
   pour être préparé correctement, et pour découvrir ce que vous ignoriez de
   votre propre sujet.
2. **L'exercice qui échoue utilement.** Écrivez une tâche où la mauvaise
   approche ne marche visiblement pas. Cet échec enseigne plus que votre
   explication de pourquoi elle ne marcherait pas.
3. **Une passe avec quelqu'un qui observe.** Demandez à un collègue d'assister
   et de vous dire où la salle vous a perdu. Vous ne le verrez pas vous-même.
4. **Un résultat mesuré.** Demandez ce que les gens savaient faire avant et
   après, sur quelque chose d'observable. Faites-le même pour quatre-vingt-dix
   minutes.

## Préparer l'environnement

Plus de séances échouent sur l'installation que sur le contenu :

- annoncez les prérequis, avec les versions, au moins une semaine avant ;
- prévoyez un recours qui ne demande aucune installation locale — un
  conteneur, un environnement hébergé, une machine virtuelle préparée ;
- faites l'installation vous-même sur une machine propre, pas sur la vôtre ;
- ayez quelque chose pour la personne dont le portable refuse absolument de
  coopérer, afin qu'elle travaille en binôme plutôt que de rester en dehors.

## Ce qui fait revenir un travail

- **aucune preuve que quelqu'un a appris.** La satisfaction est un vrai signal
  sur le fait que les gens reviennent, et ce n'est pas une preuve. Mesurez
  quelque chose.
- **des supports que personne d'autre ne pourrait animer.** Diapositives sans
  notes, exercices sans corrigés, un environnement que vous seul savez monter.
  C'est une performance, pas un artefact.
- **un apprenant dans le livrable.** Noms, visages, notes, messages.
  Anonymisez à la source. Un livrable qui expose un participant est refusé,
  quelle qu'ait été la qualité de la formation.

## Où aller ensuite

- `#edu-trainer` sur Discord.
- La grille de relecture de votre famille est publique : lisez-la avant de
  soumettre.
- Julie Dirksen, *Design for How People Learn* — le livre qui change la façon
  de préparer.
$md$, 410),

('onboarding-education-coding-teacher', 'onboarding', 'education', 'teaching', 'fr',
 'Débuter comme enseignant en programmation',
 'Les mêmes personnes tous les jours, débutantes pour la plupart : en quoi consiste réellement le métier.',
$md$
# Débuter comme enseignant en programmation

Le difficile, dans ce métier, ce n'est pas le sujet. Vous savez comment marche
une boucle. Le difficile, c'est de regarder quelqu'un être bloqué et de savoir
lequel de quatre problèmes différents vous avez sous les yeux.

## Les quatre raisons d'être bloqué

Apprenez à les distinguer avant tout le reste. Elles sont identiques vues du
tableau et appellent des réponses opposées :

1. **Un prérequis manquant.** La personne ne peut pas faire ceci parce qu'elle
   ne sait pas faire quelque chose d'antérieur. Reprendre l'étape en cours n'y
   changera rien.
2. **Une consigne mal lue.** Elle résout correctement un autre problème.
   Demandez-lui de vous redire ce qu'elle croit être en train de faire.
3. **Un environnement cassé.** Son raisonnement n'a rien de faux. Dix minutes
   de ça et elle conclura qu'elle est mauvaise en programmation.
4. **La peur de demander.** Bloquée depuis quarante minutes, elle n'a rien
   dit. C'est la plus fréquente et la seule qui empire avec le temps.

Demandez avant d'expliquer. « Montre-moi ce que tu as essayé » sépare les
quatre plus vite que n'importe quelle observation.

## Les trente premiers jours

1. **Un plan de cours, une idée fausse.** Prenez quelque chose que les
   débutants ratent de façon fiable et construisez le cours autour de sa mise
   en évidence.
2. **Du live coding, lentement.** Tapez, faites l'erreur exprès, et racontez
   la décision plutôt que la syntaxe. Les débutants apprennent plus en vous
   regardant déboguer qu'en vous regardant réussir.
3. **Dix minutes avec le silencieux.** Dans chaque groupe il y a quelqu'un qui
   n'a rien dit depuis la première semaine. Aller vers lui, c'est le métier.
4. **Une passation.** Donnez votre plan de cours à quelqu'un d'autre et voyez
   s'il peut l'animer. Ce qui manque, c'est ce que vous portiez dans la tête.

## Ce dont personne ne vous prévient

La troisième semaine. Dans presque toutes les cohortes, ceux qui vont partir
partent en semaine trois : l'enthousiasme du début est retombé, la matière est
devenue réelle, et personne n'a encore réussi quoi que ce soit de visible.
Prévoyez une petite victoire nette en semaine deux, et allez voir tout le
monde en semaine trois, qu'ils aient demandé ou non.

## Ce qui fait revenir un travail

- **enseigner en faisant à leur place.** Un apprenant qui ne sait travailler
  qu'en votre présence a été porté. Étayez, puis retirez l'étai.
- **un exercice qu'on peut copier.** Si la solution précédente passe,
  personne n'apprend.
- **un apprenant dans le livrable.** Anonymisez à la source. Tout compte rendu
  de ce métier parle de personnes réelles qui n'ont pas demandé à servir de
  preuve.

## Où aller ensuite

- `#edu-teacher` sur Discord.
- La grille de relecture de votre famille est publique.
- Cherchez *cognitive load theory* et *worked example effect* : deux idées
  réellement étayées qui changeront ce que vous mettez sur une diapositive.
$md$, 415),

('onboarding-education-curriculum', 'onboarding', 'education', 'curriculum', 'fr',
 'Débuter en conception de parcours',
 'Décider ce qui est appris, dans quel ordre, et comment on sait que ça a marché.',
$md$
# Débuter en conception de parcours

Un parcours est lu par des gens qui n'étaient pas dans la pièce quand il a été
décidé. C'est toute la contrainte : tout ce qui vous est évident doit être
écrit, et tout ce qui est écrit doit survivre à une lecture littérale.

## Les objectifs d'abord, et observables

« Comprend la récursivité » n'est pas un objectif — personne ne peut dire si
c'est arrivé. « Écrit un parcours d'arbre récursif et prédit sa profondeur »
en est un : un apprenant peut le viser et un évaluateur peut le vérifier.

Écrivez les objectifs avant le contenu. Un programme conçu à partir d'une
liste de sujets produit des modules défendables un à un et qui n'additionnent
rien.

## Le saut silencieux

Le défaut le plus courant de ce domaine, et de loin. Le module quatre suppose
quelque chose que le module trois n'a pas enseigné, personne ne le remarque
parce que l'auteur le savait déjà, et la moitié de la salle décroche en
silence.

Deux habitudes l'attrapent :

- écrire explicitement les prérequis de chaque module, y compris ceux que vous
  jugez trop évidents pour être dits — ce sont ceux-là ;
- faire lire la séquence par quelqu'un qui ne connaît pas le sujet et lui
  faire marquer où il serait perdu.

## Les trente premiers jours

1. **Un module, complet.** Objectifs, contenu, exercice, évaluation, notes de
   l'animateur. Complet vaut mieux que large.
2. **Une carte de prérequis** de quelque chose qui existe déjà. Prenez un
   parcours publié et dessinez ce qui dépend de quoi. Vous trouverez un saut.
3. **Une grille sur laquelle deux personnes s'accordent.** Écrivez les
   critères, puis faites évaluer le même travail par deux personnes. Là où
   elles divergent, la grille était vague.
4. **Une passation.** Donnez votre module à un formateur et regardez-le
   l'animer.

## L'alignement

Ce qui est évalué est ce qui est appris, quoi que disent les objectifs. Si les
objectifs portent sur la conception de systèmes et que l'évaluation est un QCM
de syntaxe, le programme enseigne la syntaxe. Vérifiez l'alignement en
dernier, à chaque fois, et changez l'évaluation plutôt que les objectifs.

## Ce qui fait revenir un travail

- **des objectifs non observables.** Comprendre, apprécier, être familier
  avec.
- **pas de notes de l'animateur.** Minutage, quoi faire quand une séance
  déborde, où les gens bloquent, et les corrigés. Sans elles, vous seul pouvez
  l'animer.
- **pas de péremption.** Un parcours qui nomme des versions d'outils sans les
  dater casse en silence.

## Où aller ensuite

- `#edu-curriculum` sur Discord.
- L'action mapping de Cathy Moore, pour les programmes commandés par une
  organisation qui a un problème plutôt qu'un sujet.
- La grille de relecture de votre famille est publique.
$md$, 420);

-- ═══════════════════════════════════════════════════════════════════
-- Toolkit
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES
('toolkit-education', 'toolkit', 'education', NULL, 'fr',
 'Boîte à outils de l''enseignement',
 'Ce qui suffit pour commencer, ce qui ne coûte rien, et la seule chose qui vaut d''être payée.',
$md$
# Boîte à outils de l'enseignement

Tout ce qui suit est gratuit ou dispose d'un palier gratuit réellement
utilisable, sauf mention contraire. Rien n'est nécessaire : un tableau et des
exercices préparés couvrent les six premiers mois.

## Préparer

- **Obsidian**, **Notion**, du Markdown dans un dépôt — n'importe où qui
  permette de versionner un programme. Versionnez-le : un parcours sans
  historique est un parcours que personne ne peut relire.
- **Miro**, **Excalidraw** — pour la forme d'un atelier avant qu'il ait des
  diapositives.
- **Reveal.js**, **Slidev**, **Marp** — des diapositives écrites en Markdown,
  donc rangées à côté des exercices dans le même dépôt.

## L'environnement, là où les séances échouent vraiment

- **Dev containers** / **Docker Compose** — l'installation qu'un participant
  n'a pas à faire.
- **GitHub Codespaces**, **Gitpod** — des paliers gratuits suffisants pour un
  atelier, et la réponse au portable qui refuse de coopérer.
- **Asciinema** — enregistrer un terminal en texte : léger, copiable, lisible
  sur un téléphone, et sans lecteur vidéo.

## Animer

- **BigBlueButton**, **Jitsi** — visioconférence libre avec salles de
  sous-groupe.
- **OBS Studio** — enregistrer, et diffuser une séance à qui n'a pas pu venir.
- **Excalidraw** à nouveau, partagé, comme tableau blanc où tout le monde
  dessine.

## Évaluer

- **LibreForms**, **Framaforms**, **Google Forms** — pour un point rapide qui
  prend quatre-vingt-dix secondes.
- **Moodle** — logiciel libre, et le seul LMS complet de cette liste. Lourd ;
  ça ne vaut le coup que pour un programme qui tourne plusieurs fois.
- **nbgrader**, **Autograding avec GitHub Classroom** — quand l'exercice est
  du code et que la vérification peut être automatique. Automatisez la part
  objective pour que votre attention aille à celle qui ne l'est pas.

## Enregistrer un cours

- **DaVinci Resolve** — montage professionnel, version gratuite complète.
- **Audacity** — nettoyer une piste voix.
- **Whisper** — sous-titres automatiques à relire. Ne publiez jamais des
  sous-titres que personne n'a relus.

## La seule chose qui vaut d'être payée

Un micro correct. Chaque module enregistré que vous ferez sera limité par lui,
et c'est le seul poste où cent euros changent quelque chose que tout le monde
entend.
$md$, 430);

-- ═══════════════════════════════════════════════════════════════════
-- Briefs
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('brief-education-training', 'brief_template', 'education', 'teaching', 'fr',
 'Brief — animation de formation',
 'À remplir avant de commander un atelier, un cours ou une cohorte.',
$md$
# Brief — animation de formation

## Les apprenants
- Combien :
- Ce qu'ils savent déjà, honnêtement :
- Ce qu'ils font dans leur travail :
- Viennent-ils volontairement ?

## Le résultat
- Ce qu'ils doivent savoir faire ensuite, énoncé de façon observable :
- Comment vous saurez que ça a marché :
- Que se passe-t-il s'ils n'y arrivent pas ?

## La forme
- Format : [ ] atelier · [ ] cours court · [ ] cohorte · [ ] enregistré
- Total d'heures devant les gens :
- Sur quelle période :
- En présentiel, à distance, ou les deux ?

## L'environnement
- Quelles machines ? Qu'y a-t-il d'installé dessus ?
- Qui peut installer, et combien de temps prend l'autorisation ?
- Y a-t-il un recours pour celui dont l'installation échoue ?
- Réseau, écrans, disposition de la salle :

## Livraison
- Supports remis : [ ] diapositives [ ] exercices [ ] corrigés
  [ ] notes de l'animateur [ ] enregistrement
- Qui les possède ensuite, et le formateur peut-il les réutiliser ?
- Nombre de sessions incluses dans les honoraires :

## Données des apprenants
- Le formateur verra-t-il des noms, des notes, des évaluations ?
- Qu'est-ce qui peut sortir de la salle, et avec quel consentement ?
- Y a-t-il des participants de moins de 18 ans ? (si oui, dites-le maintenant
  — ça change ce qui peut être collecté, tout court)
$md$, 440),

('brief-education-curriculum', 'brief_template', 'education', 'curriculum', 'fr',
 'Brief — conception de parcours',
 'À remplir avant de commander un programme que quelqu''un d''autre animera.',
$md$
# Brief — conception de parcours

## Le problème
- Que ne savent pas faire les gens aujourd'hui ?
- Qu'est-ce qui change dans l'organisation quand ils le savent ?
- Pourquoi un programme plutôt que de la documentation ou de l'outillage ?

## Les apprenants
- Qui, combien, et ce qu'ils savent déjà faire :
- Le temps dont ils disposent, par semaine :
- Volontaire ou obligatoire ?

## Le programme
- Durée totale et cadence :
- Animé par qui ? (l'auteur, vos équipes, un tiers)
- Matériel existant à reprendre ou à remplacer :
- Contraintes : outils, langages, plateformes qui doivent ou ne doivent pas
  apparaître

## Évaluation
- La complétion doit-elle signifier quelque chose de formel ?
- Qui évalue, et avec quelle formation ?
- Y a-t-il un recours ? (il devrait y en avoir un)

## Livraison
- Format de la remise : dépôt, documents, import LMS
- Notes de l'animateur attendues : oui / non (répondez oui)
- Nombre de tours de relecture inclus :
- Qui valide, et contre quoi ?

## Droits et maintenance
- Qui possède le parcours ? L'auteur peut-il le publier ?
- Qui le met à jour quand une version d'outil bouge ?
- Une période de maintenance fait-elle partie de la commande ?
$md$, 450),

('brief-education-teaching-engagement', 'brief_template', 'education', 'teaching', 'fr',
 'Brief — mission d''enseignement',
 'À remplir avant de commander un enseignement suivi : un semestre, un module, une série de cohortes.',
$md$
# Brief — mission d'enseignement

## La mission
- Période, et heures par semaine :
- Nombre d'apprenants par groupe, nombre de groupes :
- Programme existant, ou à concevoir ? (si à concevoir, c'est une autre
  commande)

## Les apprenants
- Niveau, parcours, pourquoi ils sont là :
- Des mineurs ? (si oui, consentement parental et minimisation des données
  s'appliquent dès le départ, pas après)
- Quelle proportion est censée aller au bout, d'après votre historique ?

## L'enseignement
- Qui d'autre enseigne sur le programme, et comment est-ce coordonné ?
- Qui prend en charge un apprenant qui décroche ?
- Permanences, suivi, temps de correction : payés ou supposés ?

## Évaluation
- Ce qui est évalué, par qui, contre quoi :
- Temps de correction inclus dans les honoraires : oui / non
- Qui défend une note si elle est contestée ?

## Données
- Quelles données d'apprenants l'enseignant détient-il, et où ?
- Que faut-il supprimer à la fin, et quand ?
- Que peut publier l'enseignant sur la mission ensuite ?

## Conditions
- Honoraires, et ce qu'ils couvrent (préparation et correction comprises ou
  non) :
- Annulation d'une séance : de part et d'autre, avec quel préavis
- Ce qui se passe si le groupe ne se remplit pas
$md$, 460);

-- ═══════════════════════════════════════════════════════════════════
-- Writeup templates
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('writeup-edu-curriculum-doc', 'writeup_template', 'education', 'curriculum', 'fr',
 'Document de conception de parcours',
 'La structure d''un programme que quelqu''un d''autre peut animer.',
$md$
# {Nom du programme}

**Public :** · **Durée :** · **Version :** · **Dernière relecture :**

## Ce que les apprenants sauront faire
Observable, vérifiable, et écrit pour l'apprenant.

## Prérequis
Y compris ceux qui semblent trop évidents pour être dits. Ce sont ceux-là.

## Séquence
| # | Module | Objectifs | Dépend de | Heures |
|---|---|---|---|---|

## Évaluation
Ce qui est évalué, comment, et contre quel objectif.

## Pour l'animateur
Minutage, quoi couper si ça déborde, où les gens bloquent, corrigés.

## Maintenance
Versions d'outils nommées, et ce qui casse quand chacune bouge.
$md$, 470),

('writeup-edu-lesson-plan', 'writeup_template', 'education', 'teaching', 'fr',
 'Plan de cours',
 'Une séance, sous une forme qu''un autre enseignant peut animer.',
$md$
# {Titre du cours}

**Durée :** · **Public :** · **Prérequis :**

## Objectif
Ce qu'ils sauront faire à la fin.

## L'idée fausse que ça attrape
Ce que les apprenants ratent de façon fiable ici.

## Déroulé
| Temps | Ce qui se passe | Qui fait quoi |
|---|---|---|

## Exercice
La tâche, le point de départ, et le corrigé.

## Vérification
Comment vous savez que c'est passé, avant qu'ils sortent.

## Si ça déborde
Quoi couper, dans l'ordre.
$md$, 475),

('writeup-edu-workshop-outline', 'writeup_template', 'education', 'teaching', 'fr',
 'Plan d''atelier — trois heures',
 'La forme d''une séance de pratique.',
$md$
# {Titre de l'atelier}

**Durée :** 3 h · **Participants :** · **Niveau :**

## Ce qu'ils savent faire en repartant

## Environnement
Ce qui doit marcher avant de commencer, et le recours quand ça ne marche pas.

## Déroulé
| Temps | Segment | Regarder ou faire |
|---|---|---|
| 0:00 | Vérification de l'installation | |
| 0:15 | | |

Gardez la colonne « faire » plus longue que la colonne « regarder ». C'est
toute la conception.

## Exercices
Chacun avec son point de départ, son corrigé, et son échec instructif.

## Vérification
Ce que vous demandez à la fin pour savoir si ça a marché.
$md$, 480),

('writeup-edu-cohort-syllabus', 'writeup_template', 'education', 'teaching', 'fr',
 'Syllabus de cohorte — huit semaines',
 'Ce à quoi une cohorte s''engage, des deux côtés.',
$md$
# {Nom de la cohorte}

**Dates :** · **Places :** · **Engagement hebdomadaire :**

## Pour qui c'est
Et pour qui ce n'est pas. Être explicite épargne huit semaines à des gens.

## Ce que vous saurez faire à la fin

## Semaine par semaine
| Semaine | Sujet | Ce que vous faites | Rendu |
|---|---|---|---|

## La semaine trois
Dites ce qui se passe en semaine trois, et quel soutien existe. C'est là que
les gens partent, et le nommer à l'avance aide.

## Évaluation
Ce qui est évalué, quand, et contre quoi.

## Ce qu'on vous demande, et ce que vous pouvez nous demander
$md$, 485),

('writeup-edu-rubric', 'writeup_template', 'education', 'curriculum', 'fr',
 'Grille d''évaluation',
 'Des critères avec lesquels deux évaluateurs arrivent à la même note.',
$md$
# Grille — {ce qui est évalué}

**Objectif évalué :**

| Critère | Pas encore | En approche | Atteint | Dépassé |
|---|---|---|---|---|

Chaque case décrit quelque chose d'observable dans le travail. « Bonne
structure » n'est pas observable ; « chaque fonction a une seule
responsabilité et son nom dit laquelle » l'est.

## Exemple traité
Un travail réel, noté, avec le raisonnement.

## Recours
Comment un apprenant conteste une note, et qui tranche.
$md$, 490),

('writeup-edu-pedagogy-post', 'writeup_template', 'education', 'teaching', 'fr',
 'Compte rendu pédagogique',
 'Écrire sur la façon dont les gens apprennent, à partir de ce que vous avez vu.',
$md$
# {Titre}

## Ce que j'ai observé
Concrètement, avec des chiffres s'il y en a. Anonymisé à la source.

## Ce que je pense qu'il se passe
Votre interprétation, signalée comme interprétation.

## Ce que dit la littérature
Si elle dit quelque chose. Si vous n'avez pas cherché, dites-le.

## Ce que j'ai essayé
Et ce que ça a donné.

## Ce que je ne peux pas conclure
La section qui sépare ceci d'un billet d'humeur. Une classe est une classe.
$md$, 495),

('writeup-edu-post-mortem', 'writeup_template', 'education', 'teaching', 'fr',
 'Rétrospective de formation',
 'Ce qui s''est passé, honnêtement, tant que c''est frais.',
$md$
# Rétrospective — {séance ou cohorte}

**Quand :** · **Participants :** · **Complétion :**

## Ce qui a marché
Assez précisément pour être refait.

## Ce qui n'a pas marché
Assez précisément pour être corrigé. Y compris les problèmes d'installation.

## Où la salle s'est perdue
Et si vous l'avez remarqué sur le moment.

## Résultats
Ce que les gens savaient faire ensuite, mesuré.

## Ce que je change la prochaine fois
Trois choses, dans l'ordre.
$md$, 497),

('writeup-edu-outcomes-report', 'writeup_template', 'education', 'curriculum', 'fr',
 'Rapport de résultats d''apprentissage',
 'Ce qui a changé, sous une forme qui n''expose personne.',
$md$
# Résultats — {programme}

**Cohorte :** · **Apprenants :** · **Période :**

## Méthode
Ce qui a été mesuré, comment, et quand. Avant et après, sur quelque chose
d'observable.

## Résultats
| Objectif | Avant | Après | Mesuré par |
|---|---|---|---|

Agrégé uniquement. Aucune ligne n'est une personne.

## Complétion
Commencé, terminé, et — quand c'est su — pourquoi les autres ont arrêté.

## Satisfaction
Rapportée comme un chiffre à part, et lue comme un signal sur le retour plutôt
que sur l'apprentissage.

## Limites
Taille de l'échantillon, auto-sélection, ce que ceci ne montre pas.

## Données des apprenants
Confirmez ce qui a été anonymisé et quel consentement couvre le reste.
$md$, 499);
