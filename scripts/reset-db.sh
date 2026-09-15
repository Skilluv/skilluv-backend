#!/usr/bin/env bash
#
# Throw a database away and build it back: drop, migrate, seed.
#
# Written for staging, works anywhere that is not production. A round of manual
# testing leaves data in a shape nobody can reason about, and the way back is a
# sequence with several steps that are easy to forget.
#
# ## What it resets, and why that is more than Postgres
#
# Dropping the database alone does not give a clean slate, because three other
# things remember:
#
#   * Redis holds sessions, OAuth state, rate-limit counters and the cached
#     open-slices pool (one hour). Keep it and you are testing a fresh database
#     through a stale cache, signed in as a user who no longer exists.
#   * The mail catcher - Mailpit locally, MailHog on staging - holds every
#     verification and reset mail from the last run, so the next "did the mail
#     arrive?" answers itself wrongly.
#   * MinIO holds uploaded objects. Those become orphans once the rows that
#     referenced them are gone, which is inert rather than harmful, so it is
#     opt-in with `--storage`.
#
# ## How the schema comes back
#
# The backend applies the migrations and then the seed catalogue on every boot.
# So where the backend runs as a container - staging, and a local stack that is
# fully composed - this stops it, drops the database, and starts it again. That
# is the whole migration step, and it is the same path a real deployment takes.
#
# Where there is no backend container but there is a toolchain - a development
# checkout - it falls back to `cargo sqlx migrate run` and `skilluv-seed-all`.
#
# ## Usage
#
#   scripts/reset-db.sh --env-file .env.staging     # staging
#   scripts/reset-db.sh                             # whatever .env points at
#   scripts/reset-db.sh --env-file .env.staging --storage
#   scripts/reset-db.sh --db-only                   # Postgres alone
#   scripts/reset-db.sh --yes                       # no typed confirmation
#
# ## The one thing it refuses
#
# `ENVIRONMENT=production`. Staging is disposable and that is what staging is
# for; production is not, and no flag here turns that off.

set -euo pipefail

ENV_FILE=".env"
WIPE_STORAGE=0
DB_ONLY=0
ASSUME_YES=0

while [ $# -gt 0 ]; do
    case "$1" in
        --env-file) ENV_FILE="${2:?--env-file needs a path}"; shift 2 ;;
        --env-file=*) ENV_FILE="${1#*=}"; shift ;;
        --storage)  WIPE_STORAGE=1; shift ;;
        --db-only)  DB_ONLY=1; shift ;;
        --yes|-y)   ASSUME_YES=1; shift ;;
        --help|-h)  sed -n '2,46p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1 (try --help)" >&2; exit 2 ;;
    esac
done

# ─── Environment ──────────────────────────────────────────────────────
# Read only well-formed KEY=VALUE lines: sourcing the file outright would
# execute anything in it, and these files hold secrets rather than code.
if [ -f "$ENV_FILE" ]; then
    while IFS= read -r line; do
        case "$line" in
            ''|'#'*) continue ;;
            *=*)
                key="${line%%=*}"
                case "$key" in *[!A-Za-z0-9_]*) continue ;; esac
                if [ -z "${!key:-}" ]; then
                    value="${line#*=}"
                    value="${value%\"}"; value="${value#\"}"
                    value="${value%\'}"; value="${value#\'}"
                    export "$key=$value"
                fi
                ;;
        esac
    done < "$ENV_FILE"
else
    echo "No ${ENV_FILE}; using the environment as it stands." >&2
fi

: "${DATABASE_URL:?DATABASE_URL is not set - give me an --env-file that has it}"
ENVIRONMENT="${ENVIRONMENT:-dev}"
MAIL_UI_URL="${MAIL_UI_URL:-http://localhost:8025}"
MINIO_BUCKET="${MINIO_BUCKET:-avatars}"
MINIO_BUCKET_PRIVATE="${MINIO_BUCKET_PRIVATE:-documents}"

