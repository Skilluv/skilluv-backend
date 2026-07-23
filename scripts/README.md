# scripts/ — Bootstrap seeds

Scripts SQL de bootstrap **contenu** (pas des migrations schéma). Ne sont pas
lus par sqlx et n'apparaissent pas dans `_sqlx_migrations`. À runner manuellement
en dev et via l'UI admin en prod.

## Scripts disponibles

| Fichier                          | Effet                                                                  |
|----------------------------------|------------------------------------------------------------------------|
| `seed_oss_partners.sql`          | Insère 12 partenaires OSS curés en Tier 1 (`skilluv_partnership_level=NULL`) |
| `seed_flagships.sql`             | Insère 2 flagships Skilluv (`hello-africa`, `wax-icons`)               |
| `seed_season1_deliverables.sql`  | Crée la Saison 1 "Hello World" + 10 deliverables `challenge_templates` |

Tous les scripts sont **idempotents** (`ON CONFLICT DO NOTHING`) : re-runnables
sans dégât.

## Dev — application locale

Docker Compose lancé, Postgres accessible via `skilluv-postgres` :

```bash
docker cp scripts/seed_oss_partners.sql skilluv-postgres:/tmp/s.sql
docker exec skilluv-postgres psql -U skilluv -d skilluv -v ON_ERROR_STOP=1 -f /tmp/s.sql
```

Répéter pour chaque fichier dans l'ordre :
1. `seed_oss_partners.sql`
2. `seed_flagships.sql`
3. `seed_season1_deliverables.sql`

## Prod — via l'admin panel

**Ne pas exécuter ces scripts directement en prod.** Ils utilisent
`admin@skilluv.local` comme owner/steward — un compte fixture dev.

En prod, passer par l'UI admin :
- `/projects` (skilluv-admin) pour les 14 projets
- `/challenges` (skilluv-admin) pour les 10 deliverables

Les scripts servent de source de vérité pour le contenu à saisir (slug, titre,
description, notes éditoriales), pas comme mécanisme d'insertion.

## Convention

- Un script SQL par lot logique de contenu
- Toujours `ON CONFLICT` sur une contrainte unique existante
- Toujours un `SELECT` de résumé à la fin pour visibilité
- Owner en dev = `admin@skilluv.local` — à ré-owner en prod
