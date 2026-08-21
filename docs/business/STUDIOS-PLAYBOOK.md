# Studios — comment on en monte un, et comment on le vend

Un studio Skilluv est une équipe permanente qu'un client réserve par son nom.
Ce document dit ce qu'il faut pour en constituer un, ce qui le distingue d'une
équipe assemblée pour l'occasion, et pourquoi il coûte plus cher.

---

## 1. Ce qu'un studio est, et n'est pas

**Est** : trois à quinze personnes qui travaillent ensemble régulièrement, ont
un domaine annoncé, un tarif journalier, et des parts de revenus convenues une
fois pour toutes.

**N'est pas** : une liste de gens disponibles cette semaine-là. C'est
exactement ce que la sous-traitance vend, à 15 % de marge au lieu de 25 %, et
la différence de prix est la différence entre les deux choses.

Ce qu'un client achète en plus, pour ces dix points :

- une équipe qui a déjà livré ensemble ;
- une coordination assurée par le studio, pas par le client ;
- des parts convenues à l'avance, donc pas de renégociation à chaque mission.

## 2. Constituer un studio

**Deux personnes au minimum.** Une seule est un indépendant, et Skilluv a déjà
un endroit pour ça. Le service le refuse.

**Une spécialisation écrite.** « Nous faisons de tout » est un job board avec
un nom. Ce qui est écrit ici est ce que le client lit avant de réserver.

**Des parts qui totalisent 100 %.** Elles sont recopiées sur chaque engagement
que le studio prend : une erreur ici est une erreur sur tout le travail futur
de l'équipe. Le service refuse l'activation autrement, avec le total actuel
dans le message.

**Un chef d'équipe qui est dans l'équipe.** Évident, et vérifié.

Un studio en formation n'est pas réservable et n'apparaît sur aucune liste
publique. Réserver une équipe en cours de constitution, c'est réserver des
gens qui n'ont pas encore été recrutés.

## 3. Fixer les parts

Le sujet le plus difficile, et celui qui casse les studios.

**Trois répartitions qui marchent :**

- **égale** — le plus simple, le plus solide, et ce que la plupart des petites
  équipes devraient prendre. Trois personnes à 33,33 / 33,33 / 33,34 ;
- **par rôle** — quand quelqu'un fait la relation client et le suivi en plus
  du travail technique. Une prime de 5 à 10 points, pas davantage ;
- **par apport** — quand une personne apporte le client. Une part
  supplémentaire sur cet engagement précis, pas sur tous.

**Ce qui casse :** une part fondée sur l'ancienneté dans le studio, sur qui a
eu l'idée, ou sur une hiérarchie implicite jamais discutée. Ces trois-là
produisent la même conversation dix-huit mois plus tard, et elle finit
généralement par un départ.

**Le conseil** : écrire les parts, les relire ensemble à voix haute, et fixer
d'avance la date où on les rediscute. Un studio qui n'a jamais rediscuté ses
parts en a de mauvaises et ne le sait pas encore.

## 4. Prendre un engagement

Quatre formes, toutes dans `team_engagements` :

| Forme | Durée | Ce que c'est |
|---|---|---|
| Sous-traitance | libre | Un projet confié |
| Cadrage | 2 à 6 semaines | Le client a une question, pas un brief |
| Sprint | 1 à 12 semaines | Cohorte fixe, intense |
| Placement fractionné | mois | Une personne, 0,5 à 4 jours par semaine |

Un studio peut prendre les quatre. Les bornes de durée sont dans la base
parce qu'un cadrage sans borne devient une facture sans borne.

**Le point à ne pas rater** : quand un studio prend un engagement, ses membres
et leurs parts sont recopiés automatiquement. C'est tout l'intérêt d'une
équipe permanente — la répartition a été convenue une fois.

## 5. Les jalons et l'argent

Le client paye la totalité à l'ouverture. Rien n'atteint personne avant
qu'un jalon ne soit accepté, et alors :

1. la part du jalon dans le contrat est calculée ;
2. la marge Skilluv est prélevée ;
3. le reste est réparti selon les parts convenues ;
4. chaque part arrive en solde **en attente**, avec le délai de libération
   habituel.

**Chaque jalon porte son critère d'acceptation, écrit avant de commencer.** Un
jalon défini après coup est un jalon discuté, et c'est la personne qui a fait
le travail qui perd la discussion.

**Skilluv relit avant le client.** Un jalon ne peut pas atteindre le client
sans être passé par là — la base le refuse. C'est ce que la marge achète, et
c'est ce qui distingue un studio d'une place de marché de freelances.

## 6. Démarrer

Le service refuse de lancer un engagement tant que :

- une personne nommée dessus n'a pas accepté sa part. Personne n'est mis au
  travail payé sans avoir dit oui, et une part modifiée annule l'accord
  précédent ;
- les parts ne totalisent pas 100 % ;
- les jalons ne couvrent pas 100 % du contrat. Le reste n'aurait nulle part
  d'où être payé.

Ces trois refus sont des messages, pas des codes d'erreur : ils disent qui
manque, combien il manque, et pourquoi.

## 7. Dissoudre

Un studio se dissout avec un motif écrit, et pas tant qu'un engagement tourne
sous son nom — cela laisserait un client avec une équipe qui n'existe plus.

Le motif est demandé parce que des gens se sont construit une réputation sous
ce nom, et qu'ils sont en droit de savoir ce qui a été enregistré.

## 8. Le studio certifié extérieur

Un studio qui n'est pas de Skilluv peut payer un label et apparaître dans la
place de marché de sous-traitance. Ce n'est pas la même chose et le vocabulaire
ne doit pas les confondre :

| | Studio Skilluv | Studio certifié |
|---|---|---|
| Membres | Des contributeurs de la plateforme | Les salariés d'une autre entreprise |
| Parts | Dans notre base | Leur affaire |
| Relecture | Par Skilluv | Selon notre méthode, par eux |
| Marge | 25 % de l'engagement | Un label annuel |

Voir [CERTIFICATION-LEGAL.md](CERTIFICATION-LEGAL.md) pour ce que le label
engage.
