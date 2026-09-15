#!/usr/bin/env bash
#
# Throw the local database away and build it back: drop, migrate, seed.
#
# For when a round of manual testing has left the data in a shape you can no
# longer reason about, and you want to start from what a brand new deployment
# would have.
#
# ## What it resets, and why that is more than Postgres
#
# Dropping the database alone does not give a clean slate, because three other
# things remember:
#
#   * Redis holds sessions, OAuth state, rate-limit counters and the cached
#     open-slices pool (one hour). Keep it and you are testing a fresh database
#     through a stale cache, signed in as a user who no longer exists.
#   * Mailpit holds every verification and reset mail from the last run, so the
#     next "did the mail arrive?" answers itself wrongly.
#   * MinIO holds uploaded objects. Those become orphans - nothing in the new
#     database points at them - which is inert rather than harmful, so it is
#     opt-in with `--storage`.
#
# ## Usage
#
#   scripts/reset-local-db.sh                 # postgres + redis + mailpit
#   scripts/reset-local-db.sh --storage       # ... and empty the MinIO buckets
#   scripts/reset-local-db.sh --db-only       # postgres alone
#   scripts/reset-local-db.sh --yes           # skip the typed confirmation
#
# ## Refusing to run
#
# It stops unless the database is on this machine and `ENVIRONMENT` is not a
# production one. Both checks are on purpose: this is a `DROP DATABASE` with a
# convenient name, and the day somebody runs it with a production
# `DATABASE_URL` exported is the day it has to say no rather than work.

set -euo pipefail

# ─── Arguments ────────────────────────────────────────────────────────
WIPE_STORAGE=0
DB_ONLY=0
ASSUME_YES=0

for arg in "$@"; do
    case "$arg" in
        --storage)  WIPE_STORAGE=1 ;;
        --db-only)  DB_ONLY=1 ;;
        --yes|-y)   ASSUME_YES=1 ;;
        --help|-h)  sed -n '2,36p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $arg (try --help)" >&2; exit 2 ;;
    esac
done

# ─── Environment ──────────────────────────────────────────────────────
# `.env` is the source of truth for a local run. Read only well-formed
# KEY=VALUE lines: sourcing the file outright would execute anything in it.
if [ -f .env ]; then
    while IFS= read -r line; do
        case "$line" in
            ''|'#'*) continue ;;
            *=*)
                key="${line%%=*}"
                case "$key" in
                    *[!A-Za-z0-9_]*) continue ;;
                esac
                # Only fill what the caller has not already exported.
                if [ -z "${!key:-}" ]; then
                    value="${line#*=}"
                    value="${value%\"}"; value="${value#\"}"
                    value="${value%\'}"; value="${value#\'}"
                    export "$key=$value"
                fi
                ;;
        esac
    done < .env
fi

DATABASE_URL="${DATABASE_URL:-postgres://skilluv:skilluv_secret@localhost:5433/skilluv}"
REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
ENVIRONMENT="${ENVIRONMENT:-dev}"
MAILPIT_URL="${MAILPIT_URL:-http://localhost:8025}"
MINIO_BUCKET="${MINIO_BUCKET:-avatars}"
MINIO_BUCKET_PRIVATE="${MINIO_BUCKET_PRIVATE:-documents}"

PG_CONTAINER="${PG_CONTAINER:-skilluv-postgres}"
REDIS_CONTAINER="${REDIS_CONTAINER:-skilluv-redis}"
MINIO_CONTAINER="${MINIO_CONTAINER:-skilluv-minio}"

# ─── Pull the pieces out of the URL ───────────────────────────────────
# postgres://user:pass@host:port/dbname[?params]
url_body="${DATABASE_URL#*://}"
url_creds="${url_body%%@*}"
url_rest="${url_body#*@}"
DB_USER="${url_creds%%:*}"
DB_HOSTPORT="${url_rest%%/*}"
DB_HOST="${DB_HOSTPORT%%:*}"
DB_NAME="${url_rest#*/}"
DB_NAME="${DB_NAME%%\?*}"

