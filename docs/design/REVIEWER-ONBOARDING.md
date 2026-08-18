# Devenir relecteur design

*Ce document sert à deux choses : recruter des relecteurs, et leur dire ce
qu'on attend d'eux avant qu'ils acceptent.*

---

## 1. Ce qu'est un relecteur ici

Un relecteur lit une version rendue et répond par un verdict argumenté. Ce
n'est pas un jury de concours, ce n'est pas un mentor, et ce n'est pas un
client.

La différence avec un avis de forum : **votre verdict crée une preuve**. Une
approbation produit un livrable vérifié, une attestation, et fait bouger un
rang. Une approbation complaisante ne fait pas plaisir à quelqu'un, elle
dévalue les attestations de tous les autres.

## 2. Les treize familles

Le droit de relire est accordé par famille, pas par domaine. La capacité
s'appelle `design_reviewer:{famille}` et donne accès à la file des métiers de
cette famille uniquement.

| Famille | Métiers couverts |
| --- | --- |
| `product` | produit, design system, conversationnel |
| `web` | sites, éditorial web |
| `mobile` | applications mobiles |
| `motion` | motion d'interface, 2D, 3D, vidéo |
| `brand` | identité, typographie, nommage |
| `illustration` | illustration, icônes, personnages |
| `dataviz` | visualisation de données |
| `ux-writing` | écriture d'interface |
| `marketing` | déclinaisons multi-supports |
| `game` | interface et environnement de jeu |
| `3d-viz` | architecture et intérieur |
| `immersive` | AR/VR spatial, son |
| `service` | design de service, design ops |

`design_reviewer:all` existe et est **rare** : il s'accorde à quelqu'un qui a
démontré sa compétence dans plusieurs familles, ou à un membre de l'équipe qui
arbitre. Détenir la famille dont on relève n'y donne pas droit.

## 3. Ce qu'on demande avant d'accorder la capacité

Trois choses, dans cet ordre d'importance :

1. **Du métier dans la famille visée.** Pas un rang sur la plateforme : du
   travail qu'on peut regarder. Un portfolio externe suffit à cette étape —
   c'est le seul endroit où une réputation importée compte, parce qu'on ne
   juge pas un profil mais une compétence.
2. **Savoir écrire une critique.** C'est la compétence rare. Beaucoup de gens
   savent voir ce qui ne va pas ; peu savent l'écrire de façon qu'on puisse
   agir dessus.
3. **De la disponibilité.** Une file où le plus ancien attend trois semaines
   fait plus de dégâts qu'une file vide.

Ce qu'on ne demande pas : d'être senior, d'avoir travaillé en agence, ou
d'avoir un diplôme.

## 4. Écrire une critique utilisable

Un verdict `iterate` ou `reject` exige un motif de blocage et un texte d'au
moins quarante caractères. Le minimum est une contrainte technique, pas une
cible.

**Une critique utilisable dit trois choses :**

- *ce que vous voyez* — descriptif, vérifiable, pas une impression ;
- *pourquoi c'est un problème* — au regard du brief, pas de votre goût ;
- *ce qui changerait la réponse* — sans dessiner à la place de l'auteur.

> Inutilisable : « Le logo ne fonctionne pas, il manque quelque chose. »
>
> Utilisable : « Le logotype passe sous 24 px dans le favicon et les deux
> contreformes se ferment — c'est le pire cas que le brief posait comme
> non négociable. Une version simplifiée pour les petites tailles, ou un
> dessin qui tient tel quel à 16 px, répondraient tous les deux. »

La deuxième prend une minute de plus et fait gagner un tour.

## 5. Les motifs de blocage

Ils existent pour que la critique soit comparable d'un relecteur à l'autre, et
pour que les statistiques disent quelque chose. Le texte reste obligatoire :
le motif classe, il n'explique pas.

| Motif | Quand |
| --- | --- |
| `brief_mismatch` | Répond à une autre question que celle posée. |
| `craft_gap` | La direction tient, l'exécution ne suit pas. |
| `accessibility_fail` | Contraste, taille, dépendance à la couleur, sous-titres. |
| `sources_missing` | Livré sans ce qu'il faut pour reprendre le travail. |
| `third_party_unclear` | Éléments repris non déclarés ou licence absente. |
| `rationale_absent` | Aucun raisonnement défendable derrière la proposition. |
| `scope_incomplete` | Une partie des livrables demandés manque. |

`third_party_unclear` n'est pas une accusation. Le plagiat avéré ne se traite
pas par un verdict : il se signale.

## 6. Ce qui vous est interdit

- **Relire un challenge que vous avez réclamé.** Refusé par le code.
- **Approuver pour vider la file.** Le score de métier compte les tours de
  critique menés à la validation ; approuver vite ne vous avantage pas.
- **Demander un tour de plus sans dire quoi changer.** C'est un tour perdu
  pour tout le monde, et le motif obligatoire existe pour le rendre difficile.
- **Juger sur ce qui n'est pas dans la grille.** Elle est publiée avant.

## 7. Le plafond de cinq tours

Aucun challenge ne peut aller au-delà de cinq tours : la base de données le
refuse. Si une proposition en est à son quatrième tour, la question n'est plus
« que manque-t-il » mais « le brief était-il écrit correctement ».

Un quatrième `iterate` doit être exceptionnel et argumenté. Dans le doute,
`reject` avec une critique complète est plus honnête qu'un cinquième tour
qu'on sait perdu.

## 8. Ce que vous y gagnez

La relecture est comptée dans le score de métier (`jury_service` pour les
concours, et les tours de critique menés à la validation pour les challenges).
Elle ouvre la capacité `jury_tournament`, donc les panels de concours.

Et, plus prosaïquement : relire trente propositions dans une famille apprend
plus vite sur cette famille que d'en produire trente.

---

*Voir aussi : [Charte du domaine Design](CHARTER.md),
[Trames de brief](BRIEF-TEMPLATES.md).*
