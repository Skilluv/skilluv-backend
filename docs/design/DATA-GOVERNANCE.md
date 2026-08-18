# Données personnelles et portfolios design

*Ce que la plateforme stocke, ce qu'elle publie, ce qu'elle supprime, et ce
qu'un designer a le droit de mettre dans un portfolio.*

> **État de relecture.** Non relu par un juriste. Les points « à faire
> trancher » sont ceux où une relecture changerait le comportement du code ;
> le reste décrit ce qui est déjà implémenté.

---

## 1. Pourquoi le design pose un problème que le code ne pose pas

Un livrable de code est du code : il ne contient pas de visages, rarement des
noms, et jamais d'entretiens.

Un livrable de design en contient tout le temps. Une maquette porte des noms
d'utilisateurs, une étude porte des verbatims, un rendu d'architecture porte
une adresse, une vidéo porte des voix. Et un portfolio est **fait pour être
public** : c'est son objet.

D'où un document séparé.

## 2. Ce que la plateforme stocke pour un designer

| Donnée | Où | Public |
| --- | --- | --- |
| Compte : identifiant, nom affiché, e-mail | `users` | e-mail jamais public |
| Métiers déclarés (max. 3) | `user_orientations` | oui |
| Réponses du questionnaire d'accueil | `user_domain_profiles` | **non** |
| Comptes externes déclarés | signaux externes | oui, en tant que lien |
| Livrables validés et leur adresse | `deliverables` | oui, sauf marqués privés |
| Trace des tours de critique | `slice_validation_decisions` | oui |
| Attestations | `attestations` | oui |
| Participations et classements de concours | `tournament_participants` | oui |

Les réponses du questionnaire ne sont **pas** publiques et n'entrent dans aucun
score. Elles servent à trier ce qu'on vous montre. C'est écrit dans le module
qui les gère, et c'est une décision qu'il faut pouvoir défendre : un niveau
déclaré affiché publiquement deviendrait une prétention à vérifier.

## 3. Ce qu'un designer peut publier

### Anonymisez d'abord

Avant de publier un travail contenant des données qui ne sont pas les vôtres :

- remplacez les noms réels par des noms fictifs, y compris dans les captures ;
- floutez ou remplacez les visages, sauf accord écrit ;
- retirez adresses, numéros, identifiants de compte, codes-barres ;
- citez un entretien sans nommer la personne, même par son prénom si le
  contexte l'identifie.

### Le cas d'un client

Un travail réalisé pour un client se publie **si le client l'accepte**, et
uniquement dans les limites qu'il pose. Un accord de confidentialité prime,
même sur un travail dont vous êtes l'auteur : le droit moral vous donne la
paternité, pas le droit de divulguer ce qui a été confié.

### Le livrable privé

Un travail qui ne peut pas être publié peut quand même être validé. La critique
se fait sur un lien à accès restreint, et le livrable est enregistré comme non
public : l'attestation existe, la preuve compte pour le rang, et seule la
démonstration publique est perdue.

C'est le compromis honnête pour du travail sous NDA, et il vaut mieux que les
deux mauvaises réponses habituelles — publier quand même, ou ne rien pouvoir
prouver.

## 4. Ce qui se supprime, et ce qui ne se supprime pas

Une demande de suppression de compte retire : l'e-mail, le nom, les réponses du
questionnaire, les comptes externes déclarés, les messages privés.

Trois choses survivent, et il est plus honnête de le dire avant qu'après :

1. **Le classement d'un concours conclu.** La ligne devient anonyme — le code
   la renvoie sans nom plutôt que de la faire disparaître, parce que la retirer
   réécrirait le classement de tous ceux qui étaient derrière.
2. **Le fait d'une attestation révoquée.** Une révocation qui disparaît
   n'informe plus personne, ce qui est exactement ce qu'un compte révoqué
   voudrait.
3. **Le journal d'audit.** En ajout seul par construction. Il contient des
   identifiants et des dates, pas d'œuvre.

Dans les trois cas, ce qui reste est un fait, pas une identité.

**À faire trancher :** la durée de conservation du journal d'audit, et si une
pseudonymisation des identifiants après N années est possible sans casser sa
propriété d'ajout seul.

## 5. Les signaux externes

Un compte Behance, Dribbble, ArtStation, Vimeo ou Foundry se **déclare**. Rien
n'est importé : ni les projets, ni les statistiques, ni les images.

Trois raisons, dans l'ordre :

1. **Une preuve importée n'est pas une preuve.** Un score qu'on peut faire
   monter en connectant un compte cesse de vouloir dire « prouvé ici ».
2. **La provenance juridique.** Importer une image, c'est la reproduire sur nos
   serveurs — un acte de reproduction dont rien ne garantit qu'il est licite
   pour un travail commandé.
3. **La minimisation.** Ne pas détenir une donnée est la seule façon sûre de ne
   pas la perdre.

Ce qui est stocké est donc une URL et un identifiant public, rien d'autre.

## 6. Hébergement et transferts

**À faire trancher, et c'est le point le plus important de ce document.**

La plateforme est destinée à opérer depuis le Bénin, avec des utilisateurs en
France et ailleurs en Europe. Le Bénin ne fait pas l'objet d'une décision
d'adéquation de la Commission européenne.

Conséquences à instruire avec un juriste :

- quelle base légale pour le traitement des données d'utilisateurs européens ;
- quel mécanisme de transfert — clauses contractuelles types, très
  probablement — et quelle analyse d'impact ;
- où sont hébergés les fichiers déposés, qui sont la donnée la plus sensible
  puisqu'ils peuvent contenir les données de clients de nos utilisateurs ;
- qui est responsable de traitement quand un designer publie les données d'un
  client sur son portfolio : lui, nous, ou les deux.

En attendant cette instruction, la conduite tenue est la plus prudente : rien
n'est importé, les fichiers ne sont pas rendus publics par défaut, et
l'anonymisation est demandée avant publication plutôt que corrigée après.

## 7. Ce qu'on demande à une entreprise

Une entreprise qui publie un brief ou une mission ne reçoit **aucune donnée
personnelle** au-delà de ce qui est déjà public sur un profil : identifiant,
nom affiché, métiers déclarés, livrables publics, attestations.

Pas d'e-mail, pas d'adresse, pas de coordonnées. La mise en relation passe par
la plateforme, ce qui la rend traçable et laisse le talent maître de ce qu'il
donne.

---

*Voir aussi : [Propriété intellectuelle](IP-AND-COPYRIGHT.md),
[Charte](CHARTER.md), et pour la mise en œuvre technique
[`docs/RLS-ENFORCEMENT.md`](../RLS-ENFORCEMENT.md).*
