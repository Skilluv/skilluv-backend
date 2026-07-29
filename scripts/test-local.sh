#!/usr/bin/env bash
# Run the same test suite CI runs, but locally. Prerequisites:
#   docker compose up -d postgres redis minio mailpit
#
# Usage:
#   scripts/test-local.sh                    # fmt + clippy + full test suite (~15 min)
#   scripts/test-local.sh --lib              # fmt + clippy + unit tests only (~1 min)
#   scripts/test-local.sh --check            # fmt + clippy + audit + gitleaks (no tests, ~1 min)
#   scripts/test-local.sh <test_name>        # fmt + clippy + one integration test file
#
# Exits non-zero on any failure — safe to call from git hooks.

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

# ─── Static checks first — matches CI Build & Lint step ──────────────
# Cheap ; run before tests so we fail fast.
echo "▶ cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "▶ cargo clippy -- -D warnings -A dead_code -A unused_imports"
cargo clippy --lib --bins --tests -- -D warnings -A dead_code -A unused_imports

if command -v cargo-machete > /dev/null 2>&1; then
  echo "▶ cargo machete --with-metadata  (unused deps)"
  cargo machete --with-metadata
else
  echo "⚠  cargo-machete not installed — skipping unused-deps check"
  echo "   Install: cargo install cargo-machete --locked"
fi

# ─── Security scans (cheap, run in --check too) ──────────────────────
# Prefer cargo-deny (broader: CVE + licenses + banned crates) ; fall back
# to cargo-audit if only that is installed.
if command -v cargo-deny > /dev/null 2>&1; then
  echo "▶ cargo deny check advisories licenses bans sources"
  cargo deny check advisories licenses bans sources
elif command -v cargo-audit > /dev/null 2>&1; then
  echo "▶ cargo audit --deny warnings  (cargo-deny not installed, using audit only)"
  cargo audit --deny warnings
else
  echo "⚠  cargo-deny/cargo-audit not installed — skipping CVE + license scan"
  echo "   Install: cargo install cargo-deny --locked"
fi

if command -v gitleaks > /dev/null 2>&1; then
  echo "▶ gitleaks detect --source . --no-git"
  gitleaks detect --source . --no-git --config .gitleaks.toml
else
  echo "⚠  gitleaks not installed — skipping secret scan"
  echo "   Install: https://github.com/gitleaks/gitleaks#installing"
fi

# Allow --check to stop right here for a quick pre-push sanity pass.
if [ "${1:-}" = "--check" ]; then
  echo "✅ fmt + clippy + deny/audit + gitleaks clean (skipping tests)"
  exit 0
fi

# ─── Verify infra is up (needed for tests) ────────────────────────────
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
# Prefer nextest (matches CI) when available ; fall back to cargo test.
if command -v cargo-nextest > /dev/null 2>&1; then
  TEST_RUNNER="cargo nextest run"
  TEST_FLAGS="--test-threads=1 --failure-output=immediate"
else
  echo "ℹ  cargo-nextest not installed. Install with: cargo install cargo-nextest --locked"
  TEST_RUNNER="cargo test"
  TEST_FLAGS="-- --test-threads=1 --nocapture"
fi

if [ "${1:-}" = "--lib" ]; then
  echo "▶ $TEST_RUNNER --lib (unit tests only)"
  exec $TEST_RUNNER --lib
elif [ $# -gt 0 ]; then
  echo "▶ $TEST_RUNNER --test $* $TEST_FLAGS"
  exec $TEST_RUNNER --test "$@" $TEST_FLAGS
else
  echo "▶ $TEST_RUNNER $TEST_FLAGS  (mimics CI exactly)"
  exec $TEST_RUNNER $TEST_FLAGS
fi
