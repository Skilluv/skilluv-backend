#!/usr/bin/env bash
# SKI-35 — Mobile Money payout end-to-end test.
#
# Runs a real 1000 XOF payout via Orange/MTN/Wave sandbox.
#
# Prereqs (any provider) :
#   export SKILLUV_BASE_URL="https://staging.skill-uv.com"
#   export SKILLUV_TEST_EMAIL="test@example.com"
#   export SKILLUV_TEST_PASSWORD="..."
#   export SKILLUV_MOMO_PROVIDER="mtn|orange|wave"
#   export SKILLUV_MOMO_MSISDN="+22890000001"  # sandbox number
#
# See docs/PAYMENT_STAGING_SETUP.md for provider-specific setup.

set -euo pipefail

: "${SKILLUV_BASE_URL:?}"
: "${SKILLUV_TEST_EMAIL:?}"
: "${SKILLUV_TEST_PASSWORD:?}"
: "${SKILLUV_MOMO_PROVIDER:?}"
: "${SKILLUV_MOMO_MSISDN:?}"

TEST_RUN_ID="momo-$SKILLUV_MOMO_PROVIDER-$(date +%s)"
COOKIE_JAR=$(mktemp)
trap 'rm -f "$COOKIE_JAR"' EXIT

echo "═══════════════════════════════════════════════════════════"
echo "  Momo payout test — $TEST_RUN_ID"
echo "  Base URL: $SKILLUV_BASE_URL"
echo "  Provider: $SKILLUV_MOMO_PROVIDER  MSISDN: $SKILLUV_MOMO_MSISDN"
echo "═══════════════════════════════════════════════════════════"

# 1. Login
echo "• Step 1/3: login"
curl -sS -c "$COOKIE_JAR" -X POST "$SKILLUV_BASE_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$SKILLUV_TEST_EMAIL\",\"password\":\"$SKILLUV_TEST_PASSWORD\"}" \
    > /dev/null

# 2. Request payout
echo "• Step 2/3: POST payout — 1000 XOF to $SKILLUV_MOMO_MSISDN"
payout_response=$(curl -sS -b "$COOKIE_JAR" -X POST \
    "$SKILLUV_BASE_URL/api/talent-wallet/payouts" \
    -H "Content-Type: application/json" \
    -d "{
        \"amount_cents\": 100000,
        \"currency\": \"xof\",
        \"provider\": \"$SKILLUV_MOMO_PROVIDER\",
        \"msisdn\": \"$SKILLUV_MOMO_MSISDN\",
        \"description\": \"Test run $TEST_RUN_ID\"
    }")
echo "$payout_response"

transaction_id=$(echo "$payout_response" | python3 -c "import sys,json; print(json.load(sys.stdin)['data'].get('transaction_id',''))")
if [[ -z "$transaction_id" ]]; then
    echo "  ❌ No transaction_id returned" >&2
    exit 1
fi

# 3. Poll status (Momo can take 30-60s)
echo "• Step 3/3: poll status for up to 60s"
for i in $(seq 1 12); do
    tx_status=$(curl -sS -b "$COOKIE_JAR" \
        "$SKILLUV_BASE_URL/api/talent-wallet/transactions/$transaction_id" \
        | python3 -c "import sys,json; print(json.load(sys.stdin)['data'].get('status',''))")
    echo "  poll $i/12: status=$tx_status"
    if [[ "$tx_status" == "succeeded" ]]; then
        echo "  ✓ transaction succeeded"
        break
    fi
    if [[ "$tx_status" == "failed" ]]; then
        error=$(curl -sS -b "$COOKIE_JAR" \
            "$SKILLUV_BASE_URL/api/talent-wallet/transactions/$transaction_id" \
            | python3 -c "import sys,json; print(json.load(sys.stdin)['data'].get('error_code','no code'))")
        echo "  ❌ transaction failed — error: $error" >&2
        exit 1
    fi
    sleep 5
done

echo "═══════════════════════════════════════════════════════════"
echo "  ✓ Momo payout test complete — run_id: $TEST_RUN_ID"
echo "═══════════════════════════════════════════════════════════"
echo ""
echo "Cleanup: DELETE FROM talent_wallet_transactions WHERE description LIKE '%$TEST_RUN_ID%';"
