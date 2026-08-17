# Propriété, licences et données — domaine IA

*Destinée à être publiée sur `skill-uv.com/ai/ip`.*

**Ce document n'est pas un avis juridique.** Il énonce la position de Skilluv
et les règles que la plateforme applique. Les questions ouvertes sont
signalées comme telles ; elles attendent une relecture par un juriste
spécialisé, et ce document sera repris à ce moment-là.

Il est écrit maintenant parce que l'absence de règle est elle-même une règle,
et la pire : sans texte, chacun improvise et découvre le problème une fois le
modèle publié.

---

## 1. À qui appartient un modèle entraîné

**Par défaut, à la personne qui l'a entraîné.** Un modèle produit dans le
cadre d'un challenge Skilluv appartient à son auteur ; la plateforme n'en
revendique rien et n'en héberge pas les poids.

Deux exceptions :

- **Mission commandée.** Ce qui est produit pour un client suit ce que le
  contrat dit, et le contrat doit le dire. Voir §5.
- **Modèle dérivé.** Affiner un modèle pré-entraîné ne remet pas les compteurs
  à zéro : voir §2.

## 2. La chaîne des licences

Un modèle affiné hérite des obligations de son modèle de base. Elles diffèrent
et l'écart est important :

| Type de licence amont | Ce qu'elle implique en pratique |
|---|---|
| Apache 2.0 / MIT | Usage commercial libre, attribution requise |
| Licence communautaire d'éditeur | Souvent des seuils d'usage, des interdictions d'usage, parfois une obligation de nommer |
| Non commerciale | Le dérivé ne peut pas être vendu, ni servir un produit payant |
| Poids fermés derrière une API | On n'affine pas ; on n'a pas les poids |

**Règle Skilluv** : la licence du modèle de base est citée dans la fiche, et
la licence du dérivé est compatible avec elle. Un livrable dont la chaîne de
licences n'est pas cohérente est refusé — non par formalisme, mais parce
qu'il est inutilisable par quiconque le reprendrait.

Les termes changent. **Vérifier la licence à la date de l'entraînement**, et
la noter dans la fiche avec cette date.

## 3. Provenance des données

**Recevable** : un jeu publié sous licence ouverte ; des données produites par
soi ; des données obtenues avec consentement ; un corpus public dont les
conditions d'usage autorisent l'usage prévu.

**Non recevable** : un site aspiré contre ses conditions ; des données
personnelles sans base légale ; un jeu dont on ne peut pas dire d'où il vient.

Une position par défaut sur le web ouvert serait imprudente et n'est pas prise
ici : la légalité de l'aspiration à des fins d'entraînement varie selon les
juridictions et évolue. **Question ouverte, à trancher avec un juriste.**

Ce qui est tranché : sur Skilluv, un livrable **dit d'où viennent ses
données**. Un jeu sans provenance énoncée est refusé quelle que soit la
réponse juridique par ailleurs.

## 4. Données personnelles

Le RGPD suppose qu'une donnée puisse être effacée. Un modèle entraîné ne le
permet pas simplement : les poids ne s'oublient pas sur demande.

La conséquence pratique est en amont, pas en aval :

- ne pas mettre de données personnelles dans un jeu d'entraînement sans base
  légale explicite ;
- anonymiser avant, pas après ;
- pour un jeu contenant des personnes — visages, voix, textes identifiants —
  documenter le consentement dans la fiche.

Un modèle qu'on ne pourrait pas corriger si une personne se retirait est un
modèle dont on ne publie pas les poids.

## 5. Missions commandées

Quand un travail est payé par un tiers, quatre partages sont possibles et le
contrat en désigne un :

- **cession complète** — poids et code passent au client ;
- **modèle ouvert** — le client garde ses droits d'usage, le modèle est publié ;
- **licence commerciale** — le client peut l'exploiter, personne d'autre ;
- **poids au client, code à l'auteur** — le plus fréquent, et le plus mal écrit.

**Aucune mission ne commence sans que le partage soit écrit.** L'ambiguïté ne
se découvre qu'au moment où le travail a de la valeur, c'est-à-dire au pire
moment.

*Le module de missions n'existe pas encore côté plateforme ; cette section dit
ce qu'il devra imposer.*

## 6. Réglementation européenne

Le règlement européen sur l'IA classe les usages par niveau de risque et
impose des obligations de transparence croissantes. Skilluv n'est pas
soumis à l'essentiel de ces obligations aujourd'hui — nous ne mettons pas de
système IA sur le marché européen — mais les travaux publiés ici peuvent
l'être par ceux qui les reprennent.

La position : **écrire ce qu'il faut pour qu'un repreneur puisse se
conformer**. C'est exactement ce que la fiche de modèle demande déjà — usage
prévu, données, limites, évaluation. Un travail bien documenté au sens de la
charte est un travail sur lequel la conformité est possible.

**Question ouverte** : le détail des obligations selon la classification, à
préciser avec un juriste avant qu'une entreprise européenne ne commande une
mission.

## 7. Contenu génératif

Le statut des sorties d'un modèle génératif varie selon les juridictions et
n'est pas stabilisé. Le rappeler est plus honnête que trancher.

Ce que Skilluv applique :

- les modèles et LoRA utilisés sont cités, avec leur licence ;
- un style imitant une personne vivante identifiable n'est pas publié sans
  son accord — indépendamment de ce que la loi autorise ;
- la nature générative de l'artefact est déclarée.

## 8. Ce qui entraîne une révocation

- une chaîne de licences violée, découverte après validation ;
- un jeu de données retiré pour un problème de droits ;
- des données personnelles trouvées dans un jeu publié ;
- une provenance déclarée qui se révèle fausse.

La révocation retire l'artefact du décompte et laisse l'historique visible.

---

*Voir aussi : la [charte du domaine](./CHARTER.md) et la [politique de
divulgation](./SAFETY-DISCLOSURE.md).*
