# Modèles de brief — domaine IA

Six modèles, un par nature d'artefact. À utiliser pour rédiger l'énoncé d'un
challenge IA.

Un brief mal écrit produit des livrables incomparables entre eux : chacun
répond à une question différente, et le relecteur arbitre au jugé. C'est pire
en IA qu'ailleurs, parce qu'un chiffre a l'air comparable même quand il ne
l'est pas.

**La famille détermine la grille de revue appliquée** (`reviewer_group` sur
l'orientation). Écrire un brief dans la mauvaise famille, c'est promettre une
grille et en appliquer une autre.

---

## Structure commune

Tout brief IA comporte ces sept sections, dans cet ordre.

### 1. Problème

Ce qui ne va pas aujourd'hui, **du point de vue de quelqu'un que ça gêne**.
Pas la solution attendue, et surtout pas l'architecture.

> Mauvais : « Entraîner un classifieur BERT. »
> Bon : « Le support trie douze cents messages par jour à la main et met
> quatre heures à repérer les urgences. »

### 2. Données disponibles

Ce qui existe, dans quel état, sous quelle licence, avec quel volume. Si le
brief ne fournit pas de données, il dit où en trouver de recevables.

Préciser aussi ce qui est **interdit** : aspirer un site, réutiliser un jeu
sous licence non commerciale pour un livrable commercial.

Une section « données » vide produit un livrable dont personne ne peut juger
la provenance.

### 3. Référence de comparaison

Contre quoi le résultat se mesure. Une prévision naïve, une règle métier
existante, un modèle publié, la version en production.

**Obligatoire.** Un brief sans référence demande un chiffre sans second terme,
et tout le monde le réussit.

### 4. Métriques de succès

La métrique, l'unité, le sens (plus haut vaut mieux ou l'inverse), et le seuil
à atteindre. Chiffré.

> Mauvais : « Le modèle doit être bon. »
> Bon : « F1 macro ≥ 0,78 sur le jeu de test fourni, contre 0,61 pour la
> règle actuelle. »

Dire aussi ce qui est mesuré **en plus de** la qualité : latence, mémoire,
coût par mille requêtes. Un modèle qui ne tient pas dans la cible n'est pas
fini.

### 5. Cible de déploiement

Où le résultat doit tourner : un processeur, une carte à huit gigaoctets, une
carte embarquée, une API tierce. Cette section change tout le reste et elle
est presque toujours oubliée.

### 6. Éthique et provenance

Ce qu'il faut regarder pour ce sujet précis : sous-populations à évaluer
séparément, données personnelles, usage détourné plausible, licences amont à
respecter.

Une case à cocher ne suffit pas. Si le brief ne sait pas nommer le risque du
sujet, il n'est pas prêt.

### 7. Hors périmètre

Ce qui n'est explicitement pas demandé. C'est la section qui évite qu'un
candidat passe trois semaines sur une interface web dont personne n'a parlé.

---

## 1. `brief-data-pipeline.md`

Ajouter à la structure commune :

- **Sources et fraîcheur attendue** : d'où vient la donnée, à quelle fréquence
  elle doit arriver, quel retard est tolérable.
- **Sémantique de livraison** : au plus une fois, au moins une fois, exactement
  une fois. Dire laquelle est exigée — pas « fiable ».
- **Volume et croissance** : le volume d'aujourd'hui et celui prévu, sinon le
  travail est calibré pour l'échantillon.
- **Comportement en panne** : ce qui doit se passer si la source disparaît en
  milieu de chargement.
- **Budget** : octets scannés ou coût mensuel maximal. Un pipeline planifié
  coûte tous les jours.

Critères d'acceptation typiques : un rattrapage sur N jours qui ne duplique
rien, des contrôles de qualité qui arrêtent le pipeline, un plan de reprise
documenté.

## 2. `brief-ml-model.md`

Ajouter à la structure commune :

- **Découpage des jeux** : comment train, validation et test sont séparés, et
  selon quel axe. Un découpage aléatoire sur des données temporelles fabrique
  une fuite.
- **Déséquilibre des classes** : la distribution réelle, et la métrique
  choisie en conséquence.
- **Contraintes d'inférence** : latence maximale, mémoire, matériel.
- **Réentraînement** : à quelle fréquence, et avec quelles données.

Critères typiques : le seuil chiffré atteint sur le jeu de test, une
comparaison à la référence, un entraînement reproductible d'une machine à
l'autre.

## 3. `brief-llm-agent-system.md`

Ajouter à la structure commune :

- **Jeu d'évaluation** : les cas, dont des cas d'échec choisis exprès. Fourni
  par le brief, ou à construire — et alors c'est un livrable à part entière.
- **Outils accessibles** et ce que l'agent ne doit jamais pouvoir faire.
- **Comportement en cas d'ignorance** : dire qu'il ne sait pas, ou inventer.
  Le premier est le seul acceptable, et cela se teste.
- **Budget par exécution** : jetons, appels, temps de réponse.
- **Injection** : les entrées venant d'un tiers sont-elles possibles, et que
  doit-il en advenir.

Critères typiques : un taux de réussite sur le jeu d'évaluation, un taux de
refus correct sur les cas hors périmètre, un coût moyen mesuré.

## 4. `brief-cv-application.md`

Ajouter à la structure commune :

- **Conditions de prise de vue** : lumière, angle, résolution, flou. Un modèle
  entraîné sur des images propres échoue en production.
- **Composition du jeu d'images** et sous-populations à évaluer séparément.
- **Annotation** : fournie, ou à produire — et alors avec quel protocole et
  quel accord inter-annotateurs.
- **Cible matérielle** et images par seconde attendues.
- **Personnes** : si le sujet en montre, l'usage prévu et le consentement se
  traitent dans le brief, pas en revue.

Critères typiques : mAP ou mIoU chiffré, performance par sous-population,
débit mesuré sur le matériel visé.

## 5. `brief-nlp-service.md`

Ajouter à la structure commune :

- **Langues** couvertes, et le niveau attendu pour chacune. Une moyenne
  cache l'unique langue qui échoue.
- **Domaine du texte** : juridique, médical, conversationnel. Un modèle
  généraliste s'effondre en dehors.
- **Jeu annoté** : fourni ou à produire, avec le protocole d'annotation.
- **Métrique et sa limite** : BLEU, ROUGE et consorts se rapportent avec ce
  qu'ils ne mesurent pas.

Critères typiques : la métrique chiffrée **par langue** et par type d'entité,
plus une évaluation manuelle sur un échantillon.

## 6. `brief-ai-safety-evaluation.md`

Ajouter à la structure commune :

- **Cible et version exactes**, y compris la date d'instantané.
- **Périmètre autorisé** : ce qu'il est permis de tenter, et sur quels
  comptes. Un red-team hors périmètre est un incident, pas un livrable.
- **Nombre d'essais minimal** pour qu'un taux ait un sens.
- **Circuit de divulgation** : qui prévenir, dans quel délai, et qui décide en
  cas de double usage.
- **Ce qui ne se publie pas** : décidé avant de commencer, pas après avoir
  trouvé.

Critères typiques : un protocole rejouable par un tiers, un taux de réussite
sur N essais, une atténuation proposée, une divulgation conforme à la
[politique](./SAFETY-DISCLOSURE.md).

---

*Voir aussi : la [charte du domaine](./CHARTER.md).*
