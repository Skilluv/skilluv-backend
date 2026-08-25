-- The French half of the communication guides.
--
-- Migration 0514 seeded twenty rows in English and said the French ones would
-- follow "the same way audio's English ones were in 0421". They did not, and
-- the guide endpoint falls back requested → English → French, so a French
-- reader was served English and nothing anywhere said the translation was
-- missing. Ticket communication/G-01 asks for both.
--
-- ## Why this is not a mechanical translation
--
-- Three of these guides are about French itself. The translation guide's
-- examples of what does not get translated are chosen for a reader whose
-- target language is French, and the paragraph on under-resourced languages
-- names Wolof, Lingala and Bambara because those are the languages the people
-- this platform is for actually translate into. Rendering that from English
-- word for word would produce a page that reads as though it were written
-- somewhere else, which for a guide about writing well is a self-refuting
-- document.
--
-- Tool names, format names and the four Diátaxis page types stay in English
-- where that is what practitioners say. `#comm-tech-writer` is a channel
-- name, not a phrase.

-- ═══════════════════════════════════════════════════════════════════
-- Onboarding
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('onboarding-communication-documentation', 'onboarding', 'communication', 'documentation', 'fr',
 'Débuter en documentation technique',
 'Écrire pour quelqu''un qui a une tâche et pas de patience : par où commencer, et ce qui fait revenir un travail.',
$md$
# Débuter en documentation technique

Documenter, ce n'est pas expliquer ce que fait le code. C'est répondre à la
question que se pose vraiment quelqu'un de bloqué, dans l'ordre où il se la
pose.

## Les quatre types de page, et pourquoi les mélanger échoue

Cette distinction vient du cadre Diátaxis et c'est la chose la plus utile à
savoir avant d'écrire une ligne :

- **Tutorial** — j'apprends. Tu me tiens la main, je ne décide rien,
  j'arrive à un résultat que je vois.
- **How-to guide** — j'ai une tâche précise. Je sais ce que je fais, je veux
  la recette.
- **Reference** — je cherche un fait. Paramètres, valeurs de retour, erreurs.
- **Explanation** — je veux comprendre pourquoi c'est construit comme ça.

Une page qui en fait deux perd les deux lecteurs. Le tutoriel qui bifurque
vers l'architecture au milieu perd le débutant et ennuie l'expert.

## Les trente premiers jours

1. **Une correction acceptée en amont.** Une phrase fausse, un lien mort, une
   commande qui a changé. C'est petit, et c'est la vraie leçon : on découvre
   le processus de contribution d'un projet, qui est la moitié du métier.
2. **Une page de référence manquante.** Prenez une fonction publique
   documentée par sa seule signature et écrivez ce qui manque.
3. **Un tutoriel, rejoué.** Écrivez-le, puis exécutez-le sur une machine
   propre. L'écart entre les deux, c'est ce que vous avez supposé sans le
   dire.
4. **Un changelog.** Le format le plus ingrat, et le plus utile.

## Ce qui fait revenir un travail

Trois choses, dans cet ordre de fréquence :

- **l'exemple ne tourne pas.** Versions de dépendances absentes, un import
  manquant, une sortie qui a changé. Un exemple que personne ne peut exécuter
  coûte plus cher que pas d'exemple.
- **un prérequis n'a pas été annoncé.** Le lecteur arrive à l'étape 4 avec
  quelque chose que l'auteur savait déjà. Écrivez les prérequis en haut, et
  ne supposez rien d'autre ensuite.
- **la page ne dit pas à qui elle s'adresse.** Une ligne d'ouverture disant
  pour qui c'est et ce qu'on aura à la fin vaut trois paragraphes.

## Où aller ensuite

- Le canal `#comm-tech-writer` sur Discord.
- Les grilles de relecture sont publiques : lisez celle de votre famille avant
  de soumettre, pas après.
- Write the Docs (writethedocs.org) : la communauté du métier, et son Slack.
$md$, 310),

