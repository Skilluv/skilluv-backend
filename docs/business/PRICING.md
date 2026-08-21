# Grille tarifaire

**Version 2 — chiffres cibles, à valider par le marché.**

Aucun de ces prix n'a encore été payé par personne. Ils sont écrits pour
pouvoir être discutés, pas parce qu'ils sont établis. Le seul chiffre qui ait
été implémenté et gelé dans le code est la commission sur mission (§3).

Trois monnaies, parce que trois marchés : **XOF** pour l'Afrique de l'Ouest,
**EUR** pour l'Europe, **USD** pour le reste. Les montants ne sont pas des
conversions les uns des autres — un tarif européen converti en XOF est un
tarif que personne à Cotonou ne paiera.

---

## 1. Talent

### Packs de crédits

Les crédits paient la recherche et les mises en relation. Achetés d'avance,
sans expiration : un crédit qui expire est un crédit vendu deux fois.

| Pack | Crédits | XOF | EUR | USD |
|---|---|---|---|---|
| Starter | 50 | 60 000 | 90 | 100 |
| Growth | 200 | 200 000 | 300 | 330 |
| Pro | 600 | 500 000 | 760 | 830 |
| Enterprise | 2 000 | 1 500 000 | 2 300 | 2 500 |

La remise au volume va jusqu'à environ 25 %. Au-delà, le pack devient moins
cher que ce qu'il coûte à servir sur les gros comptes qui l'utilisent
réellement.

### Abonnement pipeline

Le suivi de candidatures. Mensuel, résiliable au mois.

| Palier | Recruteurs | XOF/mois | EUR/mois | USD/mois |
|---|---|---|---|---|
| Starter | 2 | 30 000 | 45 | 50 |
| Growth | 10 | 90 000 | 140 | 150 |
| Scale | illimité | 250 000 | 380 | 420 |

### Recrutement comme service

Campagne menée de bout en bout. **Frais de mise en place** de 300 000 XOF /
450 EUR / 500 USD, puis **8 % du salaire annuel brut** à l'embauche confirmée.

Le taux du marché est de 15 à 25 %. Le nôtre est bas délibérément : la
plateforme a déjà fait la moitié du travail de qualification, et facturer le
prix d'un cabinet pour un travail qu'on n'a pas fait est une façon rapide de
perdre le second client.

**Aucun frais si l'embauche ne se fait pas.** Pas de retainer.

### Concours de recrutement

| Formule | Ce que ça comprend | XOF | EUR |
|---|---|---|---|
| Autonome | Le concours, les inscriptions, le classement | 250 000 | 380 |
| Accompagnée | Plus la conception des épreuves et la présélection | 900 000 | 1 400 |

---

## 2. Work

### Primes

**8 % pour l'entreprise qui pose la prime.** Le contributeur reçoit le montant
annoncé — c'est le point : une prime de 50 000 XOF est une prime de 50 000
XOF, et la commission est visible côté payeur.

À réévaluer. 8 % couvre le coût de traitement et le risque de litige, sans
plus ; c'est défendable pour amorcer et probablement trop bas à l'échelle.

### Missions

**15 %, ramenés à 10 % après dix missions livrées.** Gelé à la sélection du
prestataire et recopié sur chaque facture : ce qui a été facturé en mars reste
lisible en novembre.

### Studios

Une équipe permanente, réservée par son nom. **Marge 25 %.**

Plus élevée que la sous-traitance parce que le client achète une équipe déjà
constituée, avec un historique et la coordination incluse — pas une liste de
gens disponibles cette semaine-là.

### Sous-traitance

Du travail confié à Skilluv et réparti entre contributeurs assemblés pour
l'occasion. **Marge 15 %** — plus basse que Studios parce que la coordination
est plus légère.

En dessous de 15 % la coordination n'est pas payée et se fait mal. Au-dessus
de 30 % nous sommes une SSII avec une base de données, et il y en a déjà. Les
deux bornes sont dans le code (`MARGIN_FLOOR`, `MARGIN_CEILING`) et un taux
modifié en dehors de cette bande casse la compilation des tests.

### Cadrage, sprints, placement fractionné

Trois formes de la même prestation, au même taux que la sous-traitance. Ce
qui change est la forme, pas la marge :

- **cadrage** — 2 à 6 semaines, bornées. Le livrable est une recommandation,
  et la borne existe pour qu'une exploration ouverte ne devienne pas une
  facture ouverte ;
- **sprint** — 1 à 12 semaines, cohorte fixe ;
- **placement fractionné** — une personne, 0,5 à 4 jours par semaine, sur
  plusieurs mois.

### Programmes de test

Une cohorte de testeurs rémunérés. **Récompense fixe par testeur, payée sur
retour accepté, plus un forfait d'organisation facturé séparément.**

Les deux montants restent distincts et visibles : le client doit voir ce qui
va aux testeurs et ce qui va à la plateforme. Le forfait est comptabilisé à
la clôture, pas à l'ouverture — il est gagné en livrant le rapport, et un
programme annulé la première semaine n'en a rien gagné.

Le devis annonce le maximum (récompenses × testeurs demandés + forfait), pas
la moyenne : un client qui budgète la moyenne et reçoit le maximum a été
trompé par une arithmétique.

---

## 3. Brand

Formules de sponsoring d'événement :

