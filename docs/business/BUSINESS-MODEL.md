# Le modèle économique de Skilluv

Ce document remplace toute note de monétisation antérieure. Il décrit ce que
Skilluv vend, à qui, et — plus important — ce qu'il ne vendra pas.

Il est écrit après l'implémentation, ce qui est l'ordre inhabituel : chaque
règle citée ici est appliquée par le code ou par la base, et la référence est
donnée. Un modèle économique qu'on ne peut pas vérifier dans le schéma est une
intention.

---

## 1. La règle d'or

**Les entreprises payent. Les talents ne payent pas.**

Aucune fonctionnalité qui rend quelqu'un visible, mieux classé, mieux placé ou
plus recruté ne se paye. Cette phrase est la contrainte dont tout le reste
découle, et elle est la raison pour laquelle plusieurs idées commercialement
raisonnables ont été refusées.

Trois exceptions, et elles sont exactement trois :

| Ce qu'un individu peut payer | Pourquoi c'est acceptable |
|---|---|
| Les rediffusions d'événements (10 EUR/an) | Regarder un replay n'est pas être vu |
| La lettre d'information complète (8 EUR/mois) | De l'information de marché, pas de la visibilité |
| La garantie de paiement (5–20 EUR/mois) | Une assurance sur son propre travail |

Deux d'entre elles sont des abonnements « audience », dans la même table que
les rediffusions (`audience_plans`), et le commentaire de cette table dit
pourquoi elle est délibérément courte.

Le mentorat payant est l'apparente quatrième exception. Ce n'en est pas une :
le talent paye un autre talent, et Skilluv prend une commission sur cette
transaction comme sur n'importe quelle autre. La plateforme ne vend rien.

## 2. Les six piliers

Chaque revenu appartient à un pilier, et le pilier vit en base
(`revenue_streams.pillar`) plutôt que dans un tableur.

| Pilier | Ce qui est vendu | Qui paye |
|---|---|---|
| **Talent** | Recrutement, essais, campagnes, placements longs | L'entreprise |
| **Work** | Prestations d'équipe, missions, primes, labs, propositions | L'entreprise |
| **Brand** | Sponsoring, campagnes de contenu, ambassadeurs, concours | L'entreprise |
| **Data** | API de scores, rapports, licences, marque blanche | L'entreprise, l'institution |
| **Finance** | Avances, apports bancaires et assurantiels, garantie | Le contributeur (garantie), le partenaire (commission) |
| **Ecosystem** | Marketplace créateurs, labels, cohortes, mentorat | Partagé |

## 3. Ce que le code refuse

Ce qui suit n'est pas une liste de bonnes intentions. Chaque ligne est une
contrainte que la base ou le service applique, avec l'endroit où la lire.

**Personne n'est présenté sans avoir dit oui.** Une liste courte, une équipe
sur un engagement, un ambassadeur, une proposition d'équipe : chaque fois, la
personne nommée répond elle-même. `recruitment_shortlist`,
`engagement_members`, `program_ambassadors`, `team_proposal_members`.

**Personne n'est évalué sans le savoir.** Un audit de compétences d'équipe ne
peut rien écrire sur quelqu'un qui n'a pas été informé, et l'audit n'est pas
livrable tant que chaque personne évaluée n'a pas vu ce qui a été écrit sur
elle — avec un droit de réponse conservé à côté.
`enterprise_employee_assessments`.

**Le consentement aux données est par finalité, daté, révocable, et le texte
accepté est conservé.** Quatre finalités distinctes ; accepter l'une n'accepte
rien d'autre. `talent_data_consent`, et
[DATA-LICENSING.md](DATA-LICENSING.md).

**Aucun chiffre publié ne repose sur moins de trente personnes.** Rapports,
licences, statistiques. La pression commerciale va exactement dans l'autre
sens, ce qui est la raison pour laquelle le plancher est dans le code.
`data_consent::COHORT_FLOOR`.

**Une licence commerciale reverse une part aux personnes qu'elle contient.**
Zéro est refusé par la base pour une vente ; c'est défendable pour un jeu de
données de recherche publique.

**Un label ne s'achète pas.** Payer ne certifie pas ; réussir l'audit
certifie. Un échec laisse les frais engagés et ne donne pas le label.
`certifications`.

**Rien n'atteint le client sans être relu.** Jalons d'engagement, pièces de
campagne, contributions de lab. C'est ce que la marge achète.

**Une avance n'est pas un prêt.** Elle porte sur une facture émise, ne peut
pas la dépasser, se rembourse dessus, et l'impayé est porté par Skilluv.
`advance_pay_requests`.

**Un stagiaire financé ne doit jamais rien.** La colonne
`unplaced_owe_nothing` ne peut valoir que vrai. Refuser un poste à la fin est
une issue normale.

**Une introduction bancaire ou assurantielle exige une immatriculation.** Un
partenariat ne devient actif qu'avec une base réglementaire écrite et un
contrat signé. Le code est complet ; l'interrupteur est un document.
`financial_partnerships`.

## 4. Ce que Skilluv ne vendra pas

Écrit ici pour que la question ne se repose pas :

- **la mise en avant payante d'un profil**. C'est la règle d'or ;
- **l'accès à des personnes qui n'ont pas consenti**. L'API répond
  « introuvable », jamais « privé » — un annuaire construit à partir des refus
  serait un annuaire des personnes ayant refusé ;
- **un label sans audit** ;
- **une reconnaissance officielle sans convention signée avec l'État
  concerné** ;
- **un lab sans cagnotte**. Une entreprise qui fait travailler cent personnes
  sur son produit contre rien, avec Skilluv facturant l'organisation, n'est
  pas un produit que nous vendons ;
- **des données individuelles nommées à des fins de prospection**, quel que
  soit le prix.

## 5. Où en est réellement l'entreprise

Trois bénévoles. Zéro utilisateur. Zéro revenu. La structure juridique n'existe
pas encore.

Tout ce qui précède est construit et testé, et rien n'a jamais été facturé.
C'est délibéré et c'est le bon ordre : les règles ci-dessus sont plus faciles
à tenir avant qu'un premier client ne demande une exception.

## 6. Ce qu'il reste

- créer la structure juridique ;
- faire relire les contrats et la politique de données par un juriste ;
- signer les partenariats de paiement mobile (Orange, MTN, Wave) ;
- déposer les marques.

Aucun de ces points n'est un développement.

---

## Documents liés

- [CHARTER.md](CHARTER.md) — ce à quoi Skilluv s'engage
- [PRICING.md](PRICING.md) — la grille tarifaire
- [DATA-LICENSING.md](DATA-LICENSING.md) — la politique de données
- [CONTRACT-CLAUSES.md](CONTRACT-CLAUSES.md) — les clauses B2B
- [CERTIFICATION-LEGAL.md](CERTIFICATION-LEGAL.md) — les labels
- [STUDIOS-PLAYBOOK.md](STUDIOS-PLAYBOOK.md) — monter et vendre un studio
- [ENTERPRISE-ONBOARDING.md](ENTERPRISE-ONBOARDING.md) — accueillir un client
- [WEB3-ANALYSIS.md](WEB3-ANALYSIS.md) — pourquoi il n'y a pas de jeton
