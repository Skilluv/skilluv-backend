# Divulgation et recherche responsable en IA

*Destinée à être publiée sur `skill-uv.com/ai/disclosure`.*

Cette politique dit quoi faire d'une trouvaille avant de la publier. Elle
s'applique à tout travail mené sur Skilluv qui met en défaut un modèle ou un
système : contournement, injection, extraction de données, détournement
d'outil, biais mesuré.

Elle est contraignante. Une trouvaille publiée hors de ce cadre n'est pas
attestée, et la publier ainsi peut entraîner la révocation du travail.

---

## 1. Le principe

Prévenir d'abord la personne qui peut corriger, publier ensuite.

L'ordre n'est pas une politesse. Entre le moment où une attaque est écrite
publiquement et celui où elle est corrigée, elle est utilisable par n'importe
qui. Cet intervalle se réduit en prévenant en premier, et il s'allonge en
publiant en premier.

## 2. Les états

La plateforme suit chaque trouvaille par un état, et les transitions sont
contrôlées :

| État | Ce qu'il veut dire |
|---|---|
| `private` | Connue de l'auteur et des relecteurs seulement |
| `vendor_notified` | Envoyée à qui peut corriger, avec la date |
| `embargoed` | Notifiée, et une date de publication convenue |
| `published` | Publiée |
| `withheld` | Délibérément non publiée, avec un motif écrit |

On ne revient pas en arrière. Une divulgation dont on peut réécrire
l'historique n'est pas une divulgation.

Passer de `private` directement à `published` est refusé par la plateforme,
pas seulement déconseillé.

## 3. Le délai

**Quatre-vingt-dix jours** à partir de la notification, par défaut.

C'est un défaut, pas une loi. Un éditeur qui corrige en une semaine ne doit
pas attendre douze : la date convenue remplace le défaut. Un éditeur qui
demande plus a parfois raison, et l'accord se note.

Si l'éditeur ne répond pas, le délai court quand même. Le silence n'est pas
un veto.

## 4. Ce qui compte comme une trouvaille

- Un modèle nommé **avec sa version**. « GPT-4 » n'est pas une cible ; sans la
  version ou la date d'instantané, personne ne peut rejouer six mois plus tard.
- Une **procédure de reproduction** qu'un tiers peut suivre.
- Un **taux de réussite** sur un nombre d'essais annoncé. Sept sur dix et sept
  sur mille se racontent pareil quand on ne compte que les réussites.
- Une **atténuation proposée**. Signaler sans proposer laisse le problème
  entier à quelqu'un d'autre.

Zéro réussite sur N essais n'est pas une trouvaille : c'est un modèle qui se
comporte correctement. C'est utile, et ça se publie ailleurs.

## 5. Les biais

Un biais mesuré se divulgue **même quand c'est gênant**, y compris pour un
partenaire de Skilluv. La condition est la même que pour le reste : protocole
écrit, sous-populations nommées, écart mesuré, tiers capable de rejouer.

Un résultat de biais non reproductible n'est pas divulgué — pas par prudence
politique, parce qu'il n'est pas établi.

## 6. Le double usage

Certaines trouvailles apprennent plus à un attaquant qu'elles n'aident un
défenseur. La plateforme prévoit `withheld` pour ce cas, et **exige un motif
écrit** : retenir sans dire pourquoi ne se distingue pas d'enterrer.

Les cas sensibles se décident à plusieurs. En phase 1, cela veut dire : au
moins un titulaire de `ai_reviewer:safety` autre que l'auteur, et la décision
est consignée.

Publier une atténuation sans publier l'exploitation complète est presque
toujours l'option qui reste ouverte quand les deux extrêmes sont mauvais.

## 7. Publier les poids

Rendre publics des poids entraînés est une décision, pas une formalité de fin
de projet. Avant publication : ce que le modèle sait faire qu'on ne veut pas,
ce que contient son jeu d'entraînement, et sous quelle licence il sort.

Un modèle qui n'a pas été évalué sur ce point n'est pas prêt à être publié,
même s'il fonctionne.

---

*Voir aussi : la [charte du domaine](./CHARTER.md) et le modèle de rapport de
red-team, servi par `GET /api/guides/template-red-team-report`.*
