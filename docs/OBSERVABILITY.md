# Observability — Sentry + Grafana / Metabase (SKI-31)

Comment surveiller le backend Skilluv en prod : erreurs (Sentry),
metrics timeseries (Grafana), business queries (Metabase).

## 1. Sentry (errors + traces)

**Already integrated** dans `src/observability.rs` — désactivé quand
`SENTRY_DSN` est vide.

### Setup gratuit — GlitchTip via Coolify

GlitchTip est une alternative Sentry-compatible open-source, self-hostable,
runs sur Coolify sans effort.

1. Coolify → Nouvelle app "Docker" → image `glitchtip/glitchtip:latest`
2. Env vars : `SECRET_KEY=<32chars>`, `DATABASE_URL=postgres://...` (peut réutiliser une DB Postgres side-car)
3. Créer un projet GlitchTip via l'UI, copier le DSN.
4. Coller dans le backend Skilluv Coolify : `SENTRY_DSN=<dsn>`.

### Setup Sentry.io (SaaS)

- Free tier 5k events/mois — largement suffisant pour Phase 1.
- Créer un projet Rust sur sentry.io, copier le DSN, coller côté backend.

### Env vars

| Variable | Note |
|---|---|
| `SENTRY_DSN` | DSN GlitchTip/Sentry — si vide, Sentry est disabled (aucun overhead) |
| `SENTRY_TRACES_SAMPLE_RATE` | 0.1 = 10% des requests tracées, ajuster selon volume |
| `ENVIRONMENT` | `prod` / `staging` — apparaît sur chaque event Sentry |
| `RELEASE` | Auto-populé depuis `CARGO_PKG_VERSION` — override via env si besoin |

### Test intentional error

```bash
# Depuis dev local, avec SENTRY_DSN set :
cargo run
# Trigger un panic délibéré via un endpoint de test
curl http://localhost:3001/api/dev/panic  # nécessite SKILLUV_DEV_MODE=true
# → panique capturée dans Sentry UI dans les 30s
```

## 2. Grafana + Prometheus (timeseries metrics)

Voir `ops/grafana/README.md`. **3 dashboards** livrés :
- Workflow challenge (P26 v2 lifecycle)
- Business overview (users/enterprises/reports)
- Ops (AI, fraud, checkouts)

Backend expose `/metrics` (public) et `/api/metrics/summary` (JSON —
gated admin depuis SKI-31).

## 3. Metabase (business queries — alternative à Grafana)

Grafana est timeseries-first ; Metabase est **SQL-first + dashboards
narratifs**. Deux perspectives complémentaires, pas concurrentes. Si tu
veux un tableau "top 10 users par validated challenges ce mois", Metabase
avec une query SQL directe sur `project_slices` est beaucoup plus
naturel que Grafana.

### Deploy on Coolify

```yaml
# ops/metabase/docker-compose.yml
services:
  metabase:
    image: metabase/metabase:v0.51.0
    environment:
      - MB_DB_TYPE=postgres
      - MB_DB_HOST=<coolify-postgres-host>
      - MB_DB_DBNAME=metabase
      - MB_DB_USER=metabase
      - MB_DB_PASS=<password>
    ports:
      - "3006:3000"
```

Metabase = 1 container. First-boot wizard : create admin + connect
Metabase à ta **DB Skilluv en read-only** (créer un user Postgres avec
`GRANT SELECT ON ALL TABLES` uniquement).

### 5 queries de base (à mettre dans Metabase)

```sql
-- Q1 : Signups / week
SELECT DATE_TRUNC('week', created_at) AS week, COUNT(*) AS signups
FROM users
WHERE created_at > NOW() - INTERVAL '90 days'
GROUP BY 1 ORDER BY 1;

-- Q2 : Slices funnel (open → claimed → validated → merged)
SELECT
  COUNT(*) FILTER (WHERE status = 'open')      AS open,
  COUNT(*) FILTER (WHERE status = 'claimed')   AS claimed,
  COUNT(*) FILTER (WHERE status = 'submitted') AS submitted,
  COUNT(*) FILTER (WHERE status = 'validated') AS validated,
  COUNT(*) FILTER (WHERE status = 'merged')    AS merged
FROM project_slices
WHERE created_at > NOW() - INTERVAL '30 days';

-- Q3 : Top 10 users par attestations
SELECT u.username, COUNT(*) AS attestations
FROM project_slices s
JOIN users u ON u.id = s.claimed_by_user_id
WHERE s.attestation_hash IS NOT NULL
GROUP BY u.username
ORDER BY attestations DESC LIMIT 10;

-- Q4 : Enterprises + payouts total (fake demo — replace with real transactions table)
SELECT e.slug, COUNT(DISTINCT em.user_id) AS members
FROM enterprises e
LEFT JOIN enterprise_members em ON em.enterprise_id = e.id AND em.status = 'active'
GROUP BY e.slug;

-- Q5 : Retention D7 (users signed up >= 7 days ago who logged in in past 7 days)
SELECT
  COUNT(DISTINCT u.id) FILTER (WHERE u.created_at < NOW() - INTERVAL '7 days'
                              AND u.last_login_at > NOW() - INTERVAL '7 days')::float
  / NULLIF(COUNT(DISTINCT u.id) FILTER (WHERE u.created_at < NOW() - INTERVAL '7 days'), 0)
  AS retention_d7
FROM users;
```

Sauve chaque query comme "Question" dans Metabase, pin les 5 dans un
dashboard "Skilluv Business".

### Grafana vs Metabase — comment choisir

| Besoin | Outil |
|---|---|
| Voir un compteur monter en live (validations, signups) | Grafana |
| Alerter sur une chute soudaine | Grafana + Prometheus alertmanager |
| Query ad-hoc "combien de users du Cameroun ont validé un challenge en août ?" | Metabase |
| Top N / classements / distributions | Metabase |
| SLOs / latences p95 / erreurs 5xx / cardinality haute | Grafana |
| Rapport partageable avec un investisseur | Metabase (screenshot ou embed) |

**Recommandation Phase 1** : déployer les DEUX. ~200 MB RAM total, config
minimal. Chacun a sa niche.

## Alerts (voir SKI-32)

Cette doc couvre visualisation. Les alertes réactives (CI red, deploy
failed, DB down) sont dans **SKI-32** via Discord webhook depuis GitHub
Actions.
