#!/usr/bin/env python3
"""Which backend routes nothing calls, and which callers point at nothing.

The audit of 2026-08-27 (SKI-329) found 317 served endpoints with no caller.
It was done by hand, and the finding that mattered was not the number: it was
the four bugs the walk turned up on the way (SKI-316 to SKI-319), each one an
endpoint that existed, answered, and gave a client nothing it could act on.

That is worth repeating after every large backend batch, so it lives here
instead of being rewritten from memory each time.

## What it does

Two lists, and the diff both ways.

  * Routes the backend registers -- every `.route("...")` in `src/routes/**`.
  * Paths the clients name -- every string literal starting with `/` in
    `skilluv-frontend/src` and `skilluv-admin/src`.

Parameters are normalised on both sides (`{id}`, `${id}` -> `{}`) so that
`/missions/{slug}` and `` `/missions/${slug}` `` are the same path.

## What it deliberately does not do

It does not prove an endpoint is dead. A route named only inside a generated
client, built by string concatenation, or called from a repository this script
does not read, shows up as orphaned and is not. Read the list, do not act on
the count.

The reverse direction -- a path a client names and the backend does not serve
-- would be the stricter one, except that most of what it catches is not a call
at all: `/admin/enterprises/e1` is a test fixture, `/dashboard` is a page route,
`/` is both. So the client side is read from `src/lib/api/**` only, which is
where the calls actually live, and even then a `.test.ts` beside them is
skipped. What survives that filter is worth reading; the raw literal sweep was
287 lines of noise around a handful of signals.

## Usage

    python scripts/orphan-endpoints.py                  # both directions
    python scripts/orphan-endpoints.py --prefix /design # one surface
    python scripts/orphan-endpoints.py --unserved       # only the 404s
"""

import argparse
import io
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BACKEND = os.path.dirname(HERE)
SIBLINGS = os.path.dirname(BACKEND)

CLIENT_REPOS = ["skilluv-frontend/src", "skilluv-admin/src"]
SKIP_DIRS = {"node_modules", "build", ".svelte-kit", "target", "dist", ".git"}

# Surfaces that are not called from a browser and whose absence from the client
# repos means nothing at all.
NOT_CLIENT_FACING = (
    "/health",
    "/metrics",
    "/webhooks/",
    "/scim/",
    "/public/v1/",
    "/.well-known/",
    "/api-docs",
    "/swagger",
)


def normalise(path):
    """One spelling for a path, whichever side wrote it."""
    path = path.strip()
    if path.startswith("/api"):
        path = path[4:]
    path = re.sub(r"\$\{[^}]*\}", "{}", path)  # `/x/${id}`
    path = re.sub(r"\{[^}]*\}", "{}", path)  # `/x/{id}`
    path = re.sub(r"/:[a-zA-Z_][a-zA-Z0-9_]*", "/{}", path)  # `/x/:id`
    return path.rstrip("/") or "/"


def walk(root, extensions):
    for base, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for name in files:
            if name.endswith(extensions):
                yield os.path.join(base, name)


def served():
    """Every path the router registers."""
    found = {}
    routes_dir = os.path.join(BACKEND, "src", "routes")
    for path in walk(routes_dir, (".rs",)):
        text = io.open(path, encoding="utf-8", errors="ignore").read()
        for raw in re.findall(r'\.route\(\s*"([^"]+)"', text):
            found.setdefault(normalise(raw), set()).add(
                os.path.relpath(path, BACKEND).replace("\\", "/")
            )
    return found


def called(api_only=True):
    """Every path a client repository names.

    `api_only` restricts the sweep to `src/lib/api/**`, excluding tests. A page
    route (`/dashboard`) and a mocked id (`/admin/enterprises/e1`) are both
    string literals starting with a slash, and neither is a call to this
    backend; reading the whole tree makes the unserved list unreadable.
    """
    found = {}
    for repo in CLIENT_REPOS:
        root = os.path.join(SIBLINGS, repo)
        if api_only:
            root = os.path.join(root, "lib", "api")
        if not os.path.isdir(root):
            print(f"note: {root} not found, skipping", file=sys.stderr)
            continue
        for path in walk(root, (".ts", ".js", ".svelte")):
            if path.endswith((".test.ts", ".spec.ts")):
                continue
            text = io.open(path, encoding="utf-8", errors="ignore").read()
            for raw in re.findall(
                r"[`'\"](/(?:api/)?[a-z0-9][a-zA-Z0-9_/{}$\-.:]*)", text
            ):
                found.setdefault(normalise(raw), set()).add(repo.split("/")[0])
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", help="only paths starting with this, e.g. /design")
    parser.add_argument(
        "--unserved",
        action="store_true",
        help="only the paths a client calls that the backend does not serve",
    )
    parser.add_argument(
        "--everywhere",
        action="store_true",
        help="sweep the whole client tree, not just src/lib/api. Noisy: page "
        "routes and test fixtures are string literals too.",
    )
    args = parser.parse_args()

    backend = served()
    clients = called(api_only=not args.everywhere)

    def wanted(path):
        return not args.prefix or path.startswith(args.prefix)

    # A client calling something nothing serves. Read from the API layer only,
    # so what is left is closer to a real 404 than to a page route.
    unserved = sorted(
        p for p in clients if p not in backend and wanted(p) and not p.startswith("/{")
    )
    print(f"── {len(unserved)} paths called by a client and served by nothing")
    for path in unserved:
        print(f"   {path:<58} {', '.join(sorted(clients[path]))}")

    if args.unserved:
        return 0

    orphans = sorted(
        p
        for p in backend
        if p not in clients
        and wanted(p)
        and not any(p.startswith(x) for x in NOT_CLIENT_FACING)
    )
    print()
    print(f"── {len(orphans)} routes served with no caller in either client repo")
    print("   (a generated client or a concatenated URL reads as orphaned here)")
    for path in orphans:
        print(f"   {path:<58} {', '.join(sorted(backend[path]))}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
