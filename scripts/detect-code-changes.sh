#!/usr/bin/env bash
#
# Answers one question for ci.yml's `changes` job: did this ref touch anything
# other than documentation? Prints `code=true` / `code=false` to $GITHUB_OUTPUT.
#
# This replaces the CI-01 design (a workflow-level paths-ignore plus a second
# stub workflow that reported the required checks for docs-only changes). The
# two path filters there were not complements for a pull request touching
# Markdown *and* Rust, so both workflows fired and a three-second stub could
# green a real change. Here a single honest signal gates the expensive jobs.
#
# Safety rule, load-bearing: when the diff range cannot be determined reliably,
# emit code=true. A false positive costs one wasted pipeline; a false negative
# lets a real change skip every test. Never bias toward skipping.
#
# The documentation set MUST stay identical to the paths the old
# ci-docs-only.yml claimed — see the regex below.

set -euo pipefail

# A changed path is "documentation" iff it matches one of these. Everything
# else is "code". Keep in sync with the FUNDING/ISSUE_TEMPLATE/docs list.
doc_re='(\.md$|^docs/|^LICENSE$|^\.gitignore$|^\.editorconfig$|^\.github/ISSUE_TEMPLATE/|^\.github/FUNDING\.yml$)'

ZERO_SHA='0000000000000000000000000000000000000000'

emit() {
  echo "code=$1"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    echo "code=$1" >> "$GITHUB_OUTPUT"
  fi
}

# ── Resolve the diff endpoints ──────────────────────────────────────
old=""
new="HEAD"
case "${EVENT:-}" in
  pull_request|pull_request_target)
    old="${BASE_SHA:-}"
    ;;
  push)
    # First push to a branch reports an all-zero before-sha: no baseline.
    if [ -n "${BEFORE:-}" ] && [ "${BEFORE}" != "${ZERO_SHA}" ]; then
      old="${BEFORE}"
      new="${GITHUB_SHA:-HEAD}"
    fi
    ;;
esac

if [ -z "${old}" ]; then
  echo "no reliable baseline for event '${EVENT:-?}' — running the full pipeline (fail safe)"
  emit true
  exit 0
fi

if ! git cat-file -e "${old}^{commit}" 2>/dev/null; then
  echo "baseline ${old} not reachable — running the full pipeline (fail safe)"
  emit true
  exit 0
fi

# ── Classify ────────────────────────────────────────────────────────
files="$(git diff --name-only "${old}" "${new}" || true)"

if [ -z "${files}" ]; then
  echo "empty diff (${old}..${new}) — running the full pipeline (fail safe)"
  emit true
  exit 0
fi

echo "changed files:"
printf '%s\n' "${files}" | sed 's/^/  /'

if printf '%s\n' "${files}" | grep -vE "${doc_re}" | grep -q .; then
  echo "→ at least one non-documentation file changed"
  emit true
else
  echo "→ documentation-only change"
  emit false
fi
