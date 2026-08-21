# Clauses contractuelles B2B

Ce document n'est pas un contrat. C'est la liste des clauses que chaque
contrat Skilluv doit contenir, avec la raison de chacune — pour qu'un juriste
rédige à partir de quelque chose, et pour que personne ne signe un contrat qui
contredit ce que le code fait déjà.

**Statut juridique** : aucun de ces contrats n'a été relu par un juriste. La
SAS n'existe pas encore. Ce document est ce qui sera remis au juriste, pas ce
qui sera signé.

---

## 1. Ce qui vaut pour tous les contrats

### Le droit applicable et le for

Skilluv opérera depuis le Bénin avec des clients en zone UEMOA et en Union
européenne. Chaque contrat nomme un droit applicable et une juridiction. Ne
pas le faire revient à laisser le premier litige décider à notre place, dans
le pays du client.

### Ce que Skilluv ne garantit jamais

Trois choses reviennent dans toutes les discussions commerciales et doivent
être exclues par écrit :

- **le résultat d'un travail** — nous garantissons la relecture, pas la
  réussite d'un produit ;
- **la disponibilité d'une personne nommée** — les gens partent, changent
  d'avis, tombent malades. Une clause qui promet Untel est une clause que nous
  ne pouvons pas tenir ;
- **le comportement d'un tiers** — un contributeur n'est pas un salarié
  Skilluv, et la plateforme ne peut pas répondre de ce qu'il fera après avoir
  été présenté.

### Ce que Skilluv garantit

- que la personne présentée est bien celle dont les preuves sont publiées ;
- que ce qui a été relu l'a été selon la grille annoncée ;
- que le paiement dû est versé selon les modalités annoncées, y compris quand
  le client conteste (voir la garantie de paiement, section 6).

### Confidentialité

Bidirectionnelle et bornée dans le temps. Une clause perpétuelle est
inapplicable et personne ne l'a jamais fait respecter.

### Propriété intellectuelle

Quatre régimes, déjà encodés dans `team_engagements.ip_terms` et
`missions.ip_terms` :

| Régime | Ce que le client obtient | Ce que le contributeur garde |
|---|---|---|
| `full_ownership_client` | Tout le livrable | Rien |
| `open_source_output` | Le livrable sous licence ouverte | Le droit de le réutiliser |
| `retain_reusable_components` | Le travail spécifique au domaine | Les briques génériques |
| `dual_license` | Le livrable | Le droit de le publier aussi |

Le régime est choisi **avant** le début du travail. Négocier la PI après que
le travail existe, c'est négocier au moment où la personne qui l'a fait n'a
plus de levier.

La base refuse par ailleurs un engagement qui promet la propriété au client
alors que la licence amont l'interdit (`upstream_license_spdx`, migration
0232).

### Sous-traitance en cascade

Interdite sans accord écrit. Un client qui a choisi une équipe sur la foi de
preuves publiques ne doit pas se retrouver avec une autre.

### Résiliation

Chaque contrat dit ce qui se passe pour le travail en cours, pas seulement
pour l'abonnement. Un contrat qui s'arrête sans dire qui paie le jalon
commencé produit exactement un litige.

---

## 2. Recrutement

Adossé à `recruitment_campaigns`, `recruitment_success_fees` et
`enterprise_contests`.

- **honoraires de succès** : pourcentage du salaire annuel déclaré, plafonné
  à 30 % par la base. Le taux est gelé à l'ouverture de la campagne ;
- **garantie de remplacement** : durée en jours, stockée par contrat parce que
  négociable. Le remboursement est **dégressif** — intégral avant un quart de
  la période, la moitié avant la moitié, un quart ensuite ;
- **ce qui déclenche la garantie** : le départ de la personne ou son
  licenciement. Ni une restructuration, ni la fin normale du contrat. Écrit
  explicitement, parce que la version large transforme la garantie en clause
  de remboursement pour tout ;
- **consentement du candidat** : nommer quelqu'un sur une liste courte exige
  son accord. La base le refuse autrement, et le contrat doit le dire au
  client avant qu'il ne s'étonne ;
- **anti-contournement** : recruter une personne présentée par Skilluv en
  dehors de la plateforme dans les douze mois déclenche les mêmes honoraires.
  Clause standard du métier, et la seule qui protège un intermédiaire.

