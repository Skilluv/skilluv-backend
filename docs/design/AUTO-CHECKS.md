# Vérifications automatiques

*Ce que la machine dit d'une version, et ce qu'elle ne dira jamais.*

---

## Le principe

Le design n'a pas de CI verte. Chaque verdict est celui d'une personne, et ça
ne changera pas : rien d'automatique ne sait si une marque convient à une
coopérative agricole, ni si une hiérarchie se lit.

Ce que ces vérifications font, c'est retirer l'arithmétique des mains du
relecteur. Un rapport de contraste a une seule bonne réponse, et un relecteur
qui le calcule à la main est un relecteur qui ne regarde pas le dessin.

**Aucune vérification ne bloque quoi que ce soit.** Une version peut porter une
alerte `error` et être validée ; elle peut être irréprochable et être refusée.
Le second cas est le plus fréquent.

## Pourquoi elles ne bloquent pas

Une vérification bloquante devrait avoir raison à chaque fois. Le premier faux
positif sur un choix délibéré apprend à toute une communauté à la contourner —
et à partir de là, le panneau est du bruit que plus personne ne lit.

Les trois niveaux se lisent donc ainsi :

| Niveau | Ce que ça veut dire |
| --- | --- |
| `info` | Un fait, affiché parce qu'il est utile. |
| `warning` | Probablement une erreur. À peser. |
| `error` | Presque sûrement une erreur, et ça mérite une phrase dans la critique. |

Aucun des trois ne refuse une soumission.

## Ce qui est vérifié

### Contraste d'une palette — `palette_contrast`

Toutes les paires de la palette, contre les seuils WCAG AA (4,5:1 pour du texte
courant, 3:1 pour du grand texte et les objets graphiques).

Les paires plutôt que le blanc, parce qu'une palette de marque est utilisée
contre elle-même : l'erreur qu'un designer découvre tard, c'est la couleur
secondaire sur la primaire.

La question posée n'est pas « toutes les paires passent-elles » — aucune
palette n'y arrive — mais **« existe-t-il une paire dans laquelle on peut
écrire »**. Si la réponse est non, c'est une `error` : la palette ne permet
aucun texte lisible.

### Jetons de design — `token_lint`

Lit les deux formes rencontrées dans la nature : plate (`{"nom": valeur}`) et
imbriquée avec un `value` sur les feuilles, comme le format Design Tokens.

Signale :

- une opacité hors de `[0, 1]` ;
- un rayon négatif ;
- un espacement hors du pas de 4 suivi par le reste de l'échelle ;
- un nom qui mélange tiret et souligné.

Le pas de 4 n'est pas une vérité universelle — certains systèmes utilisent 8,
d'autres une échelle modulaire — c'est pourquoi c'est un `warning` qui nomme le
pas. Ce qu'il attrape, c'est le vrai défaut : une échelle 4, 8, 12, 16, 22, 24,
où une valeur a été tapée au lieu d'être dérivée.

### Coût d'une animation — `motion_cost`

Lit un document Lottie et dit ce qu'il coûtera à jouer : nombre de calques,
durée, cadence.

Au-delà de 60 calques, le rendu devient coûteux sur les téléphones d'entrée de
gamme que beaucoup de nos utilisateurs ont. Au-delà de 5 secondes, ce n'est
plus une animation d'interface.

**Les calques imbriqués dans une précomposition comptent.** Ils sont rendus
aussi, et un fichier qui cache quarante calques derrière un seul est exactement
celui que cette vérification existe pour repérer.

### Cohérence d'un SVG — `svg_consistency`

Absence de `viewBox` (le dessin ne se met pas à l'échelle), et épaisseurs de
trait multiples dans un même fichier.

Volontairement pas un analyseur XML : ce qui compte pour un jeu d'icônes, c'est
le système de coordonnées et l'épaisseur, et les deux se lisent dans les
attributs sans embarquer une dépendance pour se tromper sur les espaces de
noms.

### Lecture du fichier — `fetch`

Une adresse Figma, Miro ou Framer ne se lit pas sans détenir le compte de
quelqu'un, et la plateforme n'en détient aucun. Ces versions enregistrent un
`info` qui le dit.

**Ne rien enregistrer serait pire.** Un relecteur qui ouvre un panneau vide
comprend « tout est passé ». Le silence et la réussite ne doivent pas se
ressembler.

Seul le HTTPS est lu : le résultat d'une vérification est enregistré comme un
fait, et une réponse interceptée serait enregistrée comme un fait.

## Ce qui n'est pas vérifié, et pourquoi

**Le contraste dans une maquette.** Il faudrait lire le fichier Figma, donc
détenir un compte Figma. Ce sera possible le jour où l'intégration OAuth
existera ; en attendant, l'absence est déclarée plutôt que masquée.

**Les tailles de cible tactile.** Même raison.

**L'optimisation d'un SVG.** Dire qu'un fichier pourrait être 20 % plus petit
demande de faire tourner un optimiseur complet, ce qui veut dire exécuter du
code sur un fichier fourni par un tiers. Le rapport bénéfice/risque n'y est
pas.

**Tout ce qui relève du jugement.** L'adéquation à la marque, la hiérarchie de
lecture, la justesse d'une direction. Ce n'est pas une limite technique : c'est
ce pour quoi il y a des relecteurs.

## Où ça s'affiche

Beside la version, dans l'interface du relecteur, groupé par niveau. Les
résultats sont conservés par tour : le tour 1 et le tour 3 gardent chacun les
siens, ce qui permet de voir qu'un problème signalé a été traité.

Un tour re-vérifié **remplace** ses résultats au lieu de s'y ajouter. Deux
lectures de contraste contradictoires côte à côte, c'est ainsi qu'un relecteur
apprend à ignorer le panneau.

---

*Voir aussi : [Charte du domaine Design](CHARTER.md),
[Devenir relecteur](REVIEWER-ONBOARDING.md).*
