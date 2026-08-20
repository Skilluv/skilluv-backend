#!/usr/bin/env bash
#
# Apply every migration to a throwaway database, and say what broke.
#
# A migration is not checked by `cargo check`, by clippy, or by any unit test.
# It is checked the first time something runs it — and in CI that is the
# integration job, which starts by migrating. So a constraint violation does
# not fail one test: it fails the chain, the backend never starts, and all
# eight shards go red at once, thirty-five minutes later, with a failure that
# looks like "everything is broken" rather than "one INSERT is wrong".
#
# That happened. `domain_curator:all` went in with a NULL scope against a
# CHECK requiring `family || ':' || scope`, and the whole run was lost to it.
# Fifteen seconds here would have caught it.
#
# Usage:  ./scripts/check-migrations.sh
#
# Needs a PostgreSQL 18 reachable with the credentials below — the native
# install is fine, Docker is not required. Override with DATABASE_BASE_URL.

set -euo pipefail

BASE="${DATABASE_BASE_URL:-postgres://skilluv:skilluv_secret@127.0.0.1:5432}"
DB="skilluv_migcheck_$$"

cleanup() {
    psql "${BASE}/postgres" -q -c "DROP DATABASE IF EXISTS \"${DB}\"" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "Creating ${DB}"
psql "${BASE}/postgres" -q -c "CREATE DATABASE \"${DB}\""

echo "Applying migrations"
if ! DATABASE_URL="${BASE}/${DB}" cargo sqlx migrate run --source migrations; then
    echo
    echo "A migration failed. The chain stops at the one named above; nothing"
    echo "after it ran. Fix that one and run this again — CI will not tell you"
    echo "anything this does not, and it takes thirty-five minutes to say it."
    exit 1
fi

echo
echo "Checking what the schema is supposed to hold"

fail=0
check() {
    local label="$1" sql="$2" want="$3"
    local got
    got="$(psql "${BASE}/${DB}" -tAc "${sql}")"
    if [ "${got}" != "${want}" ]; then
        echo "  FAIL ${label}: expected ${want}, got ${got}"
        fail=1
    else
        echo "  ok   ${label}"
    fi
}

# Every domain column points at the catalogue. This is the check that found
# the two Discord routing tables; a domain column with no key does not fail on
# a typo, it routes to nothing and nobody finds out.
check "no unchecked domain column" "
    SELECT count(*) FROM information_schema.columns c
      JOIN information_schema.tables t ON t.table_name = c.table_name
       AND t.table_schema='public' AND t.table_type='BASE TABLE'
     WHERE c.table_schema='public' AND c.data_type='character varying'
       AND (c.column_name IN ('skill_domain','primary_domain','target_domain','desired_domain')
            OR (c.column_name='domain' AND c.table_name<>'tenants'))
       AND NOT EXISTS (
           SELECT 1 FROM pg_constraint con
             JOIN pg_class rel ON rel.oid=con.conrelid
             JOIN pg_attribute att ON att.attrelid=con.conrelid AND att.attnum=ANY(con.conkey)
            WHERE con.contype='f' AND rel.relname=c.table_name
              AND att.attname=c.column_name AND con.confrelid='skill_domains'::regclass)" "0"

# The vocabularies that a CHECK-to-table conversion has lost once per domain.
# Each of these was a foreign key violation in CI before it was a row here.
check "design slice type" \
    "SELECT count(*) FROM slice_types WHERE slug='design_artifact'" "1"
check "domain-agnostic contest formats" \
    "SELECT count(*) FROM tournament_kinds WHERE slug IN ('duel','brief_contest')" "2"
check "contest attestation bases" \
    "SELECT count(*) FROM attestation_bases WHERE basis IN ('contest_finalist','contest_hired')" "2"
check "design attestation bases" \
    "SELECT count(*) FROM attestation_bases WHERE skill_domain='design'" "7"
check "design and ops deliverable formats" \
    "SELECT count(*) FROM mission_deliverable_formats WHERE skill_domain IN ('design','ops')" "9"
check "domain curator capabilities" \
    "SELECT count(*) FROM capability_catalog WHERE family='domain_curator'" "9"

# Every capability's name has to equal its parts, which is the constraint the
# NULL scope violated.
check "capability names match their parts" "
    SELECT count(*) FROM capability_catalog
     WHERE capability <> family || COALESCE(':' || scope, '')" "0"

# A live domain with no skills cannot place anybody on anything.
check "every live domain has skills" "
    SELECT count(*) FROM skill_domains d
     WHERE d.is_active
       AND NOT EXISTS (SELECT 1 FROM skill_nodes n WHERE n.domain = d.slug)" "0"

echo
if [ "${fail}" -ne 0 ]; then
    echo "The schema applied but does not hold what it should."
    exit 1
fi
echo "Migrations apply and the schema holds."