('onboarding-communication-advocacy', 'onboarding', 'communication', 'advocacy', 'fr',
 'Débuter en prise de parole et en contenu',
 'Un public qui peut partir : ce que ça change, et comment tenir la promesse d''un titre.',
$md$
# Débuter en prise de parole et en contenu

La différence avec la documentation tient en une phrase : votre lecteur était
bloqué et n'avait pas le choix, votre public a le choix et peut partir. Tout
en découle.

## La promesse

Le titre est un contrat. Il dit ce qu'il y a dedans, et c'est dedans. Un titre
qui promet plus que le contenu ne vous coûte pas cette fois-ci : il vous coûte
la fois d'après, et c'est la seule monnaie de ce métier.

## Les trente premiers jours

1. **Un article de fond.** Écrivez avant de filmer. Une conférence non écrite
   est une conférence qui erre.
2. **Une démonstration enregistrée de dix minutes.** Une seule chose montrée,
   du début à la fin, sans coupe magique.
3. **Une proposition de conférence.** Deux cents mots pour un comité :
   pourquoi ce sujet, pourquoi cette salle, pourquoi vous. Envoyez-la même en
   attendant un non — l'écrire est déjà l'exercice.
4. **Une conférence donnée.** Un meetup local compte, et vaut mieux qu'une
   grosse conférence l'année prochaine.

## La démonstration en direct

Préparez le moment où ça casse, parce que ça cassera :

- environnement figé — versions verrouillées, dépendances installées, pas de
  `npm install` devant la salle ;
- une capture d'écran ou un enregistrement de chaque étape en secours ;
- pas besoin du réseau, ou un plan qui marche sans ;
- une répétition complète à voix haute, chronométrée.

## Ce qui fait revenir un travail

- **le son.** C'est la première raison de fermer une vidéo, avant l'image et
  avant le contenu. Un micro correct et une pièce avec des rideaux valent
  mieux qu'une caméra chère dans un salon vide.
- **du code illisible.** Augmentez la taille de police. Ce qui est lisible sur
  votre écran ne l'est ni au fond de la salle ni sur un téléphone.
- **le plateau.** Dix minutes où rien de nouveau n'est dit. Coupez.
- **les commentaires abandonnés.** Les questions font partie de la livraison.

## Où aller ensuite

- `#comm-devrel` et `#comm-content-creator` sur Discord.
- DevRel Collective : la communauté du métier.
- Les appels à conférenciers ouverts sont listés dans les opportunités du
  domaine.
$md$, 320),

('onboarding-communication-translation', 'onboarding', 'communication', 'translation', 'fr',
 'Débuter en traduction technique',
 'Tenir un vocabulaire sur des milliers de lignes, et savoir ce qui ne se traduit pas.',
$md$
# Débuter en traduction technique

Traduire de la matière technique, ce n'est pas transposer des phrases. C'est
prendre une série de décisions de vocabulaire et s'y tenir sur des milliers de
lignes, y compris quand on ne se souvient plus de ce qu'on a décidé.

## Ce qui ne se traduit pas

Décidez-le au départ et écrivez-le :

- les noms d'API, les mots-clés du langage, les noms de commandes ;
- les messages d'erreur que le programme affiche lui-même — le lecteur va les
  coller dans un moteur de recherche ;
- les noms de projets.

Le piège est dans l'autre sens : des termes qui ont déjà une traduction admise
dans la langue cible, mais pas celle que vous auriez choisie. Regardez ce que
font les autres projets avant de trancher seul.

## Les trente premiers jours

1. **Une page courte, terminée.** Une page finie vaut mieux que dix à moitié.
2. **Le glossaire.** Dès la deuxième page, notez les termes tranchés et
   pourquoi. C'est ce qui rend la suite tenable et relisable.
3. **Une relecture par quelqu'un d'autre.** Dans les deux langues. C'est la
   règle de la famille : une traduction n'est validée que par une personne qui
   lit les deux.
4. **Une contribution au pipeline i18n.** Une phrase concaténée dans la source
   est une phrase intraduisible dans la moitié des langues du monde.

## Les langues peu outillées

Si vous traduisez vers une langue sans vocabulaire technique établi — wolof,
lingala, bambara, et beaucoup d'autres — vous ne traduisez pas, vous forgez.
Alors documentez chaque création : le terme, ce qu'il rend, pourquoi ce choix.
Ce document vaut autant que la traduction.

## Ce qui fait revenir un travail

- **deux traductions d'un même terme.** Rend la version traduite plus dure que
  l'originale.
- **le calque de construction.** Ça se voit dès la première phrase.
- **la version source non consignée.** Sans elle, un mainteneur ne peut pas
  savoir ce qui reste à refaire quand l'original bouge.

## Où aller ensuite

- `#comm-translation` et ses sous-canaux par langue.
- Weblate et Crowdin hébergent gratuitement les projets libres.
$md$, 330),

