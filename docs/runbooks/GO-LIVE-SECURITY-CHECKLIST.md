# Go-live security checklist

The mirror (`api.skill-uv.com`, the Coolify box `skilluv-prod-01`) is
deliberately loosened for the current test phase. That is fine while there are
zero users. **Before the first real user, flip every switch below.** These were
found by a live audit on 2026-08-27.

## Infrastructure — already solid (verified live)

- ✅ Only 22/80/443 exposed; Postgres, Redis, MinIO, Grafana, Prometheus all
  closed to the internet.
- ✅ SSH key-only (password auth off — not brute-forceable), OpenSSH 10.0,
  post-quantum KEX, Terrapin-protected. **MACs hardened to ETM-only** (done).
- ✅ TLS 1.3, valid Let's Encrypt cert, HTTP→HTTPS redirect.
- ✅ Security headers present (CSP, HSTS+includeSubDomains, nosniff, X-Frame DENY),
  no `Server`/`X-Powered-By` leak.
- ✅ Config paths (`/.env`, `/.git/config`, `/Cargo.toml`) 404.

## Application env — the go-live switches (set in Coolify UI, then redeploy)

Coolify → skill-uv-backend → Environment Variables. Editing the on-disk `.env`
does not persist (Coolify regenerates it from its DB on deploy), so change these
in the UI.

| Variable | Now (test) | Before real users | Why |
|---|---|---|---|
| `SKILLUV_DISABLE_RATELIMIT` | `1` | **remove it** (or `0`) | **The important one.** With it set, `/api/auth/login` and every rate-limited route have no brute-force / abuse protection. This is the app-level "anyone can force it". |
| `SKILLUV_DEV_MODE` | `true` | `false` | Dev-mode relaxations must be off in a user-facing deploy. |
| `SKILLUV_HIDE_SWAGGER` | *(unset)* | `1` | Otherwise `https://api.skill-uv.com/api/docs/` serves the full Swagger UI — the whole API surface is publicly mappable. (`/api/openapi.json` stays 200 for schemathesis, which is intended.) |
| `ENVIRONMENT` | `staging` | `production` | Reflection is already off (it only enables under `development`), but keep the label honest once it is prod. |

After changing them, redeploy and re-check:
```bash
curl -s -o /dev/null -w '%{http_code}\n' https://api.skill-uv.com/api/docs   # expect 404
# hammer the login a few times from one IP -> expect 429 once rate limiting is back
```

## Infrastructure hardening — do when convenient

- **Netcup provider firewall** shows `Active (0 policies)` — an unused free
  layer in front of the host. Add: allow 22, 80, 443; deny the rest. Belt and
  suspenders in front of Coolify/Docker (which bypasses host ufw).
- **fail2ban** — optional while SSH is key-only, but it bans scanners and quiets
  the auth log. See SERVER-HARDENING.md.
- Run `scripts/server-audit.sh` on the box for the current verdict.

## The pentest window

Zero users now = safe to test aggressively (PENTEST-MIRROR.md). Rate limiting is
off, which actually makes load/pentest easier right now — just remember it is
item #1 to turn back on. When you split staging from prod (~3 months), this box
becomes prod: run this checklist that day.
