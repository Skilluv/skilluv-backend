#!/usr/bin/env bash
#
# SE-01 -- no credential-shaped default in the config surface.
#
# Every secret must be read from the environment. A default like
# `env::var("X").unwrap_or("sk_live_...")` puts a live-shaped credential in the
# source. This fails on an unwrap_or/unwrap_or_else default that matches a known
# provider prefix or a long base64/hex blob. gitleaks catches committed VALUES;
# this catches a value written as a fallback in code.
set -uo pipefail

# unwrap_or / unwrap_or_else defaults whose string looks like a credential.
if hits=$(grep -rnE '\.unwrap_or(_else)?\s*\(\s*(\|\|\s*)?"[^"]+"' src/ --include='*.rs' 2>/dev/null \
          | grep -iE 'sk_(live|test)_|xkeysib-|AKIA[0-9A-Z]{16}|srt_[0-9a-f]{32}|"[A-Za-z0-9+/]{40,}={0,2}"'); then
  echo "SE-01 FAIL -- credential-shaped default in config/source:"
  printf '%s\n' "$hits" | sed 's/^/  /'
  exit 1
fi
echo "SE-01 ok -- no credential-shaped defaults in source"
