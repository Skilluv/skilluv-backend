# Politique de licence de données

Ce que Skilluv accepte de vendre à partir des données qu'elle détient, à qui,
et ce qu'elle refuse.

**Cette politique prime sur toute négociation commerciale.** Un contrat qui la
contredit n'est pas une exception, c'est un contrat à ne pas signer.

---

## 1. Ce qui peut faire l'objet d'une licence

Trois catégories, et une quatrième qui n'existe pas.

**Statistiques agrégées.** Répartition des compétences par pays, par langage,
par métier. Aucune ligne ne concerne une personne identifiable. Seuil minimum
de vingt personnes par cellule : en dessous, une statistique sur trois
personnes en désigne trois personnes.

**Tendances.** Évolution des compétences demandées, des technologies
utilisées, des délais de livraison. Même seuil.

**Fourchettes de rémunération.** Ce que paient les missions, par métier et par
région. Agrégées, jamais nominatives, jamais reliées à un employeur
identifiable.

**Profils individuels : uniquement sur consentement explicite** de la personne
concernée, avec une part des revenus pour elle (§4). Sans ce consentement,
aucun profil ne sort — pas anonymisé, pas pseudonymisé, pas du tout.

### La quatrième catégorie, celle qui n'existe pas

Rien de ce qui permettrait de reconstituer une personne à partir de données
prétendument anonymes. Un jeu de données « anonymisé » contenant le pays, le
métier, l'année de première contribution et trois langages désigne souvent une
seule personne. Toute demande dont la granularité approche ce seuil est
refusée, même si chaque champ pris isolément est anodin.

---

## 2. Le consentement

Pour tout ce qui touche à un profil individuel :

- **explicite** — une case décochée par défaut, jamais un consentement déduit
  de l'inscription ;
- **informé** — la personne voit quelles données, à quel type d'acheteur, et
  ce qu'elle touche ;
- **révocable à tout moment**, avec effet sur les licences en cours : un
  acheteur qui reçoit des mises à jour cesse de recevoir cette ligne, et
  s'engage contractuellement à la supprimer sous trente jours ;
- **tracé** — chaque licence accordée est enregistrée et la personne concernée
  est notifiée.

Le consentement à une licence de données n'est jamais une condition d'accès à
autre chose. Un talent qui refuse garde exactement les mêmes fonctionnalités.

---

## 3. Qui peut acheter, et qui ne peut pas

### Acheteurs recevables

- **Recherche académique et publique** — gratuitement, contre publication des
  résultats.
- **Éditeurs de logiciels de recrutement**, pour enrichir un produit, sans
  revente.
- **Institutions financières**, pour de l'analyse de marché agrégée.
- **Organismes publics et bailleurs**, pour des politiques de formation.
- **Assureurs**, pour de la tarification agrégée uniquement.

### Acheteurs refusés

Sans discussion possible, et la liste est dans le contrat :

- **la surveillance de masse** — toute finalité de suivi de personnes non
  suspectes ;
- **la discrimination** — toute utilisation visant à écarter sur une origine,
  une nationalité, un genre, un âge, une santé, une orientation, une
  appartenance ;
- **l'identification en vue d'une mesure coercitive** — police de
  l'immigration, application d'une législation pénale à partir de données de
  compétences ;
- **la revente**, sous quelque forme que ce soit ;
- **l'entraînement de modèles** sans consentement spécifique et distinct.

Un acheteur qui refuse de déclarer sa finalité est refusé. Une finalité qui
change après signature met fin au contrat sans remboursement.

---

## 4. La part des talents

**0,5 à 2 % des revenus de licence**, reversés aux personnes dont les données
sont dans le jeu concerné, au prorata.

Le taux dépend de la granularité : plus une donnée est individuelle, plus la
part est haute. Une statistique agrégée où une personne pèse un millième
reverse peu ; un profil nommé dans un jeu de données de recrutement reverse
davantage.

Les montants sont petits — c'est honnête de le dire plutôt que de le présenter
comme un revenu. Ce qui compte n'est pas la somme, c'est que **la donnée de
quelqu'un ne rapporte jamais uniquement à la plateforme.**

