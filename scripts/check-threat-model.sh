#!/usr/bin/env bash
#
# SA-03 - the threat model as a grep gate.
#
# Semgrep is the ticket's tool, but its Rust support is experimental and it does
# not run on the Windows dev box, so its rules could not be verified before
# shipping. These checks encode the same intent in a form that runs anywhere
# git-bash does, compiles nothing, and is transparent enough to read in full -
# the same reasoning that put gitleaks behind its CLI and change-detection
# behind a shell script rather than a third-party action.
#
# Each check is prevention: the tree is clean today, so a hit is a regression
# introduced by the change under review. Add a check here the moment a class of
# bug is worth never seeing again; keep each one precise enough that a hit is
# always real.

set -uo pipefail

fail=0
note() { printf 'THREAT-MODEL: %s\n' "$*"; }

# ── A) No dynamic string inside a sqlx query - the SQL-injection vector ──
# sqlx parameterises with bind(); a format! or concatenation in the query text
# is the one way to reintroduce injection under a library that otherwise
# prevents it. The tree has zero of these - keep it that way.
if hits=$(grep -rnE 'query(_as|_scalar|_file)?[a-z_]*[^(]*\(\s*&?\s*(format!|String::)' src/ --include='*.rs' 2>/dev/null); then
  note "FAIL - dynamic SQL: a format!/String built into a sqlx query. Use bind() parameters."
  printf '%s\n' "$hits" | sed 's/^/  /'
  fail=1
fi

# ── B) No console printing in server code ──
# The server logs through tracing::; a println!/dbg! is either debug residue or
# output going nowhere in production. src/bin/* are CLI tools whose whole job is
# to print, so they are exempt.
if hits=$(grep -rnE '\b(println!|eprintln!|print!|eprint!|dbg!)\s*\(' src/ --include='*.rs' 2>/dev/null | grep -vE '^src/bin/'); then
  note "FAIL - console print in server code. Use tracing:: (src/bin/ CLI tools are exempt)."
  printf '%s\n' "$hits" | sed 's/^/  /'
  fail=1
fi

# ── C) No .unwrap()/.expect() in a route handler ──
# A panic in a handler is a 500 and a dropped connection. Handlers return
# Result and use `?`. This scans only src/routes/ (services and helpers have
# their own justified unwraps); test modules inside those files are the one
# accepted source of noise and are filtered by requiring the call to sit on a
# line that is not inside a #[cfg(test)] block - approximated by excluding lines
# after a `mod tests` marker is impractical in grep, so this check is WARN-only:
# it reports but does not fail, until a compile-time lint (clippy::unwrap_used,
# scoped to routes) can replace it. See the note in the SA-03 commit.
if hits=$(grep -rnE '\.(unwrap|expect)\s*\(' src/routes/ --include='*.rs' 2>/dev/null); then
  count=$(printf '%s\n' "$hits" | grep -c .)
  note "WARN - ${count} .unwrap()/.expect() under src/routes/ (a panic is a 500). Not failing the build; triage with clippy::unwrap_used once a compile is affordable."
fi

# -- D) Auth cookies stay SameSite=Lax, and HttpOnly; Secure -----------------
# They were Strict, and this gate said so. Strict turned out to be unworkable:
# a browser withholds a Strict cookie on a navigation another site began, and
# every OAuth provider return is exactly that - so people landed signed out of
# sessions that had worked the whole way, which read as "linking GitHub does
# not work" for two days.
#
# Lax is the floor now. It still refuses a cross-site POST, which is the
# classic CSRF path; what it no longer refuses is a cross-site top-level GET,
# and require_csrf covers that - see src/middleware/csrf.rs's header for why
# CSRF_ENFORCE matters more than it did.
#
# None is still refused outright: it sends the cookie on cross-site POST too,
# which removes the defence entirely rather than narrowing it.
#
# src/no_strict_session_cookies.rs holds the other direction - that no cookie
# drifts back to Strict - because a test can name the file and line and this
# cannot.
if hits=$(grep -rniE 'SameSite=None' src/ --include='*.rs' 2>/dev/null); then
  note "FAIL - a cookie is SameSite=None, which reopens CSRF. Auth cookies must stay SameSite=Lax."
  printf '%s
' "$hits" | sed 's/^/  /'
  fail=1
fi
if ! grep -rqE 'HttpOnly; Secure; SameSite=Lax' src/routes/auth.rs 2>/dev/null; then
  note "FAIL - the auth cookie builder no longer sets HttpOnly; Secure; SameSite=Lax."
  fail=1
fi

# -- E) Admin authorization goes through the capability, not users.role -----
# P18 moved "is this user an admin?" onto user_capabilities; a handler that
# gates on `auth.role == "admin"` reads the legacy column instead, which the
# modern grant path (admin_grant_capability) never sets - so a capability-only
# admin is wrongly refused. The whole tree was converted (commit 138556d0);
# a new `auth.role (==|!=) "admin"` is that exact regression coming back.
# Doc comments (///) are narration, not code, and are exempt.
if hits=$(grep -rnE '\bauth\.role\s*(==|!=)\s*"admin"' src/routes/ --include='*.rs' 2>/dev/null | grep -vE ':\s*///'); then
  note "FAIL - admin gate on the legacy users.role column. Use require_capability(&state.db, auth.user_id, \"admin\") (or has_capability for a compound check)."
  printf '%s\n' "$hits" | sed 's/^/  /'
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  note "ok - no dynamic SQL, no console prints, auth cookies SameSite=Lax, admin via capability"
fi
exit "$fail"