## 3. Prestation d'équipe (Studios, sous-traitance, sprints)

Adossé à `team_engagements` et `engagement_milestones`.

- **jalons** : chaque jalon porte son critère d'acceptation, écrit à
  l'avance. Un jalon défini après coup est un jalon discuté ;
- **double porte** : Skilluv relit avant le client. Le contrat dit que le
  client ne reçoit rien qui n'ait passé cette relecture — c'est ce que la
  marge achète ;
- **paiement** : la totalité est versée à l'ouverture et libérée jalon par
  jalon. Le contrat nomme le tiers qui détient les fonds ;
- **répartition** : la part de chaque contributeur est convenue avant le
  démarrage et totalise 100 %. Le contrat n'a pas à la détailler au client,
  mais doit dire qu'elle existe ;
- **retard** : qui supporte quoi, et à partir de quand. Sans cette clause,
  chaque semaine de retard est une négociation.

## 4. Sponsoring et événements

Adossé à `event_sponsorships` et `sponsorship_leads`.

- **ce qui est acheté** : la formule, écrite, avec ce qu'elle comprend. Le
  prix négocié figure sur le contrat, jamais dans la grille publiée ;
- **contacts** : le sponsor reçoit les coordonnées des personnes **qui ont
  consenti**, et le nombre de celles qui ne l'ont pas fait. Le contrat le dit,
  parce qu'un sponsor qui découvre ça après l'événement se croit lésé ;
- **usage des contacts** : une prise de contact, pas une inscription à une
  base de prospection. Durée d'usage bornée ;
- **annulation de l'événement** : ce qui est remboursé et ce qui ne l'est pas.

## 5. Données

Adossé à `data_licensing_contracts` et `talent_data_consent`. Le détail est
dans [DATA-LICENSING.md](DATA-LICENSING.md) ; le contrat en reprend quatre
points :

- **finalité déclarée**, opposable. Un usage hors finalité met fin à la
  licence ;
- **pas de ré-identification**, ni tentative, ni croisement avec un autre jeu
  de données à cette fin ;
- **pas de cession à un tiers** sans avenant ;
- **propagation des retraits** : une personne qui retire son consentement
  sort du jeu suivant, et le licencié s'engage à la supprimer sous trente
  jours des jeux déjà reçus.

## 6. Garantie de paiement et avances

Adossé à `payment_guarantee_subscriptions` et `advance_pay_requests`.

- **l'avance n'est pas un prêt** : elle porte sur une facture émise, ne peut
  pas la dépasser, et se rembourse dessus. Le contrat le formule ainsi parce
  que c'est ce qui la tient hors du crédit réglementé ;
- **impayé client** : Skilluv le porte. Le contributeur garde l'argent. C'est
  la raison d'être des frais, et le contrat le dit ;
- **plafonds** : par mission et par an, tous deux écrits.

## 7. Labels et certifications

Adossé à `certifications`. Voir aussi
[CERTIFICATION-LEGAL.md](CERTIFICATION-LEGAL.md).

- **durée** : un label expire. Le contrat dit à quelle date et ce qui se
  passe ensuite ;
- **retrait** : Skilluv peut retirer un label si les faits qui l'ont fondé ne
  sont plus vrais, avec motif écrit et préavis ;
- **usage de la marque** : où le logo peut figurer, dans quelle forme, et
  jusqu'à quand après expiration — c'est-à-dire pas après.

## 8. Programme entreprise annuel

Le contrat qui regroupe plusieurs produits. Trois points supplémentaires :

- **ce que le forfait comprend**, ligne par ligne, avec les quotas. Les
  quotas vivent dans `enterprise_entitlements` et le contrat reprend les
  mêmes nombres ;
- **ce qui n'est pas reporté** d'une année sur l'autre. Un quota non consommé
  qui semble reportable est le premier litige de renouvellement ;
- **révision annuelle** : les prix peuvent bouger au renouvellement, jamais en
  cours d'année.

---

## 9. Ce qu'il reste à faire

- faire relire l'ensemble par un juriste spécialisé données et RGPD
  (ticket 15-02) ;
- rédiger les contrats eux-mêmes à partir de ce document ;
- créer la structure juridique qui les signera.

Ce document est en vigueur avant les contrats qu'il décrit, dans cet ordre
délibérément : l'inverse produit des contrats qui décident de la politique.
