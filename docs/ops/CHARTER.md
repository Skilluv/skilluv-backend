# Charte Skilluv Ops

Le domaine ops est celui où une erreur ne se rattrape pas en réécrivant une
fonction. Un cluster mal configuré, une migration mal jouée, une astreinte mal
tenue : ce qui casse est en production, chez quelqu'un, tout de suite.

Cette charte dit ce que Skilluv attend d'une personne qui travaille sur ce
domaine, et ce que Skilluv lui doit en retour.

---

## 1. Les huit métiers

| Métier | Ce qu'il fait |
|---|---|
| DevOps Engineer | CI/CD, conteneurs, infrastructure comme code |
| Platform Engineer | La plateforme interne sur laquelle les autres livrent |
| Spécialiste Kubernetes | Opérateurs, maillage de services, GitOps |
| SRE | Objectifs de service, budgets d'erreur, résilience |
| Responsable d'incident | Conduit la réponse, écrit le post-mortem |
| Architecte cloud | Conception, coûts, multi-région |
| Ingénieur observabilité | Métriques, journaux, traces, et ce qui les relie |
| Administrateur de bases de données | Réplication, réglage, reprise |

Cinq familles de relecture les regroupent, parce que celui qui lit un plan
Terraform lit un chart Helm et n'a pas d'avis utile sur un plan de requête.

## 2. Ce qui compte comme preuve

Pas une pull request. Dans ce domaine, la preuve est :

- **un artefact réutilisable** — un module, un chart, un pipeline, un tableau
  de bord, un runbook. Jugé sur une question : quelqu'un d'autre peut-il s'en
  servir sans l'auteur dans la pièce ;
- **un objectif tenu** — une cible annoncée, une fenêtre, un chiffre atteint,
  et la source du chiffre ;
- **un incident conduit** — avec les deux durées et un post-mortem publié ;
- **une réduction de coûts** — les deux montants, ce qui a été changé, et la
  confirmation que le service tient toujours.

Chacune de ces preuves est enregistrée avec ce sur quoi elle repose. « Fiable »
n'est pas une preuve ; « 99,95 % sur quatre-vingt-dix jours, tableau de bord
public » en est une.

## 3. Les post-mortems sont sans blâme, et c'est une contrainte

Il n'existe aucune colonne, nulle part, pour enregistrer qui a causé un
incident. Ce n'est pas une politique éditoriale : le schéma ne le permet pas.

La raison est pratique et pas morale. Un post-mortem qui nomme quelqu'un est
un post-mortem que personne n'écrit honnêtement la fois suivante, et c'est la
fois suivante qui compte. Ce qui est enregistré est ce que le système a permis.

Deux exigences :

- **deux cents caractères minimum**. Un post-mortem plus court est un titre, et
  la deuxième occurrence du même incident est ce qu'il coûte ;
- **au moins une action de suivi**. Un post-mortem qui ne conclut rien à faire
  a soit trouvé un système qui ne peut plus tomber, soit pas cherché.

Les actions promises et en retard sont visibles. C'est ce qui sépare une
pratique de post-mortem d'une archive de post-mortems.

## 4. Le coût est une compétence

Réduire une facture de 60 % est un travail d'ingénierie, au même titre que
tenir un objectif de disponibilité. Skilluv l'atteste, à une condition : que
quelqu'un ait vérifié que le service tient toujours.

Une réduction de coûts qui a cassé le service est une panne avec un tableur.
La vérification porte sur les deux moitiés ou sur aucune.

## 5. Ce que Skilluv attend

**La sécurité par défaut.** Un module qui ouvre un port par commodité, un
secret dans un dépôt, un rôle trop large « en attendant » : ces trois-là sont
refusés en relecture, sans discussion sur le contexte.

**Le respect de ce qui a été promis.** Un objectif annoncé est une promesse à
quelqu'un. Ne pas le tenir arrive et se dit ; le redéfinir après coup pour
qu'il soit tenu ne se fait pas.

**Ce qui tourne se documente.** Un runbook n'est pas de la documentation
d'accompagnement, c'est le livrable. Le test est celui de la section 2 :
quelqu'un d'autre, à trois heures du matin, sans l'auteur.

## 6. Ce que Skilluv doit

**Un accès délimité.** Une mission ops donne accès à une infrastructure de
production. Cet accès est temporaire, tracé, et retiré à la fin — pas quand
quelqu'un y pense.

**Une astreinte payée.** Être joignable est du travail. Une mission qui inclut
de l'astreinte le dit et le paye ; une qui ne le dit pas ne l'inclut pas.

**Pas de responsabilité sans autorité.** Personne ne porte un objectif de
disponibilité sur un système qu'il n'a pas le droit de changer. Une mission
qui demande l'un sans l'autre est refusée à la relecture du brief.

**La confidentialité, dans les deux sens et bornée.** Une topologie réseau
apprise en mission ne se raconte pas. Ce que Skilluv exige d'un contributeur,
Skilluv le tient sur lui.

---

## 7. Ce qu'il reste à faire

- rédiger les NDA renforcés propres aux missions avec accès production, et
  les contrats d'astreinte (ticket L-01) ;
- publier les grilles de relecture des cinq familles ;
- faire relire les deux par un juriste.

Aucune mission ops avec accès production ne sera ouverte avant que ces
documents existent.