Versé sur le portefeuille du talent, visible dans son relevé, avec la mention
de la licence qui l'a produit.

---

## 5. RGPD et droit à l'effacement

Skilluv traite des données de personnes situées dans l'Union européenne et en
Afrique de l'Ouest. La base légale pour la licence est le **consentement**, ce
qui implique le droit de le retirer, à tout moment, sans justification.

Concrètement :

- une demande d'effacement est traitée sous trente jours ;
- elle est **propagée aux licenciés**, contractuellement tenus de supprimer ;
- une donnée déjà agrégée dans un rapport publié n'est pas récupérable, ce qui
  est dit à la personne **avant** qu'elle ne consente, pas après qu'elle
  demande ;
- l'attestation publique fait exception : c'est une preuve émise, dont la
  vérifiabilité est le sens. Elle peut être révoquée par son détenteur, ce qui
  la rend invérifiable — mais pas rétroactivement effacée des endroits où elle
  a été montrée.

---

## 6. Journal

Chaque licence accordée écrit une ligne : quel acheteur, quelles données,
quelle finalité déclarée, quelle date, quelle durée, quel montant, quelle part
reversée.

Ce journal est consultable par toute personne dont les données y figurent,
pour ce qui la concerne. Une politique de données sans journal consultable est
une déclaration d'intention.

---

## 7. Ce que l'outillage garantit

Cette politique a été écrite avant son implémentation, délibérément, parce que
l'inverse produit un outillage qui décide de la politique. L'implémentation
existe désormais, et voici ce qu'elle rend impossible plutôt que déconseillé.

**Le consentement est par finalité.** Quatre finalités distinctes (score
public via l'API, recherche académique, licence commerciale, profil unifié),
une ligne par personne et par finalité. Accepter d'apparaître dans une API de
scores n'accepte rien d'autre. Il n'existe aucune route qui accorde un
consentement à la place de quelqu'un.

**Le texte accepté est copié sur la ligne de consentement.** La description
d'une finalité sera reformulée avec le temps ; un consentement donné à
l'ancienne formulation n'a pas été donné à la nouvelle. Ce qui peut être
produit lors d'un audit est ce qui était réellement à l'écran.

**Un retrait conserve la ligne.** Un consentement révoqué prouve qu'un
consentement existait pendant la période où un jeu de données a été construit,
et supprimer la ligne rendrait ce fait indémontrable précisément dans l'audit
où il compte.

**La population couverte est relue à chaque échéance**, jamais recopiée dans
une liste. Quelqu'un qui s'est retiré la semaine dernière n'est pas payé et ne
figure pas dans le jeu livré.

**Un plancher de trente personnes.** Aucun rapport, aucune licence, aucune
statistique ne peut être produite sur une population plus petite. Un graphique
« écart de compétences à Cotonou » tiré de quatre personnes nomme ces quatre
personnes, quel que soit son titre — et la pression commerciale va exactement
dans ce sens, ce qui est la raison pour laquelle le plancher est dans le code
et pas dans un guide de style.

**Une licence commerciale à 0 % de reversement est refusée par la base.** Zéro
se défend pour un jeu de données de recherche publique ; pas pour une vente.

**Le plafond de reversement est de 20 %** dans le schéma, la valeur par défaut
de 1 %, et la bande annoncée en section 4 reste 0,5 à 2 %. Le plafond existe
pour qu'une négociation ne puisse pas écrire un nombre absurde, pas pour être
atteint.

**L'API publique ne dit rien de quelqu'un qui n'a rien accepté** — et répond
« introuvable » plutôt que « privé ». Un annuaire construit à partir des refus
serait un annuaire de toutes les personnes ayant refusé, ce qui est encore une
information qu'elles n'ont pas acceptée de partager.

**Une reconnaissance officielle exige un contrat signé.** Une instance
gouvernementale ne peut déclarer reconnaître quoi que ce soit sans convention
signée jointe : sans elle c'est une affirmation, et ce sont les porteurs de
l'attestation qui découvriraient qu'elle ne valait rien.
