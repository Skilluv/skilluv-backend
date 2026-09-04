#!/usr/bin/env bash
# Has this exact source tree already passed the integration suite?
#
# Every change is tested twice: once on the pull request, and again on master
# when the merge lands. The second run is the same compiler on the same bytes.
# A merge that fast-forwards or squashes an up-to-date branch produces a commit
# whose *tree* — the content hash of the whole working tree — is identical to
# the head commit already tested. Git guarantees that: same tree object, same
# files, byte for byte, including Cargo.lock and this workflow itself.
#
# So the question is not "is this commit new" (it always is, the SHA includes
# the parent and the timestamp) but "have we run the suite over these exact
# bytes". If we have, running it again cannot discover anything.
#
# Two guards keep this from becoming a hole:
#
#   1. The prior run must have CONCLUDED success, and every job we are about to
#      skip must have concluded success in it — not skipped. A
#      documentation-only run greens with those jobs skipped, and its tree must
#      never license a skip for a tree nobody tested. This is also what stops a
#      licence being passed down a chain of runs that each skipped their way to
#      green.
#   2. Anything that is not an exact tree match falls through to running the
#      suite. Absence of evidence is never read as evidence here.
#
# Writes `hit=true|false` to $GITHUB_OUTPUT.
set -euo pipefail

repo="${GITHUB_REPOSITORY:?}"
tree="$(git rev-parse 'HEAD^{tree}')"
echo "tree of $(git rev-parse --short HEAD) is ${tree}"

hit=false
# The most recent successful CI runs, whatever branch they ran on: the pull
# request that was just merged is among them.
runs="$(gh api "repos/${repo}/actions/runs?status=success&per_page=30" \
          --jq '.workflow_runs[] | select(.name == "CI") | "\(.id) \(.head_sha)"' || true)"

while read -r run_id head_sha; do
  [ -n "${run_id:-}" ] || continue
  other="$(gh api "repos/${repo}/commits/${head_sha}" --jq '.commit.tree.sha' 2>/dev/null || true)"
  [ "${other}" = "${tree}" ] || continue

  # Guard 1: the work we would skip must have actually run in that run —
  # every test shard, and the lint job that the shards and the image build
  # queue behind.
  #
  # Through `gh api --jq`, which uses gh's embedded jq. Piping to a `jq`
  # binary reads as the same thing and is not: the runner has one, a
  # developer machine may not, and the fallback below then answers for a
  # query that never ran.
  verdict="$(gh api "repos/${repo}/actions/runs/${run_id}/jobs?per_page=100" \
    --jq 'def state(f): [.jobs[] | select(f) | .conclusion]
                        | if length == 0 then "none"
                          elif all(. == "success") then "success"
                          else "mixed" end;
          [ state(.name | startswith("Integration Tests (shard")),
            state(.name == "Build & Lint") ]
          | if all(. == "success") then "success" else join("/") end' \
    2>/dev/null || echo unreadable)"
  if [ "${verdict}" = "success" ]; then
    echo "tree ${tree} already passed lint and every shard in run ${run_id} (${head_sha})"
    hit=true
    break
  fi
  echo "run ${run_id} matches the tree but its shards/lint were '${verdict}' — not a licence to skip"
done <<< "${runs}"

echo "hit=${hit}" >> "${GITHUB_OUTPUT}"
echo "already tested: ${hit}"