| Formule | Ce que ça comprend | XOF | EUR |
|---|---|---|---|
| Bronze | Logo, mention, accès aux profils des participants | 300 000 | 460 |
| Argent | Plus une épreuve co-conçue et une prise de parole | 900 000 | 1 400 |
| Or | Plus le nom sur l'événement et un accompagnement des finalistes | 2 500 000 | 3 800 |
| Platine | Événement présenté par le sponsor, rapport d'impact sur mesure | 7 900 000 | 12 000 |

Cette grille est en base (`event_sponsorship_packages`), une ligne par
formule. Un prix négocié s'écrit sur le sponsoring concerné, jamais sur la
grille : remettre la remise dans la grille réécrirait l'histoire de tous les
autres sponsors de la même formule.

Contrat annuel : jusqu'à **30 % de remise** selon le nombre d'événements
engagés, et seulement sur un contrat signé — la remise paye l'engagement, pas
l'intention. Au-delà de 30 % le contrat coûte plus à servir que les
événements couverts ne rapportent.

Contenu sponsorisé : à partir de 400 000 XOF / 600 EUR l'article,
**systématiquement signalé comme tel**. Un contenu sponsorisé non signalé
n'est pas un tarif plus élevé, c'est un refus. La mention est stockée avec la
pièce et la base refuse une pièce sans elle.

Campagne de lancement : **frais d'organisation 3 à 10 k€**, plus une cagnotte
que le client met pour les contributeurs. Deux montants distincts et visibles.
Chaque contribution passe d'abord par notre contrôle qualité, ensuite par la
décision du sponsor — dans cet ordre, sinon une critique honnête se fait
refuser au nom de la « qualité ».

Programme ambassadeurs : **activation 5 à 15 k€**, gestion **1 à 3 k€ par
mois**, plus une indemnité mensuelle versée aux ambassadeurs. L'indemnité est
proratisée sur ce qui a été livré : payée en entier quel que soit le mois,
elle transformerait le programme en abonnement que le client ne peut plus
arrêter.

Abonnement audience : **10 EUR par an**, rediffusions et coulisses. C'est la
seule chose qu'un individu paye sur Skilluv, et elle ne vend ni visibilité,
ni classement, ni accès au travail.

---

## 4. Data

### Licence de données

Uniquement agrégées et anonymisées, uniquement avec le consentement des
personnes concernées, avec une part reversée. Voir
`docs/business/DATA-LICENSING.md`.

| Formule | XOF/an | EUR/an |
|---|---|---|
| Recherche académique | gratuit | gratuit |
| Rapport ponctuel | 1 200 000 | 1 800 |
| Abonnement annuel | 4 000 000 | 6 000 |

La gratuité pour la recherche n'est pas de la générosité : un jeu de données
sur les compétences en Afrique de l'Ouest cité dans un article publié vaut
plus que ce qu'un laboratoire aurait payé.

### API de scores

| Palier | Appels/mois | XOF/mois | EUR/mois |
|---|---|---|---|
| Découverte | 1 000 | gratuit | gratuit |
| Standard | 50 000 | 120 000 | 180 |
| Volume | 500 000 | 700 000 | 1 100 |

### Marque blanche

À partir de **12 000 000 XOF / 18 000 EUR par an**, mise en place comprise. Le
chiffre est large parce que chaque déploiement est particulier ; il sert à
écarter les demandes qui n'en sont pas.

---

## 5. Ecosystem

**Marketplace des créateurs : 15 %.** Standard bas du marché, choisi pour
qu'un créateur qui vend peu gagne quand même quelque chose.

**Certifications : 50 000 XOF / 75 EUR** par parcours, payé par l'entreprise
quand elle certifie ses équipes, jamais par le talent.

**Cohorte Academy : 3 500 000 XOF / 5 300 EUR** pour douze personnes sur trois
mois.

**Abonnement formation entreprise : 15 000 XOF / 23 EUR par personne et par
mois**, à partir de dix personnes.

---

## 6. Finance

Le pilier le plus sensible, parce que c'est le seul où l'argent d'un talent
transite.

**Avance sur revenus : 3 % du montant avancé, plafonné à trente jours.** Pas
d'intérêt composé, pas de pénalité de retard : le remboursement vient de la
facture, et si la facture n'arrive pas c'est Skilluv qui porte le risque.

**Intermédiation assurance : commission d'apporteur standard**, affichée au
souscripteur. Une commission d'apporteur non affichée est un conflit
d'intérêt.

**Financement de formation : remboursement indexé sur le revenu, plafonné à
1,4 fois le montant financé, et nul en dessous d'un seuil de revenu.** Le
plafond et le seuil sont ce qui distingue ceci d'un crédit à la consommation
vendu à quelqu'un qui n'en a pas les moyens.

---

## 7. Consultation

**Conseil : 350 000 XOF / 530 EUR la journée.** Deux jours minimum — en
dessous, le temps de compréhension du contexte mange la prestation.

**Onboarding accompagné : compris** dans les programmes annuels, 600 000 XOF /
900 EUR autrement.

---

## 8. Ce que ce document n'est pas

Une promesse. Chaque chiffre ici est une hypothèse sur ce qu'un marché est
prêt à payer, et la plupart seront fausses. Les deux qui comptent aujourd'hui
sont la commission sur mission (implémentée) et la commission sur prime
(implémentée) ; le reste sera révisé au premier contact avec un vrai client.

Ce qui ne bougera pas est la règle qui les gouverne tous : **rien de ce
document n'est facturé à un talent pour accéder à une opportunité.**
