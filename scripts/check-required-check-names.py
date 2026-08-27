"""Each required status check is produced by exactly one workflow job.

Branch protection on `master` requires two checks *by name*: `Build & Lint`
and `Integration Tests`. GitHub matches a required check to a job by its
resolved `name:`, across every workflow file — not by which workflow it lives
in. That has two failure modes, both silent in a diff review, and this guard
fails the build on either:

  * a required name declared by NO job — the check is never reported and every
    pull request waits for it forever (this is what a too-broad `paths-ignore`
    or a rename does);

  * a required name declared by MORE THAN ONE job — two checks share the name,
    GitHub is satisfied by whichever reports success first, and a three-second
    stub can green a real change. This is exactly the ci.yml / ci-docs-only.yml
    duplication that CI-01 removed. Re-introducing any second workflow that
    claims one of these names brings the hole straight back, so it is refused
    here rather than trusted to a comment.

The invariant is "exactly one", not "the two lists match" — the old check
compared two path filters for equality, which said nothing about a pull request
touching Markdown *and* Rust, the case that actually broke.
"""

import glob
import io
import sys

import yaml

# The checks branch protection requires by name. Keep in sync with the repo's
# branch-protection settings; this is the one place that encodes them.
REQUIRED_CHECK_NAMES = ["Build & Lint", "Integration Tests"]

WORKFLOW_GLOB = ".github/workflows/*.yml"


def load(path):
    with io.open(path, encoding="utf-8") as handle:
        return yaml.safe_load(handle) or {}


def job_names(path):
    """Every explicit job `name:` in a workflow file (job-id fallback ignored:
    the required checks are all explicitly named)."""
    doc = load(path)
    jobs = doc.get("jobs") or {}
    return [job["name"] for job in jobs.values()
            if isinstance(job, dict) and isinstance(job.get("name"), str)]


def main():
    # name -> list of workflow files whose jobs declare it
    declared = {name: [] for name in REQUIRED_CHECK_NAMES}
    for path in sorted(glob.glob(WORKFLOW_GLOB)):
        for name in job_names(path):
            if name in declared:
                declared[name].append(path)

    failed = False
    for name in REQUIRED_CHECK_NAMES:
        files = declared[name]
        if len(files) == 1:
            print(f"ok   required check {name!r} declared by exactly one job ({files[0]})")
        elif not files:
            failed = True
            print(f"FAIL required check {name!r} is declared by NO job")
            print("       branch protection will wait for it forever — every PR unmergeable")
        else:
            failed = True
            print(f"FAIL required check {name!r} is declared by {len(files)} jobs:")
            for path in files:
                print(f"         {path}")
            print("       two jobs share a required name — the faster one can green a real change (CI-01)")

    return 1 if failed else 0


sys.exit(main())
