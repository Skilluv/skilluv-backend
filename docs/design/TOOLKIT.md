# Outils, par famille

*Ce document existe pour une raison précise : Skilluv s'adresse en priorité à
des autodidactes et à des personnes en reconversion, souvent en Afrique de
l'Ouest, avec un budget contraint et une connexion qui ne l'est pas moins. Une
liste d'outils qui suppose un abonnement Adobe et la fibre est une liste qui
exclut.*

**Aucun outil n'est exigé.** Les grilles de revue jugent un livrable, jamais le
logiciel qui l'a produit. Un travail fait sous Inkscape et un travail fait sous
Illustrator sont lus avec la même grille.

---

## Ce qui compte vraiment

Trois contraintes structurent tout ce qui suit :

1. **Le format de livraison** doit être ouvrable par quelqu'un d'autre. SVG,
   PDF, PNG, MP4, glTF, WOFF2. Un `.ai` seul n'est pas un livrable.
2. **Le poids** compte : un fichier de 4 Go est un fichier que le relecteur
   n'ouvrira pas depuis une connexion mobile.
3. **La reprenabilité** : les sources, structurées et nommées.

Un outil qui permet ces trois choses convient. C'est toute l'exigence.

---

## `product`, `web`, `mobile` — écrans et systèmes

| Gratuit ou libre | Payant |
| --- | --- |
| Figma (offre gratuite : 3 fichiers) | Figma payant |
| Penpot (libre, auto-hébergeable) | Sketch |
| PhotoPea (retouche dans le navigateur) | Adobe XD |

**Recommandé pour commencer :** Penpot. Libre, exporte en SVG, et fonctionne
sur un poste modeste. L'offre gratuite de Figma tombe vite à court de fichiers
quand on enchaîne les challenges.

## `motion` — animation et vidéo

| Gratuit ou libre | Payant |
| --- | --- |
| Blender (3D, montage, compositing) | After Effects |
| Kdenlive, Shotcut (montage) | Cinema 4D |
| Rive (offre gratuite, animation d'interface) | Rive payant |
| Lottie / lottiefiles | |

**Recommandé :** Blender pour la 3D et le rendu, Kdenlive pour le montage. Le
motion d'interface se livre en Lottie, qui est un format ouvert et léger — et
qui est ce qu'un développeur intégrera de toute façon.

**Attention au poids.** Une animation rendue en 4K non compressée dépasse
n'importe quelle limite raisonnable. Livrez en 1080p, H.264, et gardez le
projet à côté.

## `brand`, `illustration` — identité, dessin, icônes

| Gratuit ou libre | Payant |
| --- | --- |
| Inkscape (vectoriel) | Illustrator |
| Krita (dessin) | Procreate |
| GIMP (retouche) | Photoshop |
| FontForge, Glyphs Mini (police) | Glyphs, FontLab |

**Recommandé :** Inkscape pour tout le vectoriel. Le SVG qu'il produit est
propre et c'est le format de livraison exigé de toute façon.

**Pour les polices :** FontForge est austère mais complet et libre. Une famille
livrée en WOFF2 + les sources est ce qu'on demande.

## `dataviz` — données lisibles

| Gratuit ou libre | Payant |
| --- | --- |
| Observable Plot, D3 | Tableau |
| RAWGraphs (navigateur) | Flourish payant |
| Matplotlib, Vega-Lite | |
| Datawrapper (offre gratuite) | |

**Recommandé :** RAWGraphs pour explorer, puis SVG repris sous Inkscape pour
finir. Une visualisation livrée en SVG reste modifiable ; une image ne l'est
pas.

## `ux-writing`, `marketing` — mots et déclinaisons

Presque rien de spécifique. Un éditeur de texte, un tableur pour les tableaux
de libellés, et l'outil d'écrans de la famille `product` pour voir les mots en
place.

**L'outil qui manque à la plupart des gens :** un compteur de signes par
emplacement. Un libellé qui tient en français et déborde en anglais est
l'erreur la plus fréquente du métier.

## `game` — sous contrainte de moteur

| Gratuit ou libre | Payant |
| --- | --- |
| Godot | Unity (gratuit sous seuil) |
| Blender | Substance (abonnement) |
| Materialize, ArmorPaint | |
| Aseprite (source libre, binaire payant) | |

**Recommandé :** Godot et Blender. La contrainte de moteur — budget de
polygones, taille de textures — se vérifie dans le moteur, pas dans le logiciel
de dessin, et un livrable de jeu doit tourner.

## `3d-viz` — architecture et intérieur

| Gratuit ou libre | Payant |
| --- | --- |
| Blender + Cycles | 3ds Max, V-Ray |
| SketchUp Free (navigateur) | SketchUp Pro |
| FreeCAD | Lumion, Twinmotion |

**Recommandé :** Blender. Le rendu Cycles suffit largement pour une
présentation, et le temps de calcul se contourne en réduisant les échantillons
plutôt qu'en payant.

## `immersive` — spatial et son

| Gratuit ou libre | Payant |
| --- | --- |
| Godot XR, A-Frame, WebXR | Unity + XR |
| Audacity, Ardour, Reaper (essai illimité) | Ableton, Pro Tools |
| LMMS | |

**Recommandé :** A-Frame pour un prototype spatial consultable dans un
navigateur — c'est aussi ce qui rend le livrable ouvrable par un relecteur qui
n'a pas de casque. Reaper pour le son : l'essai n'expire pas, et la licence
personnelle est parmi les moins chères du secteur.

## `service` — processus

Un tableur, un outil de diagramme, et de l'écrit. Excalidraw, draw.io, ou
Penpot. Le livrable est un raisonnement, pas un rendu.

---

## Ce qu'il faut savoir sur les licences

**Une police achetée sous licence bureau ne se livre pas à un client.** C'est
le piège le plus coûteux du domaine : la licence est la vôtre, pas la sienne,
et c'est lui qui recevra la facture.

Pour livrer une identité sans mauvaise surprise, utilisez des polices libres —
Google Fonts, Fontshare, Velvetyne — ou faites acheter la licence par le
client, à son nom, avant de livrer.

Même règle pour les photographies, les modèles 3D et les bibliothèques de
composants. Voir [Propriété intellectuelle](IP-AND-COPYRIGHT.md), section 5.

---

*Voir aussi : [Arriver sur Skilluv](ONBOARDING.md).*
