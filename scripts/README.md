# scripts/ — outils, pas des seeds

Les seeds de contenu **ne vivent plus ici**. Ils sont dans
`src/services/seed/`, appliqués automatiquement par le serveur après ses
migrations, à chaque démarrage.

## Pourquoi le déplacement

Ce dossier contenait sept scripts SQL à lancer à la main, dans un ordre écrit
nulle part, et rien n'enregistrait s'ils l'avaient été. Quatre d'entre eux ne
pouvaient de toute façon pas fonctionner : ils résolvaient leur propriétaire
avec `WHERE email = 'admin@skilluv.local'` alors que `seed_admin` crée
`admin@skill-uv.com`. Le CTE était vide, l'`INSERT ... SELECT` n'insérait rien,
et `psql` sortait 0. Deux autres portaient un UUID de propriétaire en dur, celui
d'une seule machine de développement.

Un seed qui réussit sans rien faire est pire qu'un seed que personne n'a lancé :
le second se remarque.

## Ce qui se passe maintenant

Au démarrage, après les migrations, le serveur applique
`services::seed::run`. Il lit la table `seed_runs`, saute chaque étape dont la
version y figure déjà, et applique le reste. Sur une base à jour, c'est un seul
`SELECT`.

Donc : **première mise en production, base supprimée et recréée, restauration
d'un dump** — le catalogue se remet en place tout seul.

Les dix étapes, dans l'ordre où les données dépendent les unes des autres :

| Étape | Contenu |
|---|---|
| `admin_account` | le compte administrateur que tout le reste possède |
| `oss_partners` | les douze dépôts partenaires de l'Annexe F |
| `projects` | nos dépôts, les partenaires, l'écosystème (≈ 50) |
| `oss_partners_ingestion` | quels labels amont deviennent des tranches |
| `flagships` | les deux projets que Skilluv porte lui-même |
| `onboarding_challenges` | un premier défi par starter |
| `badge_rule_bonjour_skilluv` | le badge de la première PR mergée |
| `season1_deliverables` | la saison 1 et ses dix livrables |
| `season2_deliverables` | la saison 2 et ses livrables |
| `design_canvas` | le travail design sur nos propres surfaces |

## Configuration

Une seule variable est obligatoire, la première fois :

```
SEED_ADMIN_PASSWORD=...   # 12 caractères minimum
SEED_ADMIN_EMAIL=...      # défaut : admin@skill-uv.com
```

Sans elle, le serveur démarre quand même, saute les étapes qui ont besoin d'un
propriétaire, et le dit très fort dans les logs. On pose la variable, on
redémarre, et le catalogue se rattrape — rien de déjà appliqué n'est refait.

`SKILLUV_SEED_ON_BOOT=0` désactive le seed au démarrage : pour une réplique qui
ne doit pas courir contre la primaire, ou pour une restauration qu'on veut
inspecter avant de lui faire confiance.

## À la main

```bash
cargo run --bin skilluv-seed-all                     # applique ce qui manque
cargo run --bin skilluv-seed-all -- --list           # les noms d'étapes
cargo run --bin skilluv-seed-all -- --forget projects  # rejouer une étape
cargo run --bin skilluv-seed-all -- --forget-all     # rejouer tout
```

Modifier un fichier de seed suffit à le rejouer : la version d'une étape SQL est
le SHA-256 de son contenu, donc le prochain déploiement la réapplique. Les
étapes écrites en Rust portent une version que l'auteur incrémente.

## Ce qui reste opt-in, et le restera

| Binaire | Pourquoi il n'est pas dans le catalogue |
|---|---|
| `skilluv-seed` | invente des utilisateurs et des soumissions. Un démarrage en production qui le lancerait mettrait des gens imaginaires au classement. |
| `skilluv-seed-guild` | fixture end-to-end, refuse déjà de tourner ailleurs qu'en local. |

## Le reste de ce dossier

Des outils, pas du contenu : vérification des migrations, capabilités nommées
dans le code, smoke tests, tests de paiement. Ils se lancent à la main et rien
ne les appelle au démarrage.