if [ -z "$DB_NAME" ] || [ -z "$DB_HOST" ]; then
    echo "Could not read a host and database name out of DATABASE_URL." >&2
    exit 1
fi

# ─── Refusing to run ──────────────────────────────────────────────────
case "$DB_HOST" in
    localhost|127.0.0.1|::1|0.0.0.0) ;;
    *)
        echo "Refusing: DATABASE_URL points at '${DB_HOST}', which is not this machine." >&2
        echo "This script drops a database. It only does that locally." >&2
        exit 1
        ;;
esac

case "$(printf '%s' "$ENVIRONMENT" | tr '[:upper:]' '[:lower:]')" in
    prod|production|staging)
        echo "Refusing: ENVIRONMENT is '${ENVIRONMENT}'." >&2
        exit 1
        ;;
esac

# ─── How to reach Postgres ────────────────────────────────────────────
# A local `psql` if there is one, the container otherwise. Windows dev boxes
# usually have the containers and no client.
if command -v psql >/dev/null 2>&1; then
    psql_admin() { psql "postgres://${url_creds}@${DB_HOSTPORT}/postgres" -v ON_ERROR_STOP=1 "$@"; }
    PG_VIA="local psql"
elif command -v docker >/dev/null 2>&1 && docker ps --format '{{.Names}}' 2>/dev/null | grep -qx "$PG_CONTAINER"; then
    psql_admin() { docker exec -i "$PG_CONTAINER" psql -U "$DB_USER" -d postgres -v ON_ERROR_STOP=1 "$@"; }
    PG_VIA="docker exec ${PG_CONTAINER}"
else
    echo "No way to reach Postgres: no local 'psql', and the container" >&2
    echo "'${PG_CONTAINER}' is not running. Start it with:" >&2
    echo "    docker compose up -d postgres redis minio mailpit" >&2
    exit 1
fi

# ─── Say what is about to go ──────────────────────────────────────────
echo
echo "  database    ${DB_NAME} on ${DB_HOSTPORT}   (via ${PG_VIA})"
echo "  environment ${ENVIRONMENT}"
if [ "$DB_ONLY" -eq 1 ]; then
    echo "  also        nothing - --db-only"
else
    echo "  also        redis FLUSHALL, mailpit messages"
    [ "$WIPE_STORAGE" -eq 1 ] && echo "              minio buckets ${MINIO_BUCKET}, ${MINIO_BUCKET_PRIVATE}"
fi
echo
echo "  Everything in it is destroyed. There is no undo."
echo

if [ "$ASSUME_YES" -eq 0 ]; then
    printf "  Type the database name (%s) to go ahead: " "$DB_NAME"
    read -r answer
    if [ "$answer" != "$DB_NAME" ]; then
        echo "  Nothing was touched."
        exit 1
    fi
    echo
fi

# ─── 1. Drop and recreate ─────────────────────────────────────────────
# Open connections would make DROP fail, and on a dev box they are the app or
# a psql left open in another window rather than anything precious.
echo "▶ dropping ${DB_NAME}"
psql_admin -q -c "SELECT pg_terminate_backend(pid)
                    FROM pg_stat_activity
                   WHERE datname = '${DB_NAME}' AND pid <> pg_backend_pid()" >/dev/null
psql_admin -q -c "DROP DATABASE IF EXISTS \"${DB_NAME}\""
psql_admin -q -c "CREATE DATABASE \"${DB_NAME}\" OWNER \"${DB_USER}\""
echo "  recreated, empty"

# ─── 2. Migrations ────────────────────────────────────────────────────
echo "▶ migrations"
if ! command -v cargo >/dev/null 2>&1; then
    echo "  cargo is not on PATH." >&2
    exit 1
fi
if ! cargo sqlx --version >/dev/null 2>&1; then
    echo "  sqlx-cli is missing. Install it with:" >&2
    echo "      cargo install sqlx-cli --no-default-features --features rustls,postgres" >&2
    exit 1
