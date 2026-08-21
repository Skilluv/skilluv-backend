# Modèles de brief — domaine Design

Treize modèles, un par famille de métiers. À utiliser pour rédiger l'énoncé
d'un challenge design ou d'un concours.

Un brief mal écrit produit des propositions incomparables : chacune répond à
une question différente, et le relecteur arbitre au jugé. C'est particulièrement
vrai en design, où une proposition hors sujet peut être objectivement belle —
et où « je préfère celle-ci » est le raisonnement qu'un brief précis rend
impossible.

**La famille détermine la grille de revue appliquée** (`reviewer_group` sur
l'orientation, migration `0229`). Écrire un brief dans la mauvaise famille,
c'est promettre une grille et en appliquer une autre.

---

## Structure commune

Tout brief design comporte ces huit sections, dans cet ordre.

### 1. Contexte

Qui commande, pour qui, et ce qui existe déjà. Un designer qui ne sait pas ce
qu'il y a en place propose une refonte quand on attendait une correction.

### 2. Problème

Ce qui ne va pas aujourd'hui, **du point de vue de quelqu'un que ça gêne**.
Pas la solution attendue.

> Mauvais : « Refaire le logo en plus moderne. »
> Bon : « Le logo est illisible en dessous de 24 px, et 70 % de nos supports
> sont des favicons et des tampons d'une seule couleur. »

### 3. Contraintes

Ce qui n'est pas négociable, et pourquoi. Format, support, monochromie,
budget d'impression, contrainte technique, réglementation. **Une contrainte
non dite est une itération perdue.**

### 4. Livrables attendus

La liste exacte, avec les formats. « Une identité » n'est pas un livrable ;
« logotype en SVG, palette avec valeurs de contraste, une police libre de
droits ou sous licence nommée, et un document de règles de 4 pages » en est un.

Cette section devient le `design_subtype` du challenge, et c'est elle qui
décide de la taille de fichier acceptée et de la comparaison proposée entre
deux tours.

### 5. Critères de jugement

Repris de la grille de famille, énoncés avant. Personne n'est jugé sur un
critère qu'on ne lui a pas montré.

### 6. Accessibilité

Ce qui est exigé concrètement : contraste minimal, taille de corps minimale,
alternative pour ce qui repose sur la couleur, sous-titres. C'est dans la
grille commune, donc c'est demandé à toutes les familles.

### 7. Éléments fournis

Ce que le commanditaire donne, et sous quelle licence. Photographies, police
achetée, bibliothèque existante, contenu réel. Un brief qui fournit du faux
texte produit des propositions qui cassent avec le vrai.

### 8. Nombre de tours annoncé

Combien de tours de critique sont prévus (`design_expected_rounds`). Le
plafond dur est cinq. Annoncer un tour unique pour une identité de marque est
une promesse qu'on ne tiendra pas.

---

## Les treize familles

### `product` — produit, systèmes, conversationnel

*Trades : `design-product`, `design-system`, `design-ai-conversational`.*

Ajouter au brief commun :

- **Le parcours visé**, du point d'entrée à la fin ;
- **Les états** : vide, chargement, erreur, permission refusée. Une maquette
  qui ne montre que le cas nominal n'est pas utilisable ;
- **Les cibles techniques** : navigateurs, tailles, mode sombre ;
- Pour un système : quels produits l'utiliseront, et qui maintiendra.

Pièges : proposer un écran isolé, oublier l'état vide, dessiner un composant
sans dire comment il se comporte quand le texte double de longueur.

### `web` — sites, éditorial

*Trades : `design-web`, `design-editorial-web`.*

Ajouter :

- **La hiérarchie de lecture** attendue et ce qui doit être vu en premier ;
- **Le volume réel de contenu**, y compris le pire cas (titre à 90 signes) ;
- **La performance** : budget de poids, comportement sans image ;
- Pour l'éditorial : la longueur des articles et la présence de médias.

Pièges : composer sur du texte inventé plus court que le vrai.

### `mobile` — applications

*Trade : `design-mobile`.*

Ajouter :

- **Les plateformes** et les conventions à respecter ou à assumer ;
- **La zone du pouce** et la tenue à une main ;
- **Le hors ligne** et la connexion lente — non négociable pour un public
  ouest-africain ;
- Les tailles réelles ciblées, y compris les petits écrans.

Pièges : maquetter sur un écran de 6,7 pouces uniquement.

### `motion` — animation, vidéo

*Trades : `design-motion-ui`, `design-motion-2d`, `design-motion-3d`,
`design-video`.*

Ajouter :

- **La durée** et le format de sortie ;
- **La cadence** et la plateforme de diffusion ;
- **Ce qui déclenche** l'animation, pour du motion d'interface ;
- **Le son** : présent ou muet par défaut ;
- **La réduction de mouvement** : ce qui se passe quand elle est activée.

Pièges : livrer un rendu sans le projet ; ignorer `prefers-reduced-motion`.

### `brand` — identité, typographie, verbal

*Trades : `design-brand-identity`, `design-typography`,
`design-naming-verbal`.*

Ajouter :

- **Les supports réels** où la marque apparaîtra, du plus contraint au plus
  libre ;
- **Le pire cas de reproduction** : une couleur, petite taille, sérigraphie,
  broderie ;
- **Ce qui existe** et ce qu'on garde ;
- Pour une police : le jeu de caractères exigé, langues comprises, et les
  graisses.

Pièges : présenter une marque uniquement en grand sur fond blanc.

### `illustration` — illustration, icônes, personnages

*Trades : `design-illustration`, `design-iconography`, `design-character`.*

Ajouter :

- **Les tailles de rendu** et la plus petite ;
- **La cohérence de jeu** : combien d'éléments, quelle grille, quelle épaisseur
  de trait ;
- **Les formats de livraison** et la façon dont les fichiers sont nommés ;
- Pour un personnage : les poses ou expressions attendues.

Pièges : dessiner une icône magnifique qui devient une tache à 16 px.

### `dataviz` — données rendues lisibles

*Trade : `design-dataviz`.*

Ajouter :

- **Les données réelles**, ou un jeu représentatif — avec les valeurs
  aberrantes ;
- **La question** à laquelle la visualisation répond ;
- **Le public** : expert ou non ;
- **La lisibilité sans couleur**, qui est ici une contrainte structurelle et
  non un ajout.

Pièges : une visualisation calibrée sur des données propres et inventées.

### `ux-writing` — les mots d'une interface

*Trade : `design-ux-writing`.*

Ajouter :

- **La langue** et le registre ;
- **Les contraintes de longueur**, par emplacement ;
- **Les cas d'erreur** à écrire, qui sont l'essentiel du travail ;
- **La traduisibilité** : ce qui doit tenir aussi en anglais et en arabe.

Pièges : écrire les libellés heureux et laisser les erreurs en anglais
technique.

### `marketing` — un message sur plusieurs surfaces

*Trade : `design-marketing`.*

Ajouter :

- **Les formats exacts**, avec leurs dimensions ;
- **Le message unique** à faire passer ;
- **Les contraintes des plateformes** de diffusion ;
- **Ce qui doit rester lisible** en vignette.

Pièges : décliner un visuel qui ne fonctionne qu'au format d'origine.

### `game` — sous contrainte de moteur

*Trades : `design-game-ui`, `design-game-environment`.*

Ajouter :

- **Le moteur** et ses limites : budget de polygones, taille des textures ;
- **La lisibilité en mouvement**, à la vitesse réelle du jeu ;
- **La manette** autant que la souris ;
- **Le style** de référence, avec des images.

Pièges : une interface conçue pour être vue à l'arrêt.

### `3d-viz` — bâtiments qui n'existent pas encore

*Trade : `design-arch-interior-viz`.*

Ajouter :

- **Les plans** ou le modèle source ;
- **L'heure et la lumière** attendues ;
- **Les points de vue** demandés ;
- **Le degré de réalisme** : présentation commerciale ou étude technique.

Pièges : un rendu magnifique dont les proportions ne correspondent pas aux
plans.

### `immersive` — ce que ça fait à un corps dans un espace

*Trades : `design-ar-vr-spatial`, `design-sound`.*

Ajouter :

- **Le matériel** ciblé ;
- **La durée d'usage** prévue, et le confort à cette durée ;
- **Le confort** : ce qui est fait contre la nausée ;
- Pour le son : l'environnement d'écoute, et le rendu au casque comme au
  haut-parleur.

Pièges : concevoir pour une démonstration de deux minutes une expérience qui
durera vingt.

### `service` — vérifier qu'un processus tient

*Trades : `design-service`, `design-ops`.*

Ajouter :

- **Les acteurs**, y compris ceux qui ne voient jamais l'écran ;
- **Les points de contact** dans l'ordre ;
- **Ce qui casse aujourd'hui**, avec des faits ;
- **Ce qui est mesurable** après.

Pièges : une carte d'expérience qui décrit le parcours idéal et pas celui qui
échoue.

---

*Voir aussi : [Charte du domaine Design](CHARTER.md),
[Trames d'écrit](WRITEUP-TEMPLATES.md).*
