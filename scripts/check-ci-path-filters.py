"""The two CI workflows must cover every path exactly once.

`ci.yml` ignores a list of documentation paths so a README typo does not spend
half an hour compiling Rust. `ci-docs-only.yml` claims exactly that list and
reports the two required checks for it, because branch protection waits for
`Build & Lint` and `Integration Tests` by name and a pull request that
triggers neither workflow waits forever.

That only holds while the two lists are identical. Drift either way is silent
and neither is visible in a diff review:

  * a path in `paths-ignore` but not in `paths` — nothing runs, no check is
    reported, and the pull request is unmergeable with no error to read;
  * a path in `paths` but not in `paths-ignore` — both workflows run, two jobs
    report the same required name, and the trivial one can green a real change.

So the lists are compared here rather than trusted to a comment.
"""

import io
import sys

import yaml

CI = ".github/workflows/ci.yml"
DOCS = ".github/workflows/ci-docs-only.yml"


def load(path):
    with io.open(path, encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def main():
    # `on` is parsed as the boolean True by YAML 1.1, which is what PyYAML
    # implements. Accept either key rather than depending on that.
    ci_on = load(CI).get("on") or load(CI).get(True)
    docs_on = load(DOCS).get("on") or load(DOCS).get(True)

    ignored = set(ci_on["pull_request"].get("paths-ignore", []))
    claimed = set(docs_on["pull_request"].get("paths", []))

    if ignored == claimed:
        print(f"ok   the two path filters are complements ({len(ignored)} paths)")
        return 0

    print("FAIL the CI path filters have drifted apart")
    for path in sorted(ignored - claimed):
        print(f"  ignored by ci.yml, claimed by nobody: {path}")
        print("       a pull request touching only this waits for a check that never runs")
    for path in sorted(claimed - ignored):
        print(f"  claimed by ci-docs-only.yml, not ignored by ci.yml: {path}")
        print("       both workflows run and both report the same required check name")
    return 1


sys.exit(main())
