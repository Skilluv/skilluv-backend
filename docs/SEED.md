# Seed binaries

Utilities to bootstrap a Skilluv instance with initial data. All seeds are idempotent — safe to re-run.

## `skilluv-seed-admin` — provision or rotate the admin account

Creates a Skilluv admin account, or resets an existing one (password rotation, role force to `admin`, email verified).

### Prerequisites

- `DATABASE_URL` set (standard sqlx connection string)
- Migrations applied

### Password is mandatory

The binary refuses to run without a password. Minimum **12 characters**. No auto-generation — an operator must consciously choose the secret.

```bash
# Via environment variable (recommended for CI / Coolify pre-deploy)
SEED_ADMIN_PASSWORD='S3cure!Pass123' cargo run --bin skilluv-seed-admin

# Via CLI arg
cargo run --bin skilluv-seed-admin -- \
    --email admin@skill-uv.com \
    --password 'S3cure!Pass123'
```

### Environment variables

| Var | Required | Default |
|---|---|---|
| `SEED_ADMIN_PASSWORD` | **YES** (≥12 chars) | — |
| `SEED_ADMIN_EMAIL` | no | `admin@skill-uv.com` |
| `SEED_ADMIN_USERNAME` | no | `admin` |
| `SEED_ADMIN_FIRST_NAME` | no | `Admin` |
| `SEED_ADMIN_LAST_NAME` | no | `Skilluv` |
| `DATABASE_URL` | yes | — |

CLI args always take precedence over env vars.

### What it does

1. UPSERT on `users` by email:
   - **Insert** if email not present: fresh row with `role='admin'`, `email_verified=TRUE`, `terms_accepted_at=NOW()`, `password_changed_at=NOW()`
   - **Update** if email exists: rotate password_hash, force `role='admin'`, force `email_verified=TRUE`, refresh `password_changed_at`, `updated_at`
2. Call `capabilities_engine::recompute_capabilities_for_user` to ensure derived capabilities are correct. For a fresh admin account, this results in the baseline capability set (role-based access via `AdminGate` middleware, plus any rank/activity-derived caps).
3. Print a summary block to stdout (email, username; password NOT echoed since caller provided it).

### Idempotence

Safe to run multiple times. Second run with the same password = no-op behaviorally (password_hash regenerated but same effective credential). Second run with a different password rotates the password in place.

### Login after seed

Once seeded, login at:
- Prod: `https://skill-uv.com` with `admin@skill-uv.com` + the password you provided
- Local: `http://localhost:3001` (or your local frontend URL)

Admin panel accessible after login (role='admin' opens the AdminGate middleware).

### Errors

| Symptom | Cause | Fix |
|---|---|---|
| `SEED_ADMIN_PASSWORD is required` | No password provided | Set the env var or `--password` CLI arg |
| `SEED_ADMIN_PASSWORD too short` | Password < 12 chars | Use a longer password |
| `DATABASE_URL is required` | No DB connection string | Export DATABASE_URL |
| `failed to upsert admin user` | DB error, migration missing, etc. | Check migrations applied, inspect the underlying sqlx error |