PG_CONTAINER="${PG_CONTAINER:-skilluv-postgres}"
REDIS_CONTAINER="${REDIS_CONTAINER:-skilluv-redis}"
MINIO_CONTAINER="${MINIO_CONTAINER:-skilluv-minio}"
API_CONTAINER="${API_CONTAINER:-skilluv-backend}"

# ─── Pull the pieces out of the URL ───────────────────────────────────
# postgres://user:pass@host:port/dbname[?params]
url_body="${DATABASE_URL#*://}"
url_creds="${url_body%%@*}"
url_rest="${url_body#*@}"
DB_USER="${url_creds%%:*}"
DB_HOSTPORT="${url_rest%%/*}"
DB_NAME="${url_rest#*/}"; DB_NAME="${DB_NAME%%\?*}"

if [ -z "$DB_NAME" ] || [ -z "$DB_HOSTPORT" ]; then
    echo "Could not read a host and database name out of DATABASE_URL." >&2
    exit 1
fi

# ─── The one refusal ──────────────────────────────────────────────────
case "$(printf '%s' "$ENVIRONMENT" | tr '[:upper:]' '[:lower:]')" in
    prod|production)
        echo "Refusing: ENVIRONMENT is '${ENVIRONMENT}'." >&2
        echo "This drops a database. Staging is disposable; production is not." >&2
        exit 1
        ;;
esac

# ─── How to reach Postgres ────────────────────────────────────────────
have_container() {
    command -v docker >/dev/null 2>&1 &&
        docker ps --format '{{.Names}}' 2>/dev/null | grep -qx "$1"
}

if have_container "$PG_CONTAINER"; then
    psql_admin() { docker exec -i "$PG_CONTAINER" psql -U "$DB_USER" -d postgres -v ON_ERROR_STOP=1 "$@"; }
    PG_VIA="docker exec ${PG_CONTAINER}"
elif command -v psql >/dev/null 2>&1; then
    psql_admin() { psql "postgres://${url_creds}@${DB_HOSTPORT}/postgres" -v ON_ERROR_STOP=1 "$@"; }
    PG_VIA="psql against ${DB_HOSTPORT}"
else
    echo "No way to reach Postgres: '${PG_CONTAINER}' is not running and there" >&2
    echo "is no local psql. Run this on the host that has the stack." >&2
    exit 1
fi

# ─── Say what is about to go ──────────────────────────────────────────
echo
echo "  database    ${DB_NAME} on ${DB_HOSTPORT}"
echo "  reached by  ${PG_VIA}"
echo "  environment ${ENVIRONMENT}   (from ${ENV_FILE})"
if [ "$DB_ONLY" -eq 1 ]; then
    echo "  also        nothing - --db-only"
else
    echo "  also        redis flush, mail catcher emptied"
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

# ─── 1. Stop the backend, if it is a container ────────────────────────
# Its pool would keep reconnecting through the DROP, and on the way back up it
# is what applies the migrations and the seeds.
API_WAS_RUNNING=0
if have_container "$API_CONTAINER"; then
    API_WAS_RUNNING=1
    echo "▶ stopping ${API_CONTAINER}"
    docker stop "$API_CONTAINER" >/dev/null
fi

# ─── 2. Drop and recreate ─────────────────────────────────────────────
echo "▶ dropping ${DB_NAME}"
psql_admin -q -c "SELECT pg_terminate_backend(pid)
                    FROM pg_stat_activity
                   WHERE datname = '${DB_NAME}' AND pid <> pg_backend_pid()" >/dev/null
psql_admin -q -c "DROP DATABASE IF EXISTS \"${DB_NAME}\""
psql_admin -q -c "CREATE DATABASE \"${DB_NAME}\" OWNER \"${DB_USER}\""
echo "  recreated, empty"