('onboarding-communication-research-writing', 'onboarding', 'communication', 'research-writing', 'fr',
 'Débuter en écriture de recherche',
 'Un texte dont la valeur tient à sa méthode : ce qui a été mesuré, comment, et ce que ça ne prouve pas.',
$md$
# Débuter en écriture de recherche

Un livre blanc, un rapport sectoriel et une spécification externe ont une
chose en commun : leur valeur ne tient pas à la prose mais à ce qu'un lecteur
peut vérifier.

## La structure qui marche

1. **La question.** Ce qui est demandé, et pourquoi ça se pose maintenant.
2. **Ce qui existe déjà.** Lu, cité, situé. Annoncer du neuf sans regarder
   l'ancien est la faute la plus fréquente et la plus coûteuse.
3. **La méthode.** Assez précise pour qu'un tiers la rejoue : protocole,
   données, versions, matériel.
4. **Les résultats.** Avec leurs incertitudes, et les cas défavorables aussi
   complètement que les favorables.
5. **Les limites.** Écrites par vous. Un document sans section « limites » est
   une plaquette.

## Les trente premiers jours

1. **Un état de l'art de deux pages** sur un sujet que vous croyez connaître.
   Vous découvrirez que quelqu'un l'a déjà écrit.
2. **Une mesure reproduite.** Prenez un résultat publié, rejouez-le, écrivez
   ce que vous avez trouvé — y compris quand c'est la même chose.
3. **Un livre blanc court.** Quinze pages avec une vraie méthode valent mieux
   que quarante sans.

## Les citations, et l'outil qui écrit à votre place

Toute affirmation empruntée porte une référence atteignable. Un lien mort est
une citation manquante.

Les challenges d'écriture de recherche de cette plateforme sont réglés sur
`human_verified`, et c'est la seule famille dans ce cas. La raison est
précise : une référence inventée par un modèle de langage est indiscernable
d'une vraie pour un lecteur qui fait confiance au document, et toute la valeur
de l'écriture de recherche est que ses sources se suivent. Utilisez l'outil si
vous voulez ; ouvrez chacun des liens qu'il vous donne.

## Conflits d'intérêts

Financement, employeur, produit évalué : déclarés en haut. Un rapport
sectoriel payé par un acteur de ce secteur se lit autrement, et le lecteur y a
droit.

## Où aller ensuite

- `#comm-research` sur Discord.
- Zotero pour les références, Overleaf si vous passez à un format académique.
$md$, 340);

