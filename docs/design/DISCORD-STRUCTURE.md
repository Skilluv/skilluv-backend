# Salons Discord — domaine Design

*Structure proposée pour la partie design du serveur. Elle suit celle du
domaine code (`docs/code/DISCORD-STRUCTURE.md`) : mêmes conventions, mêmes
rôles, pas de serveur séparé.*

---

## Le principe : un salon par famille, pas par métier

Vingt-six salons pour vingt-six métiers donneraient vingt-six salons vides. Le
découpage suit les treize familles de relecture, qui sont déjà le découpage par
lequel les gens se ressemblent assez pour s'entraider.

Et même treize est optimiste au départ. **Ouvrir un salon quand il y a du
monde, pas avant.** Un salon mort décourage plus qu'il n'accueille.

## Ordre d'ouverture

| Phase | Salons | Quand |
| --- | --- | --- |
| 1 | `#design-general`, `#design-critique` | Tout de suite |
| 2 | `#design-produit`, `#design-brand`, `#design-illustration` | Au premier groupe régulier |
| 3 | les dix autres familles | Quand une famille a cinq personnes actives |

## Catégorie `DESIGN`

### `#design-general`

Le point d'entrée. Présentations, questions générales, ce qui ne rentre nulle
part ailleurs.

### `#design-critique`

**Le salon qui compte.** On y demande un regard sur un travail en cours, avant
de le rendre.

Règle d'usage, épinglée : une demande de critique dit ce qu'on cherche. « Un
avis ? » n'appelle que des réponses inutiles. « Le logotype tient-il à 16 px,
et la contreforme se ferme-t-elle sur fond sombre ? » appelle une réponse
utilisable.

Ce salon n'est **pas** la relecture officielle. Ce qui s'y dit ne crée aucune
preuve et n'engage personne.

### `#design-veille`

Ce qu'on a lu, vu, appris. Pas de dépôt de liens sans phrase : ce qui a de la
valeur, c'est pourquoi vous le partagez.

### Salons de famille

`#design-produit`, `#design-web`, `#design-mobile`, `#design-motion`,
`#design-brand`, `#design-illustration`, `#design-dataviz`,
`#design-ux-writing`, `#design-marketing`, `#design-game`, `#design-3d-viz`,
`#design-immersif`, `#design-service`.

Un par famille de relecture, ouverts au fur et à mesure.

### `#design-concours`

Briefs ouverts, questions sur un brief en cours, résultats.

**Interdit dans ce salon :** publier sa proposition pendant qu'un concours à
fenêtre aveugle est ouvert. La fenêtre existe contre le mimétisme, et la
contourner par Discord la vide de son sens. Un message qui le fait est
supprimé, sans sanction la première fois.

### `#design-relecteurs` (privé)

Réservé aux détenteurs d'une capacité `design_reviewer:*`. Arbitrage des cas
limites, calibrage entre relecteurs, signalements de plagiat.

Privé pour une raison précise : un désaccord entre relecteurs discuté devant
la personne concernée transforme une critique en tribunal.

## Rôles

| Rôle | Attribué à |
| --- | --- |
| `Designer` | Toute personne ayant déclaré un métier design. |
| `Relecteur design` | Détenteur d'une capacité `design_reviewer:*`. |
| `Relecteur — {famille}` | Un rôle par famille, pour mentionner les bonnes personnes. |
| `Jury` | Détenteur de `jury_tournament`. |

Les rôles suivent les capacités de la plateforme et ne s'accordent pas
séparément sur Discord. Un rôle Discord qui ne correspond à rien côté
plateforme est une autorité qui n'existe pas.

## Ce que le bot annonce

Peu de choses, et jamais en mention automatique de tout le monde :

- un nouveau concours ouvert, dans `#design-concours` ;
- un classement publié, dans `#design-concours` ;
- une file de relecture qui dépasse le seuil d'ancienneté, dans
  `#design-relecteurs`.

**Ce que le bot n'annonce pas :** chaque version rendue, chaque validation,
chaque badge. Un serveur où le bot parle plus que les gens est un serveur que
les gens quittent. Les notifications individuelles ont déjà leurs canaux —
in-app, e-mail, push — et Discord n'a pas à les répéter.

## Ce qu'on ne fait pas ici

**Pas de recrutement en message privé non sollicité.** Une entreprise qui
découvre quelqu'un passe par la plateforme, où la mise en relation est tracée
et où le talent n'est pas seul face à elle.

**Pas de vente de prestations entre membres.** Les missions passent par la
plateforme, qui garantit le paiement.

---

*Voir aussi : [Arriver sur Skilluv](ONBOARDING.md),
[Devenir relecteur](REVIEWER-ONBOARDING.md).*
