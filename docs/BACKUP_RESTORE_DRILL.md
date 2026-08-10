# Backup restore drill

SKI-29 (Hygiène pré-prod HYG-02). Cadence : **mensuel** — un backup
qu'on n'a jamais restauré n'est pas un backup.

## Automatique — subcommand `drill-report`

Le binary `skilluv-backup` a une subcommand `drill-report` qui automatise :
1. Crée un backup frais (via `run_backup`)
2. Immédiatement le restore dans une DB éphémère (via `restore_test`)
3. Vérifie les counts sur les tables critiques (users, challenges, submissions)
4. Émet un JSON complet + notif webhook

```bash
# En prod (Coolify runner ou serveur backup)
skilluv-backup drill-report | tee logs/backup-drill-$(date +%F).json
```

Sortie type :

```json
{
  "ran_at": "2026-08-10T09:00:00Z",
  "duration_seconds": 187,
  "backup": {
    "backup_key": "prod/2026-08-10T09-00-00Z.dump.gz",
    "size_bytes": 512847362,
    ...
  },
  "restore_test": {
    "backup_key": "prod/2026-08-10T09-00-00Z.dump.gz",
    "counts": {
      "users": 12,
      "challenges": 45,
      "submissions": 87,
      ...
    },
    "checks": ["schema_ok", "counts_nonzero", "index_bloat_ok"]
  },
  "verdict": "restore_chain_alive"
}
```

## Manuel — étape par étape

Si tu veux valider chaque étape ou reproduire sur un serveur ad-hoc :

### 1. Créer un backup

```bash
skilluv-backup backup
```

Output : clé S3/R2 du nouveau dump + taille + durée.

### 2. Lister les backups pour choisir le key

```bash
skilluv-backup list
```

### 3. Restore-test (auto-cleanup)

```bash
skilluv-backup restore-test
```

Prend le dernier backup, download, restore dans une DB éphémère
(nom aléatoire prefix `restore_test_...`), vérifie les counts, cleanup.

### 4. Restore manuel vers une DB nommée (pour investigation)

```bash
# Créer la DB cible
psql -h localhost -U skilluv -c "CREATE DATABASE skilluv_restore_20260810;"

# Restaurer
skilluv-backup restore --backup-key "prod/2026-08-10T09-00-00Z.dump.gz" \
                      --target-db "skilluv_restore_20260810"

# Vérifier
psql -h localhost -U skilluv -d skilluv_restore_20260810 -c "
  SELECT 'users' as table_name, COUNT(*) FROM users
  UNION SELECT 'challenges', COUNT(*) FROM challenges
  UNION SELECT 'deliverables', COUNT(*) FROM deliverables
  UNION SELECT 'attestations', COUNT(*) FROM attestations
  ORDER BY table_name;
"

# Cleanup quand terminé
psql -h localhost -U skilluv -c "DROP DATABASE skilluv_restore_20260810;"
```

## Métriques à archiver

Après chaque drill, capturer :

| Métrique | Cible |
|---|---|
| Duration end-to-end | < 30 min (RTO cible) |
| Taille dump | Track over time (indique croissance DB) |
| Row count users | Cohérent avec prod monitoring |
| Row count challenges | idem |
| Verdict | Doit rester `restore_chain_alive` |

## Runbook incident : "backup fail"

Si `skilluv-backup backup` échoue en prod :
1. Vérifier `df -h` — disque plein ? (`pg_dump` écrit temporaire local avant upload)
2. Vérifier `s3cmd ls s3://skilluv-backups/prod/` — R2/MinIO reachable ?
3. Vérifier les credentials env `BACKUP_R2_ACCESS_KEY` / `BACKUP_R2_SECRET_KEY`
4. Rollback vers l'ancien backup n-1 si besoin (`skilluv-backup list` → identifier avant-dernier)

Si `restore-test` échoue mais `backup` passe :
- Le dump est corrompu ou incompatible avec le schema courant
- Investigate : `pg_restore --list <dump>` révèle-t-il la structure ?
- Peut indiquer un drift de version Postgres (dump fait avec 15, restore sur 18 → warning + parfois erreur)

## Cron scheduling (optionnel)

Ajouter au crontab du runner ops :

```
# Backup daily at 03:00 UTC
0 3 * * * cd /opt/skilluv && skilluv-backup backup >> logs/backup.log 2>&1

# Drill monthly at 04:00 UTC on the 1st
0 4 1 * * cd /opt/skilluv && skilluv-backup drill-report > logs/drill-$(date +%F).json 2>&1
```

## Historique des drills

| Date | Duration | Verdict | Ran by |
|---|---|---|---|
| 2026-08-10 | *(pending — first run)* | *(pending)* | — |

*Update ce tableau après chaque drill mensuel.*
