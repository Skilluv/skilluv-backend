#!/usr/bin/env bash
# SKI-35 — Stripe Connect payout end-to-end test.
#
# Runs a real 1€ payout against Stripe test mode. Cleanup manuel via
# le `test_run_id` tagué dans la description.
#
# Prereqs :
#   export STRIPE_TEST_SECRET_KEY="sk_test_..."
#   export STRIPE_TEST_CONNECT_ACCOUNT="acct_test_..."
#   export SKILLUV_BASE_URL="https://staging.skill-uv.com"
#   export SKILLUV_TEST_EMAIL="test@example.com"
#   export SKILLUV_TEST_PASSWORD="..."
#
# See docs/PAYMENT_STAGING_SETUP.md for full setup.

set -euo pipefail

: "${STRIPE_TEST_SECRET_KEY:?}"
: "${STRIPE_TEST_CONNECT_ACCOUNT:?}"
: "${SKILLUV_BASE_URL:?}"
: "${SKILLUV_TEST_EMAIL:?}"
: "${SKILLUV_TEST_PASSWORD:?}"

TEST_RUN_ID="stripe-$(date +%s)"
COOKIE_JAR=$(mktemp)
trap 'rm -f "$COOKIE_JAR"' EXIT

echo "═══════════════════════════════════════════════════════════"
echo "  Stripe payout test — $TEST_RUN_ID"
echo "  Base URL   : $SKILLUV_BASE_URL"
echo "  Connect acct: $STRIPE_TEST_CONNECT_ACCOUNT"
echo "═══════════════════════════════════════════════════════════"

# 1. Login on Skilluv
echo "• Step 1/4: login"
curl -sS -c "$COOKIE_JAR" -X POST "$SKILLUV_BASE_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$SKILLUV_TEST_EMAIL\",\"password\":\"$SKILLUV_TEST_PASSWORD\"}" \
    > /dev/null

# 2. Request payout
echo "• Step 2/4: POST /api/talent-wallet/payouts (amount 100 = 1€)"
payout_response=$(curl -sS -b "$COOKIE_JAR" -X POST \
    "$SKILLUV_BASE_URL/api/talent-wallet/payouts" \
    -H "Content-Type: application/json" \
    -d "{
        \"amount_cents\": 100,
        \"currency\": \"eur\",
        \"provider\": \"stripe\",
        \"stripe_account_id\": \"$STRIPE_TEST_CONNECT_ACCOUNT\",
        \"description\": \"Test run $TEST_RUN_ID\"
    }")
echo "$payout_response"

transaction_id=$(echo "$payout_response" | python3 -c "import sys,json; print(json.load(sys.stdin)['data'].get('transaction_id',''))")
stripe_transfer_id=$(echo "$payout_response" | python3 -c "import sys,json; print(json.load(sys.stdin)['data'].get('stripe_transfer_id',''))")

if [[ -z "$transaction_id" ]]; then
    echo "  FAIL No transaction_id returned — payout failed" >&2
    exit 1
fi

# 3. Verify Stripe side
if [[ -n "$stripe_transfer_id" ]]; then
    echo "• Step 3/4: verify Stripe transfer $stripe_transfer_id"
    stripe_status=$(curl -sS -u "$STRIPE_TEST_SECRET_KEY:" \
        "https://api.stripe.com/v1/transfers/$stripe_transfer_id" \
        | python3 -c "import sys,json; print(json.load(sys.stdin).get('object','unknown'))")
    if [[ "$stripe_status" != "transfer" ]]; then
        echo "  FAIL Stripe API didn't return a transfer object" >&2
        exit 1
    fi
    echo "  OK Stripe confirms transfer exists"
fi

# 4. Poll our backend for status update
echo "• Step 4/4: poll /api/talent-wallet/transactions/$transaction_id"
for i in $(seq 1 12); do
    tx_status=$(curl -sS -b "$COOKIE_JAR" \
        "$SKILLUV_BASE_URL/api/talent-wallet/transactions/$transaction_id" \
        | python3 -c "import sys,json; print(json.load(sys.stdin)['data'].get('status',''))")
    echo "  poll $i/12: status=$tx_status"
    if [[ "$tx_status" == "succeeded" ]]; then
        echo "  OK transaction succeeded"
        break
    fi
    if [[ "$tx_status" == "failed" ]]; then
        echo "  FAIL transaction failed" >&2
        exit 1
    fi
    sleep 5
done

echo "═══════════════════════════════════════════════════════════"
echo "  OK Stripe payout test complete — run_id: $TEST_RUN_ID"
echo "═══════════════════════════════════════════════════════════"
echo ""
echo "Cleanup: DELETE FROM talent_wallet_transactions WHERE description LIKE '%$TEST_RUN_ID%';"
