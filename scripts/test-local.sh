#!/usr/bin/env bash
# Run the same test suite CI runs, but locally. Prerequisites:
#   docker compose up -d postgres redis minio mailpit
#
# Usage:
#   scripts/test-local.sh                    # runs full test suite (~15 min)
#   scripts/test-local.sh --lib              # unit tests only (~30 sec)
#   scripts/test-local.sh <test_name>        # run one integration test file
#
# Exits non-zero if any test fails — safe to call from git hooks.

set -euo pipefail

# ─── CI-parity env vars (identical to .github/workflows/ci.yml) ───────
export DATABASE_URL="${DATABASE_URL:-postgres://skilluv:skilluv_secret@localhost:5433/skilluv}"
export REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
export MINIO_ENDPOINT="${MINIO_ENDPOINT:-http://localhost:9004}"
export MINIO_ACCESS_KEY="${MINIO_ACCESS_KEY:-skilluv}"
export MINIO_SECRET_KEY="${MINIO_SECRET_KEY:-skilluv_secret}"
export MINIO_BUCKET="${MINIO_BUCKET:-avatars}"
export MINIO_BUCKET_PRIVATE="${MINIO_BUCKET_PRIVATE:-documents}"
export JWT_SECRET="${JWT_SECRET:-test_secret_min_32_chars_long_enough}"
export SMTP_HOST="${SMTP_HOST:-localhost}"
export SMTP_PORT="${SMTP_PORT:-1025}"
export SMTP_TLS="${SMTP_TLS:-none}"
export EMAIL_FROM="${EMAIL_FROM:-noreply@skilluv.local}"
export EMAIL_FROM_NAME="${EMAIL_FROM_NAME:-Skilluv Test}"
export ENVIRONMENT="${ENVIRONMENT:-test}"

# Silence noisy logs during tests
export RUST_LOG="${RUST_LOG:-warn}"

# ─── Verify infra is up ───────────────────────────────────────────────
missing=()
for svc in skilluv-postgres skilluv-redis skilluv-minio skilluv-mailpit; do
  if ! docker ps --format '{{.Names}}' | grep -qx "$svc"; then
    missing+=("$svc")
  fi
done

if [ ${#missing[@]} -gt 0 ]; then
  echo "❌ Missing containers: ${missing[*]}"
  echo "   Run: docker compose up -d postgres redis minio mailpit"
  exit 1
fi

# ─── Run tests ────────────────────────────────────────────────────────
if [ "${1:-}" = "--lib" ]; then
  echo "▶ cargo test --lib (unit tests only)"
  exec cargo test --lib
elif [ $# -gt 0 ]; then
  echo "▶ cargo test --test $* -- --test-threads=1 --nocapture"
  exec cargo test --test "$@" -- --test-threads=1 --nocapture
else
  echo "▶ cargo test -- --test-threads=1 --nocapture  (mimics CI exactly)"
  exec cargo test -- --test-threads=1 --nocapture
fi
