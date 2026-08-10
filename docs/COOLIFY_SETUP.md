# Coolify setup runbook

Configuration prod côté Coolify pour l'app `skill-uv-backend`. Couvre le
pre-deploy hook (SKI-48), la config de secrets, et le troubleshooting.

## 1. Pre-deploy verification (SKI-48)

**Objectif** : refuser tout deploy d'une image GHCR non signée par nos GH Actions.

**Chaîne supply chain existante** (déjà en place côté CI) :
- `.github/workflows/image-sign.yml` signe chaque image push via `cosign`
- `.github/workflows/slsa-provenance.yml` produit une attestation SLSA
- `.github/workflows/docker-scan.yml` produit un SBOM CycloneDX

`scripts/verify-image.sh` (déjà dans le repo) vérifie ces trois signaux.

### Coolify config

Dans le panel Coolify → `skill-uv-backend` app → **Pre-Deployment Command** :

```bash
bash <(curl -sSL https://raw.githubusercontent.com/skilluv/skilluv-backend/master/scripts/verify-image.sh) "$COOLIFY_IMAGE_TAG" || exit 1
```

**Pourquoi curl et pas un path local ?** Le script n'est PAS dans l'image Docker (le `Dockerfile` ne copie pas `scripts/`). Il doit être disponible sur le host Coolify. Deux options :

1. **Fetch inline** (recommandé, pattern ci-dessus) — toujours à jour avec master, zero maintenance
2. **Installer sur le host** : `sudo curl -o /opt/skilluv/verify-image.sh https://raw.githubusercontent.com/skilluv/skilluv-backend/master/scripts/verify-image.sh && chmod +x /opt/skilluv/verify-image.sh` puis pre-deploy `bash /opt/skilluv/verify-image.sh "$COOLIFY_IMAGE_TAG"`.

**Prérequis host Coolify** :
- `cosign` installé (`curl -sSL https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64 -o /usr/local/bin/cosign && chmod +x /usr/local/bin/cosign`)
- `jq` (pour parser SBOM output)

### Comportement attendu

| Scénario | Comportement pre-deploy |
|---|---|
| Image poussée par notre CI (signée cosign + SBOM + SLSA) | Exit 0 → deploy continue ✅ |
| Image signée mais SBOM/SLSA absents | Warn dans log, exit 0 → deploy continue (les checks optionnels sont warn-only par design) |
| Image sans signature cosign | Exit 1 → deploy refusé ❌ |
| Image tag inexistant sur GHCR | `cosign verify` retourne erreur → exit 1 ❌ |

### Test de non-régression manuel

```bash
# Test 1 : image signée légitime (doit passer)
COOLIFY_IMAGE_TAG="ghcr.io/skilluv/skilluv-backend:master" bash scripts/verify-image.sh
# → exit 0

# Test 2 : image inconnue (doit refuser)
COOLIFY_IMAGE_TAG="ghcr.io/skilluv/skilluv-backend:definitely-not-signed" bash scripts/verify-image.sh
# → exit 1
```

## 2. Env vars requises côté Coolify

Sensitive — passe par le panel "Environment Variables" avec toggle "Is Secret".

### Coeur backend

| Variable | Valeur type | Note |
|---|---|---|
| `DATABASE_URL` | `postgres://user:pass@host/db` | connection string prod |
| `REDIS_URL` | `redis://host:6379` | |
| `JWT_SECRET` | 32+ chars random | `openssl rand -hex 32` |
| `BASE_URL` | `https://api.skill-uv.com` | API origin |
| `FRONTEND_URL` | `https://skill-uv.com` | pour les liens dans mails (SKI-67) |
| `ENVIRONMENT` | `prod` | active les hard-fails `assert_production_secrets` |

### Storage

| Variable | Note |
|---|---|
| `MINIO_ENDPOINT` | R2/S3-compatible endpoint |
| `MINIO_ACCESS_KEY` / `MINIO_SECRET_KEY` | credentials storage |
| `MINIO_BUCKET` (public) + `MINIO_BUCKET_PRIVATE` (KYC etc.) | |

### Payment (Stripe + Momo)

| Variable | Note |
|---|---|
| `STRIPE_SECRET_KEY` | prod key |
| `STRIPE_WEBHOOK_SECRET` | pour verify signatures |
| `MOMO_API_USER` / `MOMO_API_KEY` | prod credentials |

### Observability (SKI-31)

| Variable | Note |
|---|---|
| `SENTRY_DSN` | GlitchTip ou Sentry.io — voir `docs/OBSERVABILITY.md` |
| `SENTRY_TRACES_SAMPLE_RATE` | `0.1` par défaut |
| `METRICS_TOKEN` | bearer token qui protège `/metrics` (à passer à Prometheus scraper) |

### P26 v2 (workflow challenge)

| Variable | Note |
|---|---|
| `SKILLUV_BOT_GITHUB_TOKEN` | bot GitHub pour ingest, poll CI, refresh, PR check |
| `LINEAR_WEBHOOK_SECRET` + `LINEAR_API_KEY` + `LINEAR_DONE_STATE_ID` | tracker sync |
| `GITHUB_WEBHOOK_SECRET` | inbound webhook receiver |
| `PDF_RENDERER_URL` | pour SKI-118 PDF attestation |

### Deploy trigger

| Variable | Note |
|---|---|
| `COOLIFY_WEBHOOK_URL` (GitHub secret) | URL webhook à hitter depuis `.github/workflows/ci.yml` |
| `COOLIFY_TOKEN` (GitHub secret) | bearer pour le webhook |

## 3. Troubleshooting

### Pre-deploy verify-image échoue en boucle

**Cause probable** : nouveau workflow CI qui touche image-sign a échoué → l'image existe sur GHCR mais sans signature.

**Fix immédiat** :
1. Rollback vers l'image précédente signée : dans Coolify panel → **Rollback** → sélectionner la dernière version verte.
2. Investiguer côté CI : `gh run list --workflow=image-sign.yml` — voir pourquoi le sign a échoué.
3. Une fois CI green, relancer un deploy.

### Deploy en boucle sans succès

Vérifier :
- `docker logs` de l'image sur le host Coolify — probablement env var manquant qui fait paniquer `assert_production_secrets`
- Ping `/api/health` post-deploy — si down, le smoke test CI (SKI-28) va le catcher

### Rollback rapide

Depuis le panel Coolify, chaque deploy laisse une "version précédente" cliquable en 1-click rollback. Priorité : rollback puis investiguer, jamais l'inverse.
