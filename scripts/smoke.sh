#!/usr/bin/env bash
# SKI-28 (Hygiène pré-prod HYG-01) — smoke test suite.
#
# Hits the critical endpoints of a running Skilluv backend and exits
# non-zero on the first failure. Used by CI after a Coolify deploy to
# catch silent breakages ("deploy succeeded, app is 500ing").
#
# Usage:
#   ./scripts/smoke.sh https://api.skill-uv.com
#   ./scripts/smoke.sh http://localhost:3001     # local dev
#
# Exit codes:
#   0  — all checks passed
#   1  — at least one endpoint failed
#   2  — misuse (missing base URL)

set -euo pipefail

BASE_URL="${1:-}"
if [[ -z "$BASE_URL" ]]; then
    echo "usage: $0 <base_url>" >&2
    exit 2
fi
BASE_URL="${BASE_URL%/}"  # strip trailing slash

# Retry wrapper — the API can be re-warming after a Coolify swap.
# 5 tries × 3s = 15s total ceiling; enough for cold-start without
# masking a real outage.
retry_curl() {
    local url="$1"
    local expected_status="$2"
    local max_tries=5
    local wait_sec=3
    for ((i=1; i<=max_tries; i++)); do
        local status
        status=$(curl -sS -o /dev/null -w "%{http_code}" \
            --max-time 10 --connect-timeout 5 "$url" || echo "000")
        if [[ "$status" == "$expected_status" ]]; then
            return 0
        fi
        if ((i < max_tries)); then
            sleep "$wait_sec"
        fi
    done
    echo "  FAIL $url → got HTTP $status, expected $expected_status" >&2
    return 1
}

# Check that a JSON key exists in the response.
check_json_key() {
    local url="$1"
    local jq_path="$2"
    local expected="$3"
    local body
    body=$(curl -sS --max-time 10 "$url")
    local actual
    actual=$(echo "$body" | jq -r "$jq_path" 2>/dev/null || echo "")
    if [[ "$actual" != "$expected" ]]; then
        echo "  FAIL $url → $jq_path = '$actual', expected '$expected'" >&2
        return 1
    fi
    return 0
}

echo "═══════════════════════════════════════════════════════════"
echo "  Skilluv backend smoke test — $BASE_URL"
echo "═══════════════════════════════════════════════════════════"

FAILED=0

echo "• /api/health (liveness)"
retry_curl "$BASE_URL/api/health" "200" || FAILED=$((FAILED + 1))
check_json_key "$BASE_URL/api/health" ".status" "live" || FAILED=$((FAILED + 1))

echo "• /api/health/deep (Postgres + Redis + MinIO + WS)"
retry_curl "$BASE_URL/api/health/deep" "200" || FAILED=$((FAILED + 1))

echo "• /api/openapi.json (contract still served)"
retry_curl "$BASE_URL/api/openapi.json" "200" || FAILED=$((FAILED + 1))

echo "• /metrics (observability up)"
# 200 (public) OR 401 (gated by METRICS_TOKEN — still means backend is alive)
metrics_status=$(curl -sS -o /dev/null -w "%{http_code}" \
    --max-time 10 "$BASE_URL/metrics" || echo "000")
if [[ "$metrics_status" != "200" ]] && [[ "$metrics_status" != "401" ]]; then
    echo "  FAIL /metrics → HTTP $metrics_status (expected 200 or 401)" >&2
    FAILED=$((FAILED + 1))
fi

echo "• /api/auth/register with empty body (validation active)"
register_status=$(curl -sS -o /dev/null -w "%{http_code}" \
    -X POST -H "Content-Type: application/json" -d '{}' \
    --max-time 10 "$BASE_URL/api/auth/register" || echo "000")
# 400/422 = validation working (422 = well-formed JSON, missing fields),
# 429 = rate-limited (also fine — proves middleware alive)
if [[ "$register_status" != "400" ]] && [[ "$register_status" != "422" ]] && [[ "$register_status" != "429" ]]; then
    echo "  FAIL POST /api/auth/register → HTTP $register_status (expected 400, 422, or 429)" >&2
    FAILED=$((FAILED + 1))
fi

echo "• /api/geo/countries (public, DB-backed)"
retry_curl "$BASE_URL/api/geo/countries" "200" || FAILED=$((FAILED + 1))

echo "• /api/challenges (public catalog)"
retry_curl "$BASE_URL/api/challenges" "200" || FAILED=$((FAILED + 1))

echo "═══════════════════════════════════════════════════════════"
if [[ "$FAILED" -eq 0 ]]; then
    echo "  OK all checks passed"
    echo "═══════════════════════════════════════════════════════════"
    exit 0
else
    echo "  FAIL $FAILED check(s) failed"
    echo "═══════════════════════════════════════════════════════════"
    exit 1
fi