-- ═══════════════════════════════════════════════════════════════════
-- Toolkit
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES
('toolkit-communication', 'toolkit', 'communication', NULL, 'fr',
 'Boîte à outils de la communication technique',
 'Ce qui suffit pour commencer, et ce qui ne coûte rien.',
$md$
# Boîte à outils de la communication technique

Tout ce qui suit est gratuit ou dispose d'un palier gratuit réellement
utilisable. Rien n'est nécessaire pour commencer : un éditeur de texte et un
micro correct couvrent les six premiers mois.

## Écrire

- **Vale** — linter de prose configurable. Le seul outil de cette liste que
  les gens regrettent de ne pas avoir adopté plus tôt : il attrape les
  incohérences de vocabulaire que la relecture humaine laisse passer.
- **markdownlint**, **lychee** (liens morts) — branchez-les dans le pipeline.
- **LanguageTool** — grammaire et style, logiciel libre, utilisable hors
  ligne.
- **Obsidian**, **Zettlr**, **VS Code** — écrire du Markdown avec ses notes à
  côté.

## Publier

- **MkDocs Material**, **Docusaurus**, **mdBook**, **Sphinx** — générateurs de
  sites de documentation. Le premier demande le moins de décisions.
- **Hugo**, **Zola**, **Eleventy** — pour un blog personnel qui compilera
  encore dans dix ans.

## Parler et montrer

- **Reveal.js**, **Slidev**, **Marp** — des diapositives écrites en Markdown,
  donc versionnées avec le reste.
- **OBS Studio** — enregistrement et diffusion. Logiciel libre, et la
  référence.
- **Asciinema** — enregistrer un terminal en texte plutôt qu'en vidéo :
  léger, copiable, lisible sur un téléphone.

## Monter

- **DaVinci Resolve** — montage professionnel, version gratuite complète.
- **Shotcut**, **Kdenlive** — logiciels libres, plus légers.
- **Audacity** — nettoyer une piste voix.
- **Whisper** (openai-whisper, whisper.cpp) — sous-titres automatiques à
  relire. Ne publiez jamais des sous-titres que personne n'a relus.

## Traduire

- **Weblate** — logiciel libre, hébergement gratuit pour les projets libres.
- **Crowdin** — gratuit pour l'open source.
- **Poedit**, **OmegaT** — hors ligne, pour les fichiers PO et les mémoires de
  traduction.

## Chercher et citer

- **Zotero** — gestion de références, logiciel libre.
- **Overleaf**, **Typst** — pour un format académique.
- **OpenAlex**, **Semantic Scholar** — trouver ce qui existe déjà, sans
  péage.

## Le son, qui compte plus que le reste

Un micro-casque USB correct et une pièce avec des rideaux valent mieux qu'une
caméra chère dans une pièce vide. C'est le seul poste où cent euros changent
quelque chose que tout le monde entend.
$md$, 350);

