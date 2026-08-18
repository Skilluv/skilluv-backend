# Rendre un fichier lourd

*Une scène Blender de 4 Go, un projet After Effects, un master audio non
compressé. Comment ça se passe, et pourquoi c'est fait comme ça.*

---

## Le principe : les octets ne passent pas par l'API

Le serveur ne reçoit jamais le fichier. Il remet des **URL signées**, une par
morceau, et le client dépose chaque morceau directement dans le stockage
d'objets. À la fin, le serveur demande au stockage d'assembler.

Ce n'est pas une optimisation. Cinq gigaoctets à travers l'API, c'est une
connexion et un tampon mémoire immobilisés aussi longtemps que dure la mise en
ligne — et pour chaque envoi simultané. Un seul designer sur une connexion
lente dégraderait la plateforme pour tout le monde.

Effet de bord utile : **l'envoi est reprenable sans rien mémoriser**. Un
morceau qui a échoué est à une URL signée près d'être réessayé, et le serveur
n'a aucun décalage d'octets à retenir.

## Le déroulé

1. `POST /api/design/uploads` — vous annoncez le sous-type, le nom du fichier,
   son type MIME et sa taille. Vous recevez une session, la taille de morceau,
   et une URL signée par morceau.
2. Vous déposez chaque morceau par `PUT` à l'URL correspondante. Le stockage
   vous répond un en-tête `ETag` : **gardez-le**, c'est ce qui prouve que le
   morceau est arrivé intact.
3. `POST /api/design/uploads/{id}/complete` avec la liste des
   `{part_number, etag}`. Le stockage assemble.

Si une URL expire (six heures) ou si votre machine redémarre :
`GET /api/design/uploads/{id}/parts?from=…&to=…` en redonne. Demandez les
morceaux pour lesquels vous n'avez pas d'`ETag` ; c'est la même opération que
reprendre.

## Les limites, et d'où elles viennent

| Sous-type | Limite |
| --- | --- |
| Deck de contenu, document de recherche | 100 Mo |
| Kit de marque, jeu d'icônes, famille de caractères | 200 Mo |
| Interface, design system | 500 Mo |
| Son | 500 Mo |
| Jeu d'illustrations | 1 Go |
| Projet motion | 2 Go |
| Vidéo, scène 3D | 5 Go |

Elles viennent de ce que l'artefact **est**. Un kit de marque, c'est du
vectoriel et un document ; un jeu d'icônes fait quelques centaines de
kilooctets et fait semblant d'être plus gros. Une scène et une vidéo rendue
sont réellement énormes, et il n'y a pas de façon honnête de demander de les
réduire.

Le refus arrive **avant** que les octets bougent, à partir de la taille que
vous annoncez. La taille réelle est relue dans le stockage à l'assemblage : un
client qui ment perd son envoi. Les deux contrôles sont nécessaires — le
premier évite le transfert inutile, le second évite d'être crédule.

Un fichier qui dépasse sa limite est presque toujours un fichier dans le
mauvais sous-type.

## L'aperçu : fourni, pas généré

Quatre sous-types doivent arriver avec un aperçu : **projet motion, vidéo,
scène 3D, son**. Ce sont ceux dont le fichier source ne s'ouvre pas dans un
navigateur.

`POST /api/design/uploads/{id}/preview-url` donne une URL signée pour le
déposer.

**Pourquoi la plateforme ne le génère pas.** Le rendre côté serveur demanderait
ffmpeg pour la vidéo, Blender sans interface pour la 3D, un générateur de
vignettes pour le reste — trois binaires lourds et un accès au démon Docker,
sur une machine que ce projet ne peut pas se payer. Le tout pour produire une
image fixe que la personne qui a fabriqué le fichier choisirait mieux que
n'importe quelle heuristique.

Le cahier des charges admettait déjà le principe pour After Effects — rien ne
sait lire un `.aep`, donc un MP4 d'aperçu est exigé à côté. La règle est
simplement étendue à tout ce qui est dans la même situation.

Ce n'est pas une commodité : **une file de relecture pleine de fichiers qu'on
ne peut pas ouvrir est une file que personne ne traite.**

## Où ça vit, et qui peut le lire

Dans le seau privé, jamais public. Un livrable design peut être sous accord de
confidentialité — voir [Données personnelles](DATA-GOVERNANCE.md) — donc la
règle par défaut est « lisible seulement par une URL signée ».

`GET /api/design/uploads/{id}/download-url` en fabrique une, valable une heure
par défaut, un jour au maximum.

## Ce qui est abandonné, et quand

Un envoi non terminé est abandonné au bout de **sept jours**. Un envoi
multipart interrompu conserve les morceaux déjà déposés, et le stockage les
facture que quelqu'un termine ou non ; sept jours laissent le temps de revenir
de vacances sans transformer le seau en décharge.

Le balayage tourne une fois par nuit, activé par variable d'environnement
(`SKILLUV_DESIGN_UPLOAD_SWEEP_ENABLED`) — comme les autres, pour que « quelle
machine fait le ménage » soit une réponse explicite plutôt qu'un hasard.

## Ce que ça coûte

Du stockage compatible S3 tourne autour de 15 $ le téraoctet et par mois. Cent
designers actifs à 2 Go en moyenne, c'est 200 Go, soit environ 3 € par mois.
Ce n'est pas le stockage qui coûte, c'est le calcul — raison de plus pour ne
pas générer d'aperçus.

---

*Voir aussi : [Charte](CHARTER.md), [Vérifications automatiques](AUTO-CHECKS.md),
[Données personnelles](DATA-GOVERNANCE.md).*