fi
DATABASE_URL="$DATABASE_URL" cargo sqlx migrate run --source migrations

# ─── 3. Seeds ─────────────────────────────────────────────────────────
# The server applies these itself on every boot; running them here means the
# database is usable before anything starts, and a seed that fails says so now
# rather than in a log nobody is reading.
#
# The admin step is the one that needs a secret, and it refuses rather than
# inventing one. It is not in `.env` by default, so say so plainly.
echo "▶ seeds"
if [ -z "${SEED_ADMIN_PASSWORD:-}" ]; then
    echo "  SEED_ADMIN_PASSWORD is not set, so no administrator will be created." >&2
    echo "  Every other seed still runs. To get an admin account, set it (12+" >&2
    echo "  characters) and run:  cargo run --bin skilluv-seed-all" >&2
    echo
fi
cargo run --quiet --bin skilluv-seed-all

if [ "$DB_ONLY" -eq 1 ]; then
    echo
    echo "Done. Postgres only - redis and mailpit still hold the last run."
    exit 0
fi

# ─── 4. Redis ─────────────────────────────────────────────────────────
# Sessions, OAuth state, rate limits, and the one-hour open-slices cache. A
# fresh database read through yesterday's cache is the confusing half-state
# this whole script exists to avoid.
echo "▶ redis"
if command -v redis-cli >/dev/null 2>&1; then
    redis-cli -u "$REDIS_URL" FLUSHALL >/dev/null && echo "  flushed"
elif command -v docker >/dev/null 2>&1 && docker ps --format '{{.Names}}' | grep -qx "$REDIS_CONTAINER"; then
    docker exec "$REDIS_CONTAINER" redis-cli FLUSHALL >/dev/null && echo "  flushed"
else
    echo "  skipped: no redis-cli and '${REDIS_CONTAINER}' is not running."
    echo "  Sessions and cached feeds from the last run are still there."
fi

# ─── 5. Mailpit ───────────────────────────────────────────────────────
echo "▶ mailpit"
if command -v curl >/dev/null 2>&1 \
   && curl -fsS -X DELETE "${MAILPIT_URL}/api/v1/messages" >/dev/null 2>&1; then
    echo "  inbox emptied"
else
    echo "  skipped: ${MAILPIT_URL} did not answer."
fi

# ─── 6. MinIO, on request ─────────────────────────────────────────────
# Orphaned objects are inert once the rows that referenced them are gone, so
# this is opt-in. `mc` ships inside the MinIO image; if a future tag drops it,
# say so rather than failing the whole reset at the last step.
if [ "$WIPE_STORAGE" -eq 1 ]; then
    echo "▶ minio"
    if command -v docker >/dev/null 2>&1 && docker ps --format '{{.Names}}' | grep -qx "$MINIO_CONTAINER"; then
        if docker exec "$MINIO_CONTAINER" sh -c 'command -v mc' >/dev/null 2>&1; then
            docker exec "$MINIO_CONTAINER" sh -c "
                mc alias set local http://127.0.0.1:9000 \"\$MINIO_ROOT_USER\" \"\$MINIO_ROOT_PASSWORD\" >/dev/null &&
                for b in '${MINIO_BUCKET}' '${MINIO_BUCKET_PRIVATE}'; do
                    mc rb --force \"local/\$b\" >/dev/null 2>&1 || true
                    mc mb \"local/\$b\" >/dev/null 2>&1 || true
                done
            " && echo "  buckets emptied: ${MINIO_BUCKET}, ${MINIO_BUCKET_PRIVATE}"
        else
            echo "  skipped: no 'mc' inside ${MINIO_CONTAINER}."
            echo "  Old objects stay, orphaned and unreferenced - harmless."
        fi
    else
        echo "  skipped: '${MINIO_CONTAINER}' is not running."
    fi
fi

echo
echo "Done. The database is what a new deployment would have."
echo "Start the backend and the first request sees a clean platform."
