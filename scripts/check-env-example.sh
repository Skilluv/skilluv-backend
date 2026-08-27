#!/usr/bin/env bash
#
# SE-03 -- .env.example lists every environment variable the code reads.
#
# A variable read at runtime but missing from .env.example is a silent prod
# outage waiting to happen (nobody sets it because nobody knew it existed).
# This diffs the `env::var("X")` calls in the source against the keys documented
# in .env.example and fails on any read-but-undocumented variable.
set -uo pipefail

# Vars the code reads.
read_vars=$(grep -rhoE 'env::var(_os)?\s*\(\s*"[A-Z0-9_]+"' src/ --include='*.rs' 2>/dev/null \
            | grep -oE '"[A-Z0-9_]+"' | tr -d '"' | sort -u)

# Vars documented in .env.example (KEY=... lines, ignoring comments).
doc_vars=$(grep -oE '^[A-Z0-9_]+=' .env.example 2>/dev/null | tr -d '=' | sort -u)

missing=$(comm -23 <(printf '%s\n' "$read_vars") <(printf '%s\n' "$doc_vars"))

echo "read from env: $(printf '%s\n' "$read_vars" | grep -c .) vars; documented: $(printf '%s\n' "$doc_vars" | grep -c .)"
if [ -n "$missing" ]; then
  echo "SE-03 FAIL -- read by the code but absent from .env.example:"
  printf '%s\n' "$missing" | sed 's/^/  /'
  exit 1
fi
echo "SE-03 ok -- every env var the code reads is documented in .env.example"