# ─── 3. Migrations and seeds ──────────────────────────────────────────
echo "▶ migrations and seeds"
if [ "$API_WAS_RUNNING" -eq 1 ]; then
    # The backend migrates and then seeds on boot. Same path a deployment
    # takes, so nothing here can drift from what really happens.
    docker start "$API_CONTAINER" >/dev/null
    echo "  ${API_CONTAINER} restarting; it migrates then seeds"

    # Wait for the seed ledger to exist and have rows: that is the signal the
    # boot sequence got all the way through, rather than the container merely
    # being up.
    for _ in $(seq 1 60); do
        if psql_admin -tAc "SELECT count(*) FROM \"${DB_NAME}\".public.seed_runs" \
             >/dev/null 2>&1; then
            count="$(docker exec -i "$PG_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -tAc \
                     'SELECT count(*) FROM seed_runs' 2>/dev/null || echo 0)"
            if [ "${count:-0}" -gt 0 ]; then
                echo "  done: ${count} seed steps recorded"
                break
            fi
        fi
        sleep 2
    done
    if [ "${count:-0}" -eq 0 ]; then
        echo "  The backend has not finished seeding. Watch it with:" >&2
        echo "      docker logs -f ${API_CONTAINER}" >&2
        echo "  A missing SEED_ADMIN_PASSWORD is the usual reason there is no" >&2
        echo "  administrator; it does not stop the other steps." >&2
    fi
elif command -v cargo >/dev/null 2>&1; then
    if ! cargo sqlx --version >/dev/null 2>&1; then
        echo "  sqlx-cli is missing. Install it with:" >&2
        echo "      cargo install sqlx-cli --no-default-features --features rustls,postgres" >&2
        exit 1
    fi
    DATABASE_URL="$DATABASE_URL" cargo sqlx migrate run --source migrations
    if [ -z "${SEED_ADMIN_PASSWORD:-}" ]; then
        echo "  SEED_ADMIN_PASSWORD is not set, so no administrator is created." >&2
        echo "  Set it (12+ characters; the .env.example placeholder is 9) and" >&2
        echo "  run: cargo run --bin skilluv-seed-all -- --forget admin_account" >&2
    fi
    DATABASE_URL="$DATABASE_URL" cargo run --quiet --bin skilluv-seed-all
else
    echo "  No ${API_CONTAINER} container and no cargo: nothing applied the" >&2
    echo "  migrations. The database is empty. Start the backend and it will." >&2
fi

if [ "$DB_ONLY" -eq 1 ]; then
    echo
    echo "Done. Postgres only - redis and the mail catcher still hold the last run."
    exit 0
fi

# ─── 4. Redis ─────────────────────────────────────────────────────────
echo "▶ redis"
if have_container "$REDIS_CONTAINER"; then
    docker exec "$REDIS_CONTAINER" redis-cli FLUSHALL >/dev/null && echo "  flushed"
elif command -v redis-cli >/dev/null 2>&1; then
    redis-cli -u "${REDIS_URL:-redis://localhost:6379}" FLUSHALL >/dev/null && echo "  flushed"
else
    echo "  skipped: no '${REDIS_CONTAINER}' and no redis-cli."
    echo "  Sessions and cached feeds from the last run are still there."
fi

# ─── 5. The mail catcher ──────────────────────────────────────────────
# Mailpit and MailHog both answer DELETE /api/v1/messages on 8025.
echo "▶ mail"
if command -v curl >/dev/null 2>&1 \
   && curl -fsS -X DELETE "${MAIL_UI_URL}/api/v1/messages" >/dev/null 2>&1; then
    echo "  inbox emptied"
else
    echo "  skipped: ${MAIL_UI_URL} did not answer (set MAIL_UI_URL if it is elsewhere)."
fi

# ─── 6. MinIO, on request ─────────────────────────────────────────────
if [ "$WIPE_STORAGE" -eq 1 ]; then
    echo "▶ minio"
    if have_container "$MINIO_CONTAINER"; then
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
echo "Done. ${DB_NAME} is what a new deployment would have."
