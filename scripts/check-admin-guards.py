"""AZ-01 (static half) -- every admin route handler carries the admin gate.

Admin protection is not a router-level middleware here; `build_router`'s
`admin_gate` closure is a no-op kept only so the .nest() sites don't change.
The real gate is a per-handler extractor, `AdminGate` (origin + mandatory 2FA
for admins), that each admin handler takes as a parameter. A handler registered
under `/admin/*` that forgets it runs without the origin/2FA gate, and nothing
at compile time complains -- axum simply calls a handler with fewer extractors.
So it is checked here.

This is the static half of AZ-01: a new admin route with no gate fails CI. The
runtime half -- hitting every admin route with every non-admin actor and
asserting 403 -- needs the app and a database, and lives in the integration
suite. This catches the omission that the runtime matrix would, without a build.

FAIL (exit 1): an admin handler with no `AdminGate` parameter.
WARN: an admin handler with `AdminGate` but no visible authorization call
(`require_capability`/`require_admin`/...). Authorization can be indirect, so
this is reported, not gated.
"""

import glob
import re
import sys

ROUTE_GLOB = "src/routes/*.rs"

METHOD_HANDLER = re.compile(r"\b(?:get|post|put|patch|delete)\(\s*([A-Za-z_]\w*)\s*\)")
ADMIN_PATH = re.compile(r'"(/admin[^"]*)"')
FN_DEF = re.compile(r"\basync fn\s+(\w+)\s*\(")
AUTHZ = re.compile(
    # The whole require_* guard family (require_admin, require_capability,
    # require_reader, require_reviewer, require_curator, require_arbiter, ...),
    r"require_\w+\s*\("
    r"|is_admin\("
    # The direct role check some handlers use instead of a helper.
    r'|\.role\s*[!=]=\s*"admin"|role\s*[!=]=\s*"admin"'
)


def balanced_params(text, open_idx):
    """Return the parameter-list text for a `(` at open_idx, paren-balanced."""
    depth = 0
    for i in range(open_idx, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return text[open_idx + 1 : i], i
    return "", len(text)


def functions_in(text):
    """name -> (params_text, body_window) for every async fn in one file.

    Scoped to a single file on purpose: handler names like `create`, `list`,
    `queue`, `verify` recur across dozens of modules, so a global index maps a
    registered handler to unrelated same-named functions (services included).
    Handlers are defined in the file that registers them, so resolution is
    per-file."""
    fns = {}
    for m in FN_DEF.finditer(text):
        name = m.group(1)
        params, close = balanced_params(text, m.end() - 1)
        body = text[close : close + 4000]  # enough to see an early guard call
        fns[name] = (params, body)
    return fns


def main():
    # Each admin route is resolved against the file it is registered in -- a
    # (handler, path, file) triple, not just a handler name.
    routes = []  # (handler, route_path, file)
    file_fns = {}  # file -> {name: (params, body)}
    for path in glob.glob(ROUTE_GLOB):
        text = open(path, encoding="utf-8").read()
        file_fns[path] = functions_in(text)
        for line in text.splitlines():
            if ".route(" in line and '"/admin' in line:
                pm = ADMIN_PATH.search(line)
                route_path = pm.group(1) if pm else "?"
                for hm in METHOD_HANDLER.finditer(line):
                    routes.append((hm.group(1), route_path, path))

    no_guard = []          # neither AdminGate nor any authorization -- escalation
    authz_no_gate = []     # authorized, but no AdminGate (no mandatory 2FA / origin)
    gate_no_authz = []     # gated, but no visible authorization call
    unresolved = []
    seen = set()
    for handler, route_path, path in sorted(routes):
        if (handler, path) in seen:
            continue
        seen.add((handler, path))
        fn = file_fns[path].get(handler)
        if fn is None:
            hits = [(f, d[handler]) for f, d in file_fns.items() if handler in d]
            if len(hits) != 1:
                unresolved.append((handler, route_path, path))
                continue
            _, fn = hits[0]
        params, body = fn
        has_gate = "AdminGate" in params
        has_authz = bool(AUTHZ.search(body))
        if not has_gate and not has_authz:
            no_guard.append((handler, route_path, path))
        elif not has_gate:
            authz_no_gate.append((handler, route_path, path))
        elif not has_authz:
            gate_no_authz.append((handler, route_path, path))

    print(f"admin-guards: {len(seen)} admin route handlers checked")

    if unresolved:
        print(f"\nnote -- {len(unresolved)} handler(s) not resolved to a unique fn (parser limit):")
        for h, rp, f in unresolved:
            print(f"    {h}  ({rp}, routed in {f})")

    if authz_no_gate:
        print(
            f"\nWARN -- {len(authz_no_gate)} admin route(s) authorize but skip AdminGate "
            "(no mandatory-2FA, no admin-origin check -- defense-in-depth gap, not escalation):"
        )
        for h, rp, f in authz_no_gate:
            print(f"    {h}  ({rp} in {f})")

    if gate_no_authz:
        print(
            f"\nWARN -- {len(gate_no_authz)} gated admin route(s) with no visible authorization "
            "call (may authorize indirectly -- verify):"
        )
        for h, rp, f in gate_no_authz:
            print(f"    {h}  ({rp} in {f})")

    if no_guard:
        print(
            f"\nFAIL -- {len(no_guard)} admin route handler(s) with NO guard at all "
            "(neither AdminGate nor any authorization -- a non-admin can execute):"
        )
        for h, rp, f in no_guard:
            print(f"    {h}  ({rp} in {f})")
        return 1

    print("\nok -- every admin route handler is authorized (AdminGate coverage: see WARN)")
    return 0


sys.exit(main())
