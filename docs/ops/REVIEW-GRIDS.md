# Grilles de relecture — ops

Cinq familles, cinq grilles. Chacune dit ce qu'un relecteur regarde et,
surtout, ce qui fait échouer une relecture quel que soit le reste.

Le principe commun : **on relit ce qui tournera chez quelqu'un**, pas ce qui
est joli. Un module élégant qui laisse un port ouvert est refusé ; un module
laid qui ne laisse rien passer est accepté avec une remarque.

---

## Ce qui fait échouer toutes les familles

Trois refus sans discussion, quel que soit le contexte invoqué :

1. **un secret en clair** dans un dépôt, un manifeste, une variable
   d'environnement commitée, une capture d'écran ;
2. **un accès plus large que nécessaire** « en attendant » — un rôle
   administrateur, un groupe de sécurité ouvert à 0.0.0.0/0, un compte de
   service partagé ;
3. **rien qui permette de revenir en arrière** — pas de plan de retour, pas de
   sauvegarde vérifiée avant une migration destructive.

Ces trois-là ne se compensent pas par la qualité du reste.

---

## 1. Infra — `ops_reviewer:infra`

Modules Terraform, manifestes Kubernetes, charts, pipelines.

| Critère | Ce qu'on regarde |
|---|---|
| Reproductible | Deux exécutions donnent le même état. Rien ne dépend de ce qui était là avant |
| Paramétré, pas copié | Les valeurs qui changent sont des variables, avec des valeurs par défaut sûres |
| Le plan est lisible | `terraform plan` montre ce qui va se passer, sans ressource surprise |
| Destruction testée | Ce qui est créé peut être détruit sans laisser d'orphelins |
| Documentation d'usage | Quelqu'un d'autre l'utilise sans lire le code |
| Versions épinglées | Providers, images, charts. Une version flottante est une panne différée |

**Refus spécifique** : un `apply` qui ne peut être joué qu'une fois.

## 2. Fiabilité — `ops_reviewer:reliability`

Objectifs de service, runbooks, post-mortems, tests de résilience.

| Critère | Ce qu'on regarde |
|---|---|
| L'objectif est mesurable | Une cible, une fenêtre, une source de mesure nommée |
| L'objectif est atteignable | Une cible que l'architecture rend impossible est un mensonge poli |
| Le budget d'erreur est utilisé | Un budget jamais entamé signale une cible trop basse |
| Le runbook est jouable | Écrit pour quelqu'un qui n'a pas construit le système, à trois heures du matin |
| Le post-mortem porte sur le système | Ce que le système a permis, pas qui a tapé quoi |
| Les actions ont un porteur et une date | Sinon elles n'existent pas |

**Refus spécifique** : un runbook qui commence par « demander à ».

## 3. Cloud — `ops_reviewer:cloud`

Conception, coûts, multi-région.

| Critère | Ce qu'on regarde |
|---|---|
| Le coût est chiffré | Une architecture sans facture estimée est une architecture qu'on découvrira |
| Les compromis sont écrits | Ce qui a été choisi et ce qui a été écarté, avec la raison |
| L'enfermement est nommé | Ce qui serait à réécrire pour changer de fournisseur, et pourquoi c'est accepté |
| La reprise est décrite | RTO, RPO, et le test qui les a vérifiés |
| La région est justifiée | Latence, souveraineté, coût. Pas « c'est la région par défaut » |

**Refus spécifique** : un schéma multi-région dont la base de données est
mono-région sans que ce soit dit.

## 4. Observabilité — `ops_reviewer:observability`

Métriques, journaux, traces, alertes, tableaux de bord.

| Critère | Ce qu'on regarde |
|---|---|
| L'alerte est actionnable | Elle dit quoi faire, ou pointe vers le runbook qui le dit |
| L'alerte réveille pour une raison | Une alerte qui se déclenche sans action possible détruit l'astreinte |
| La cardinalité est maîtrisée | Une étiquette par identifiant utilisateur est une facture, pas une métrique |
| Les traces relient | Une requête se suit d'un bout à l'autre, y compris à travers les files |
| Le tableau de bord répond à une question | Un mur de graphiques n'est pas de l'observabilité |
| La rétention est décidée | Combien de temps, pourquoi, et ce que ça coûte |

**Refus spécifique** : une alerte sur un seuil de ressource sans lien avec un
symptôme utilisateur.

## 5. Données — `ops_reviewer:data`

Réplication, réglage, migrations, reprise.

| Critère | Ce qu'on regarde |
|---|---|
| La migration est réversible | Ou explicitement irréversible, avec la sauvegarde vérifiée avant |
| Le verrou est borné | Une migration qui verrouille une table active en heure de pointe est un incident |
| Le plan de requête est joint | Avant et après, sur des volumes réalistes |
| L'index sert | Un index ajouté sans requête qui l'utilise est un coût d'écriture permanent |
| La restauration a été testée | Une sauvegarde jamais restaurée n'est pas une sauvegarde |
| La réplication a un décalage surveillé | Sinon la bascule se découvre pendant la bascule |

**Refus spécifique** : `ALTER TABLE` sans indication du volume de la table.

---

## Comment se passe une relecture

1. le relecteur appartient à la famille concernée — la capability est
   `ops_reviewer:{famille}` ;
2. il lit contre la grille et écrit ses constats, chacun avec ce sur quoi il
   repose ;
3. quatre tours au maximum. Au-delà, ce n'est plus une relecture mais une
   réécriture, et il faut le dire plutôt que le faire ;
4. un refus dit ce qui manque et ce qui suffirait. « Insuffisant » n'est pas un
   retour.
