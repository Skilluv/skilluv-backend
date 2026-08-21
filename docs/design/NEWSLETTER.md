# Lettre design mensuelle

*Plan éditorial. Ce document dit ce qu'on publie, à quelle fréquence, avec
quoi, et ce qui existe déjà côté backend pour l'alimenter.*

---

## Pourquoi mensuelle et pas hebdomadaire

Une lettre hebdomadaire demande de la matière chaque semaine. Avec une
communauté qui démarre, elle serait vide trois semaines sur quatre — et une
lettre vide apprend à ne plus l'ouvrir.

Mensuelle, avec quatre semaines de travaux validés derrière soi, il y a
toujours quelque chose à montrer.

## Ce qu'il y a dedans

Cinq rubriques, dans cet ordre. Aucune n'est optionnelle : une lettre dont les
rubriques varient d'un mois à l'autre n'a pas de forme, et une forme est ce
qui fait qu'on la parcourt en trente secondes.

### 1. Le travail du mois

Un travail validé, montré en entier : la version 1, la critique reçue, la
version finale. **C'est la rubrique qui n'existe nulle part ailleurs.** Un
portfolio classique montre le résultat ; ici on montre la distance parcourue,
et elle est vérifiable.

Source : `GET /api/design/users/{username}/iteration-stories` — les travaux
validés après trois tours ou plus.

### 2. La personne mise en avant

Qui, et pourquoi, en une phrase écrite par la personne qui l'a choisie.

Source : `featured_talents`, une par domaine et par semaine. La lettre reprend
les quatre du mois.

### 3. Ce qui est ouvert

Concours en cours et challenges qui attendent quelqu'un, avec leur échéance.

Source : `GET /api/tournaments?kind=brief_contest&skill_domain=design` et les
slices ouvertes.

### 4. Une critique bien écrite

Une critique reçue ce mois-ci, reproduite avec l'accord de son auteur et de la
personne critiquée. C'est le meilleur outil de formation dont on dispose, et
c'est gratuit : il suffit de les republier.

Ce qu'on cherche : une critique qui dit ce qu'elle voit, pourquoi c'est un
problème, et ce qui changerait la réponse. Le modèle est dans
[Trames d'écrit](WRITEUP-TEMPLATES.md).

### 5. Un métier, expliqué

Un des vingt-six, en cinq cents mots : ce que c'est vraiment, ce qui distingue
quelqu'un de bon, par où commencer. Vingt-six mois de matière, ce qui règle la
question de quoi écrire pendant deux ans.

## Ce qu'on n'y met pas

**Pas de chiffres de croissance.** Ni nombre d'inscrits, ni nombre de
livrables. La séquence est communauté → preuves → visibilité → entreprises, et
une lettre qui parle de sa propre croissance parle déjà aux entreprises.

**Pas de contenu généré.** Une lettre sur le métier de designer écrite par une
machine est une contradiction qui se voit à la première phrase.

**Pas de sponsors.** Tant que la communauté est petite, un encart payé rapporte
peu et coûte la confiance.

## Comment elle part

Par la même infrastructure que le reste : `notification_kinds`, catégorie
`learning`, préférence par personne. Une lettre à laquelle on ne peut pas se
désabonner en un clic est un problème juridique avant d'être un problème de
goût.

**Ce qui manque côté backend :** un `kind` pour la lettre elle-même, et un
travail périodique qui la compose. Ce n'est pas fait, parce qu'il n'y a pas
encore de premier numéro — et écrire le générateur avant d'avoir écrit un
numéro à la main produirait un gabarit calqué sur rien.

L'ordre est : trois numéros à la main, puis on automatise ce qui s'est révélé
constant.

## Le podcast

Reporté, et pour une raison précise plutôt que par manque de temps : un
podcast demande un rythme, et un rythme demande de la matière et des invités.
Les deux viennent de la communauté, qui n'existe pas encore.

Ce qui peut se faire tout de suite et coûte une heure : **enregistrer une
soutenance**. Un designer défend son travail devant un relecteur, en direct.
C'est exactement le format que la plateforme défend, ça n'a besoin d'aucun
invité extérieur, et ça fait un épisode.

---

*Voir aussi : [Salons Discord](DISCORD-STRUCTURE.md),
[Trames d'écrit](WRITEUP-TEMPLATES.md).*
