# Skilluv observability — Grafana + Prometheus (SKI-31)

Stack Coolify-deployable pour visualiser les metrics Prometheus du
backend Skilluv.

## Deploy on Coolify

1. **Nouvelle app "Docker Compose"** dans Coolify → pointer sur ce dossier.
2. Env vars :
   - `GRAFANA_ADMIN_PASSWORD` — password admin initial (change via UI ensuite)
3. **Ports exposés** :
   - `3005` → Grafana UI (`https://grafana.skill-uv.com` via Coolify reverse proxy)
   - `9090` → Prometheus (optionnel exposer publiquement — usually internal-only)
4. **Config Prometheus** : édite `provisioning/prometheus.yml` pour ajouter d'autres cibles ou changer l'URL du backend.

## Login

Admin username : `admin` / password : celui de `GRAFANA_ADMIN_PASSWORD`.

## Dashboards livrés

3 dashboards auto-provisionnés (visibles dans le folder "Skilluv") :

| Dashboard | Purpose | Data sources |
|---|---|---|
| **Workflow challenge (P26 v2)** | Signal du workflow : ingest domains, external refresh transitions, CI advances, merge bonus | `skilluv_ingest_domain_source_total`, `skilluv_external_refresh_*_total`, `skilluv_ci_webhook_advanced_total`, `skilluv_ci_poll_advanced_total`, `skilluv_merge_bonus_awarded_total` |
| **Business overview** | KPIs core : users, challenges, enterprises, reports pending | gauges refreshed every 60s par `start_business_gauges` — `skilluv_users_total`, `skilluv_users_active_24h`, `skilluv_challenges_in_progress`, `skilluv_enterprises_total`, `skilluv_reports_pending`, `skilluv_conversations_active` |
| **Ops** | Latence AI, jobs queue, fraude, checkouts | `skilluv_ai_call_latency_ms`, `skilluv_ai_jobs_enqueued_total`, `skilluv_admin_2fa_resets_total`, `skilluv_fraud_deliverables_revoked_total`, `skilluv_deep_plagiarism_scans_total`, `skilluv_subscriptions_checkout_created_total` |

## /metrics gating (important)

Le backend expose `/metrics` **avec option d'authentification** :
- Si `METRICS_TOKEN` env-var set côté backend, Prometheus scrape doit envoyer `Authorization: Bearer <token>`.
- Sinon `/metrics` est public (OK pour prod interne, exposé si domain apex).

Recommandé en prod : **setter `METRICS_TOKEN`** puis configurer le scrape avec ce token :

```yaml
# provisioning/prometheus.yml
scrape_configs:
  - job_name: skilluv-backend
    metrics_path: /metrics
    static_configs:
      - targets: [api.skill-uv.com:443]
    scheme: https
    authorization:
      type: Bearer
      credentials: <METRICS_TOKEN value>
```

## Add a new dashboard

1. Créer le JSON dans `dashboards/`.
2. Restart Grafana container OR wait 30s (auto-reload via provisioning).
3. Dashboard apparaît dans folder Skilluv.

## Alerting

Prometheus + Grafana peuvent alerter mais on préfère la voie **SKI-32** :
alertes ops depuis GitHub Actions (Discord webhook sur CI red / deploy
failed / smoke test failed). Grafana reste pour la **visualisation
proactive** ; SKI-32 reste pour les **notifications réactives**.

## Metabase alternative

Voir `docs/OBSERVABILITY.md` pour le tradeoff Grafana vs Metabase +
setup Metabase si tu préfères l'angle "business queries SQL" plutôt
que "timeseries Prometheus".
