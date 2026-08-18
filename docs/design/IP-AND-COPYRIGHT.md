# Propriété intellectuelle, droit moral et concours

*Ce document dit qui possède quoi. Il est écrit pour être lu par un designer,
pas par un juriste — mais il est écrit pour tenir devant un juriste.*

> **État de relecture.** Ce texte n'a pas encore été relu par un avocat. Il
> décrit la position de la plateforme et la façon dont le code l'applique. Les
> points signalés « à faire trancher » sont ceux où une relecture changerait
> quelque chose ; le reste est de la description, pas du conseil.

---

## 1. Le principe

**Ce que vous créez vous appartient.** Skilluv ne prend aucune cession de
droits sur les travaux publiés ici, ni sur les propositions de concours, ni sur
les livrables de challenge.

La plateforme reçoit une seule chose : une **licence d'affichage**. Elle peut
montrer votre travail sur votre profil, dans la galerie d'un concours, dans une
page de résultats, et dans ce qu'elle publie pour parler d'elle-même. Elle ne
peut pas le vendre, le concéder à un tiers, ni le modifier.

Cette licence dure tant que le contenu est publié. Vous le retirez, elle
s'éteint — sauf sur les pages qui font l'objet d'un archivage historique, dont
la liste est ci-dessous.

## 2. Le droit moral

En droit français et dans la plupart des pays de tradition civiliste, le droit
moral est **inaliénable** : il ne se cède pas, même par contrat, même contre
paiement. Il comprend notamment le droit d'être nommé comme auteur et le droit
de s'opposer à une modification qui dénature l'œuvre.

Conséquences concrètes ici :

- votre nom reste attaché au travail, y compris quand une entreprise l'a
  commandé et payé ;
- une attestation ne peut pas être émise au nom de quelqu'un d'autre ;
- un client qui reprend un travail livré ne peut pas le dénaturer et continuer
  à le signer de votre nom.

**À faire trancher :** la plateforme est destinée à opérer depuis le Bénin,
avec des utilisateurs en France et ailleurs. Le régime applicable au droit
moral dépend du pays de première publication et de la résidence de l'auteur.
Une relecture doit dire quel droit s'applique par défaut et ce qu'on écrit dans
les conditions générales.

## 3. Les concours

C'est le point le plus sensible du domaine, et il mérite d'être dit sans
détour.

### Ce qui rend un concours acceptable

Un concours de design est souvent, dans la profession, une façon de faire
travailler quarante personnes pour en payer une. Ce qui sépare un concours
légitime de ce travail spéculatif tient à **une seule question** : l'argent
était-il là avant le brief ?

La plateforme y répond structurellement plutôt qu'éditorialement. Un concours
doté ne peut pas s'ouvrir aux participants tant que la somme n'est pas
séquestrée — c'est une contrainte de base de données, pas une bonne intention
(migration `0242`). Personne ne peut concourir pour une dotation qui n'existe
pas.

### Ce que le lauréat cède

Rien, par défaut. Gagner un concours ne transfère aucun droit.

Si le commanditaire veut acquérir des droits d'exploitation sur la proposition
lauréate, c'est un **contrat séparé**, négocié après le classement, entre le
lauréat et lui. La plateforme ne le signe pas à sa place et n'en prend pas de
commission — la dotation est versée entière au podium.

### Ce que les autres participants gardent

Tout. Une proposition non retenue reste la propriété de son auteur, qui peut la
retravailler, la vendre ailleurs, ou la publier.

**Ce qu'un commanditaire ne peut pas faire :** reprendre une idée d'une
proposition non retenue. C'est une clause explicite des conditions de concours,
et la galerie publique des propositions est ce qui permet de le constater.

### Le débauchage

Un commanditaire qui découvre un profil dans un concours a le droit de le
contacter — c'est une des raisons d'être de la plateforme. Ce qui est interdit,
c'est de **court-circuiter un concours en cours** : contacter les participants
pendant la fenêtre de soumission pour leur proposer de travailler en direct
vide le concours de son objet et lèse ceux qui y sont restés.

**À faire trancher :** la durée de la clause de non-sollicitation après la
clôture, et si elle est opposable dans les pays visés.

## 4. La fenêtre aveugle

Un concours peut déclarer une fenêtre de soumission aveugle : tant qu'elle est
ouverte, un participant ne voit que sa propre proposition.

Ce n'est pas de l'opacité. À la clôture, tout le champ devient public et le
reste — un résultat que personne ne peut vérifier contre l'ensemble des
propositions n'est pas un résultat. Ce que la fenêtre retire, c'est seulement
la possibilité de lire le travail des autres **pendant qu'il est encore temps
de s'en inspirer**, qui est l'échec connu du format.

Le jury n'est jamais aveuglé : un jury qui ne peut pas lire les propositions ne
peut pas les juger.

## 5. Les éléments tiers

Un livrable qui contient des éléments dont vous n'êtes pas l'auteur doit les
déclarer : photographie, police de caractères, illustration, modèle 3D, son,
bibliothèque de composants.

Pour chacun : la source et la licence. Une police sous licence bureau utilisée
dans une identité livrée à un client est un piège classique — la licence du
client n'est pas la vôtre, et c'est lui qui recevra la facture.

Une déclaration manquante est un motif de refus (`third_party_unclear`), pas
une faute morale. Une déclaration fausse est du plagiat.

## 6. Les données personnelles dans un portfolio

Un travail de design contient souvent des données qui ne sont pas les vôtres :
captures d'écran avec des noms réels, entretiens utilisateurs, photographies de
personnes identifiables, documents internes d'un client.

Règles :

- **anonymisez** avant de publier — noms, adresses, visages, identifiants ;
- une photographie de personne identifiable demande son accord, écrit ;
- un document sous accord de confidentialité ne se publie pas, même flouté ;
- un entretien utilisateur se cite sans nommer la personne.

Un travail que vous ne pouvez pas publier peut quand même être validé : la
critique peut se faire sur un lien à accès restreint, avec l'attestation
mentionnant que le livrable est privé. Ce qui est perdu, c'est la démonstration
publique, pas la preuve.

**À faire trancher :** la position exacte sur les portfolios contenant des
données d'un client européen, au regard du RGPD, quand l'hébergement est hors
UE. Voir aussi la note générale sur la donnée dans `docs/RLS-ENFORCEMENT.md`.

## 7. Ce que la plateforme archive malgré un retrait

Trois choses survivent à la suppression d'un contenu, et il est honnête de le
dire avant plutôt qu'après :

1. **Le classement d'un concours conclu.** Retirer une ligne réécrirait le
   classement de tous ceux qui étaient derrière.
2. **La trace d'une attestation révoquée.** Une révocation qui disparaît
   n'informe personne.
3. **Le journal d'audit.** Il est en ajout seul par construction
   (`docs/AUDIT-APPEND-ONLY.md`), et il ne contient pas d'œuvre — seulement des
   identifiants et des dates.

Dans les trois cas, ce qui est conservé est le fait, pas l'image.

---

*Voir aussi : [Charte du domaine Design](CHARTER.md), et pour le domaine code
[`docs/code/LEGAL.md`](../code/LEGAL.md), dont ce document reprend la
structure.*
