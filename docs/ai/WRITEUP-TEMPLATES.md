# Modèles de compte rendu — domaine IA

Dix modèles. Ce qu'on écrit **autour** d'un artefact décide s'il est
utilisable par quelqu'un d'autre — et un artefact que personne d'autre ne peut
reprendre n'est pas opposable.

Chaque modèle liste des sections. Une section qu'on ne peut pas remplir est
une information qui manque, pas une section à supprimer : écrire « non
mesuré » est un résultat, l'omettre n'en est pas un.

---

## 1. `model-card.md`

Compatible avec le format de fiche de HuggingFace, pour que la même fiche
serve ici et là-bas.

- **Ce que fait le modèle**, en une phrase, et pour qui.
- **Usage prévu**, et **usages explicitement déconseillés**.
- **Données d'entraînement** : source, volume, période, licence, prétraitement.
- **Procédure d'entraînement** : matériel, durée, hyperparamètres, graine.
- **Évaluation** : jeu de test, métriques, référence de comparaison.
- **Performance par sous-population**, quand la notion s'applique.
- **Limites connues** : ce sur quoi il échoue, écrit par vous.
- **Empreinte** : coût de calcul de l'entraînement, coût d'une inférence.
- **Licence** du modèle et licences amont respectées.
- **Comment citer**.

## 2. `dataset-card.md`

- **Ce que contient le jeu**, et ce qu'il ne contient pas.
- **Provenance** : d'où viennent les données, comment elles ont été obtenues.
- **Consentement et données personnelles** : base légale, anonymisation.
- **Composition** : taille, distribution des classes, langues, période.
- **Protocole d'annotation** : consignes, nombre d'annotateurs, accord.
- **Découpages fournis** et selon quel axe.
- **Biais connus** : ce qui est sur-représenté et sous-représenté.
- **Licence** et conditions de réutilisation.
- **Maintenance** : qui corrige une erreur signalée, et où la signaler.

## 3. `experiment-report.md`

- **Question** : ce qu'on cherchait à savoir, écrit avant de lancer.
- **Hypothèse** et ce qui l'aurait réfutée.
- **Protocole** : variantes comparées, ce qui varie et ce qui est figé.
- **Résultats** : le tableau, avec les écarts-types sur plusieurs graines.
- **Interprétation** : ce que les chiffres permettent de conclure et ce
  qu'ils ne permettent pas.
- **Ce qui n'a pas marché.** La section la plus utile aux suivants, et la
  première à disparaître quand on écrit après coup.

## 4. `benchmark-report.md`

Ce qui rend un résultat rejouable par un tiers.

- **Banc** : nom, version, jeu de données et découpage exact.
- **Métrique**, unité, et sens de l'amélioration.
- **Références de comparaison** avec leurs sources.
- **Harnais** utilisé et sa version — `lm-evaluation-harness`, `criterion`.
- **Matériel** : machine, carte, quantité de mémoire.
- **Méthode** : chauffe, nombre d'itérations, ce qui est chronométré.
- **Commande exacte** à relancer.
- **Écart attendu** entre deux exécutions.

## 5. `paper-abstract.md`

- **Contexte** en deux phrases.
- **Ce que ce travail ajoute** à ce qui est déjà publié.
- **Méthode**, assez pour comprendre sans lire le code.
- **Résultat principal**, chiffré.
- **Limites**, avant qu'un relecteur ne les trouve.
- **Code et données** : les adresses.

## 6. `rag-system-design.md`

- **Corpus** : quoi, combien, à quelle fréquence il change.
- **Découpage** : taille des fragments, chevauchement, et pourquoi.
- **Récupération** : lexicale, dense, hybride ; le reclasseur s'il y en a un.
- **Ablation** : ce que chaque étage apporte, mesuré. Sans ça, personne ne
  sait si le reclasseur sert à quelque chose.
- **Jeu d'évaluation** : les questions, et les cas d'échec choisis exprès.
- **Comportement quand rien n'est trouvé.**
- **Coût et latence** par requête.

## 7. `agent-system-design.md`

- **Tâche** et condition d'arrêt.
- **Outils** exposés, et leurs limites de permission.
- **État partagé** : ce qui circule entre les agents.
- **Boucles** : ce qui empêche une exécution infinie.
- **Bac à sable** : ce que l'agent ne peut pas atteindre.
- **Évaluation** : taux de réussite, coût moyen, trace d'une exécution
  complète.
- **Reprise après échec** : ce qui se passe quand un outil renvoie une erreur.

## 8. `ai-safety-red-team-report.md`

Le format attendu par la [politique de divulgation](./SAFETY-DISCLOSURE.md).

- **Cible** : modèle, version ou date d'instantané, mode d'accès.
- **Type d'attaque** et pourquoi ce choix.
- **Reproduction** : la procédure, assez précise pour un tiers.
- **Sortie observée**, verbatim.
- **Taux** : réussites sur essais.
- **Gravité** et le raisonnement qui y mène.
- **Atténuation proposée.**
- **Chronologie de divulgation** : notification, accusé de réception, délai
  convenu, publication.
- **Double usage** : ce qui est retenu, et pourquoi.

## 9. `deployment-runbook.md`

- **Ce qui est déployé** : version du modèle, empreinte, date.
- **Mise en service** : commandes, dépendances, secrets nécessaires.
- **Retour arrière** : la commande, et le temps qu'elle prend.
- **Surveillance** : métriques suivies, seuils, qui est alerté.
- **Dérive** : ce qui la déclenche, et l'action associée.
- **Capacité** : ce que la machine actuelle peut servir.
- **Pannes connues** et leur remède.

## 10. `post-mortem-ai-incident.md`

Sans nommer de coupable. Un post-mortem qui cherche un responsable ne produit
plus d'information dès la deuxième fois.

- **Ce qui s'est passé**, et ce que les utilisateurs ont vu.
- **Chronologie** : du premier symptôme au rétablissement.
- **Cause** : technique, et ce qui l'a rendue possible.
- **Détection** : comment on l'a su, et combien de temps après.
- **Ce qui a limité les dégâts.**
- **Ce qui les a aggravés.**
- **Actions** : chacune avec un porteur et une échéance.

---

*Voir aussi : la [charte du domaine](./CHARTER.md) et les [modèles de
brief](./BRIEF-TEMPLATES.md).*