-- ═══════════════════════════════════════════════════════════════════
-- Briefs
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('brief-communication-docs', 'brief_template', 'communication', 'documentation', 'fr',
 'Brief — documentation',
 'À remplir avant de commander de la documentation. Sans ces réponses, il n''y a pas de commande.',
$md$
# Brief — documentation

## Le lecteur
- Qui est-il ? (niveau, rôle, ce qu'il sait déjà)
- Qu'essaie-t-il de faire quand il arrive sur cette page ?
- Qu'aura-t-il fait en la quittant ?

## Le type de page
- [ ] Tutorial (il apprend) · [ ] How-to (il a une tâche) ·
  [ ] Reference (il cherche un fait) · [ ] Explanation (il veut comprendre)

## Périmètre
- Ce qui est dedans :
- Ce qui est explicitement dehors :
- Longueur visée :

## Contexte technique
- Version documentée :
- Où vit le code d'exemple :
- Qui répond aux questions techniques :

## Livraison
- Où la page est publiée :
- Date :
- Qui valide :

## Droits
- Signature de l'auteur : oui / non
- Licence de la page :
- L'auteur peut-il la montrer dans son portfolio : oui / non
$md$, 360),

('brief-communication-talk', 'brief_template', 'communication', 'advocacy', 'fr',
 'Brief — conférence',
 'À remplir avant de commander une conférence, un atelier ou une démonstration.',
$md$
# Brief — conférence

## L'événement
- Nom, date, lieu (ou en ligne) :
- Public attendu : combien, quel niveau, ce pour quoi il est venu
- Durée exacte, questions comprises :

## Le propos
- Ce que le public doit avoir compris en sortant, en une phrase :
- Ce qu'il doit savoir faire en sortant :
- Sujets à éviter :

## La démonstration
- Y en a-t-il une ? Sur quoi ?
- Réseau disponible ? Débit ?
- Qui fournit l'environnement ?

## Livraison
- Supports attendus, et dans quel format :
- Captation : par qui, publiée où, sous quelle licence
- Répétition avec l'organisateur : date

## Conditions
- Honoraires, et ce qu'ils couvrent (préparation comprise ou non) :
- Déplacement et hébergement : pris en charge ou non
- Exclusivité demandée sur le contenu : oui / non, pour combien de temps
$md$, 370),

('brief-communication-video', 'brief_template', 'communication', 'advocacy', 'fr',
 'Brief — contenu vidéo',
 'À remplir avant de commander une vidéo, une série ou un épisode.',
$md$
# Brief — contenu vidéo

## Le format
- [ ] Tutoriel · [ ] Démonstration · [ ] Entretien · [ ] Direct · [ ] Série
- Durée visée :
- Nombre d'épisodes, et cadence :

## Le sujet
- Ce qui est montré, précisément :
- Ce que le spectateur sait faire à la fin :
- Niveau supposé au départ :

## Production
- Qui écrit le script ? Est-il validé avant tournage ?
- Qui fournit le code montré, et où vit-il ?
- Voix, visage à l'image, ou capture d'écran seule ?
- Sous-titres : dans quelles langues ?

## Publication
- Quelle chaîne ? (la vôtre, celle de l'auteur, les deux)
- Signature de l'auteur, et où elle apparaît :
- Mention de partenariat rémunéré : elle est obligatoire — où la mettez-vous ?

## Livraison
- Fichiers rendus plus sources : oui / non
- Date, et nombre de tours de retouches inclus :
$md$, 380),

('brief-communication-translation', 'brief_template', 'communication', 'translation', 'fr',
 'Brief — traduction',
 'À remplir avant de commander une traduction technique.',
$md$
# Brief — traduction

## Langues
- Langue source :
- Langue(s) cible(s) :
- Variante régionale exigée ? (pt-BR ou pt-PT, fr-FR ou fr-CA…)

## Le contenu
- Quoi exactement, avec un nombre de mots ou de segments :
- Version source à traduire (commit, tag, date) :
- Format de fichier :

## Vocabulaire
- Glossaire existant ? Où ?
- Termes à ne pas traduire :
- Décisions déjà prises et à respecter :

## Relecture
- Qui relit ? Cette personne lit-elle les deux langues ?
- La relecture fait-elle partie de cette commande ou d'une autre ?

## Livraison
- Où déposer (dépôt, plateforme de traduction) :
- Le glossaire utilisé est-il livré avec ? (il devrait l'être)
- Date, et ce qui se passe si la source bouge entre-temps :
$md$, 390),

('brief-communication-research', 'brief_template', 'communication', 'research-writing', 'fr',
 'Brief — écriture de recherche',
 'À remplir avant de commander un livre blanc, un rapport ou une spécification.',
$md$
# Brief — écriture de recherche

## La question
- La question à laquelle le document répond, en une phrase :
- Pour qui, et pour quelle décision :

## Périmètre
- Ce qui est étudié :
- Ce qui est explicitement hors périmètre :
- Longueur visée :

## Méthode
- Y a-t-il des mesures à produire ? Sur quoi ?
- Qui fournit les données, et sous quelle forme ?
- L'auteur peut-il publier la méthode et les données ? (sinon, dites-le
  maintenant : ça change ce que le document a le droit d'affirmer)

## Indépendance
- Le commanditaire est-il un acteur du domaine étudié ?
- Ce lien sera indiqué dans le document. Où ?
- Le commanditaire peut-il demander le retrait d'un résultat défavorable ?
  (la seule bonne réponse est non, et elle s'écrit avant, pas après)

## Livraison
- Format, licence, où les données sont déposées :
- Date, tours de relecture compris, qui valide :
$md$, 400);

-- ═══════════════════════════════════════════════════════════════════
-- Writeup templates
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

('writeup-comm-docs-contribution', 'writeup_template', 'communication', 'documentation', 'fr',
 'Contribution documentation',
 'Le compte rendu d''une contribution acceptée en amont.',
$md$
# Contribution documentation — {projet}

**Lien vers la contribution :**
**Fusionnée le :**

## Ce qui manquait
Pour qui, et ce que cette personne ne pouvait pas faire.

## Ce que j'ai écrit
Trois lignes. Le lien fait le reste.

## Ce que la relecture a changé
Ce que les mainteneurs ont demandé, et ce que j'en ai appris.

## Ce que je ferais autrement
$md$, 410),

('writeup-comm-tutorial', 'writeup_template', 'communication', 'documentation', 'fr',
 'Tutoriel pas à pas',
 'Le squelette d''un tutoriel qu''on peut suivre sans se bloquer.',
$md$
# {Titre : ce que le lecteur saura faire}

**Pour qui :**
**Temps nécessaire :**
**Ce que vous aurez à la fin :** (une capture, une sortie, une adresse qui
répond)

## Prérequis
Tout ce qui doit être installé ou su. Rien d'autre n'est supposé ensuite.

## Étape 1 — {verbe}
Ce qu'on fait, la commande, et ce qui doit apparaître.

## Étape 2 — …

## Vérifier que ça a marché
Comment le lecteur sait qu'il a réussi.

## Si ça n'a pas marché
Les deux ou trois erreurs que les gens rencontrent vraiment, et leur cause.

## Où aller ensuite
Un lien, pas cinq.
$md$, 420),

('writeup-comm-api-reference', 'writeup_template', 'communication', 'documentation', 'fr',
 'Entrée de référence d''API',
 'Le squelette d''une entrée de référence.',
$md$
# `{signature}`

Une phrase : ce que ça fait.

## Paramètres
| Nom | Type | Requis | Défaut | Description |
|---|---|---|---|---|

## Retour
Type, et ce que la valeur signifie.

## Erreurs
| Condition | Erreur levée |
|---|---|

## Exemple
Copiable, exécutable, avec sa sortie.

## Notes
Cas limites, comportement en concurrence, dépréciations. La version à partir
de laquelle c'est vrai.
$md$, 430),

('writeup-comm-talk-outline', 'writeup_template', 'communication', 'advocacy', 'fr',
 'Plan de conférence',
 'Le squelette d''une conférence, avant toute diapositive.',
$md$
# {Titre}

**Durée :** · **Public :** · **Ce qu'il en retire :**

## L'accroche (2 min)
Le problème, montré plutôt qu'énoncé.

## La promesse (1 min)
Ce que la salle saura à la fin.

## Le corps (15 min)
- Idée 1 → démonstration → conséquence
- Idée 2 → démonstration → conséquence
- Idée 3 → démonstration → conséquence

## La démonstration
Ce qui tourne, et le plan B si ça casse.

## La clôture (2 min)
Une chose à faire en sortant. Les liens.

## Questions attendues
Les trois qu'on va me poser, et mes réponses.
$md$, 440),

('writeup-comm-blog-tutorial', 'writeup_template', 'communication', 'advocacy', 'fr',
 'Article — tutoriel',
 'Le squelette d''un article qui apprend quelque chose.',
$md$
# {Titre : le résultat, pas la technologie}

**Ce que vous saurez faire à la fin :**
**Prérequis :**
**Code complet :** {lien}

## Le problème
Une situation concrète, pas une abstraction.

## La solution, étape par étape
Chaque bloc de code est complet et exécutable.

## Ce que ça donne
La sortie, la capture, la mesure.

## Limites
Ce que cette approche ne fait pas.

## Pour aller plus loin
$md$, 450),

('writeup-comm-blog-deep-dive', 'writeup_template', 'communication', 'advocacy', 'fr',
 'Article — analyse de fond',
 'Le squelette d''un article qui explique pourquoi.',
$md$
# {Titre}

## Pourquoi ce sujet maintenant

## Ce qu'on croit généralement
Et ce qui est vrai là-dedans.

## Ce qui se passe réellement
Le mécanisme, avec sources et mesures.

## Conséquences pratiques
Ce que ça change pour quelqu'un qui écrit du code demain.

## Ce que je ne sais pas
La section qui sépare une analyse d'une opinion.

## Sources
Chaque lien atteignable.
$md$, 460),

('writeup-comm-video-script', 'writeup_template', 'communication', 'advocacy', 'fr',
 'Script vidéo',
 'Ce qui est dit, et ce que l''image montre pendant qu''on le dit.',
$md$
# {Titre} — script

**Durée visée :** · **Format :**

## Accroche (0:00–0:15)
| Voix | Image |
|---|---|

## Annonce (0:15–0:45)
Ce que la vidéo va montrer.

## Corps
| Voix | Image | Durée |
|---|---|---|

## Clôture
Ce qu'on retient, et le lien.

## Notes de tournage
Ce qui doit être préparé avant d'appuyer sur enregistrer.
$md$, 470),

('writeup-comm-podcast-outline', 'writeup_template', 'communication', 'advocacy', 'fr',
 'Plan d''épisode de podcast',
 'Le squelette d''un épisode, en entretien ou en solo.',
$md$
# {Titre de l'épisode}

**Invité :** · **Durée visée :**

## Pourquoi cet épisode maintenant

## Ce que l'auditeur sait à la fin

## Questions
1. La question d'ouverture, large.
2. …
Écoutez la réponse plutôt que votre question suivante. Laissez le silence
travailler.

## Ce qu'il ne faut pas oublier
Les questions que je regretterais de ne pas avoir posées.

## Notes d'épisode
Tout ce qui est cité, avec son lien.
$md$, 480),

('writeup-comm-translation-style', 'writeup_template', 'communication', 'translation', 'fr',
 'Guide de style de traduction',
 'Les décisions de vocabulaire, écrites une fois.',
$md$
# Guide de style — {langue cible}

**Projet :** · **Version source :**

## Registre
Vouvoiement ou tutoiement, et pourquoi. Tournures impersonnelles ou non.

## Ce qui ne se traduit pas
Noms d'API, mots-clés, messages d'erreur du programme, noms de projets.

## Glossaire
| Terme source | Traduction retenue | Écartée | Pourquoi |
|---|---|---|---|

## Conventions
Dates, nombres, unités, guillemets, espaces insécables.

## Questions ouvertes
Ce qui reste à trancher, avec les options.
$md$, 490),

('writeup-comm-whitepaper', 'writeup_template', 'communication', 'research-writing', 'fr',
 'Livre blanc',
 'La structure d''un texte dont la valeur tient à sa méthode.',
$md$
# {Titre}

**Auteur :** · **Date :** · **Version :**
**Intérêts déclarés :** (financement, employeur, produit évalué)

## Résumé
Un paragraphe : la question, la réponse, et la confiance qu'elle mérite.

## La question
Ce qui est demandé, et pourquoi maintenant.

## Ce qui existe déjà
Lu, cité, situé.

## Méthode
Protocole, données, versions, matériel. Assez pour être rejoué.

## Résultats
Avec leurs incertitudes. Les cas défavorables aussi.

## Limites
Ce que ce document ne prouve pas.

## Conclusion

## Sources
$md$, 500);
