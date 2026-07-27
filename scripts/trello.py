#!/usr/bin/env python3
"""Backend<->Trello bridge for the SKILLUV board.

Trello acts as the shared queue between backend, frontend and admin. This
script covers the three flows the backend team runs:

  list         Show cards labelled `scope:back` (bugs / tasks assigned to us).
  push         Create new cards from a source .md file (bugs backend spots
               in front/admin, or new backend work).
  move         Move a card by ID prefix to another list (Backlog / En cours /
               Termine / Fait).

Credentials are loaded from `.env.trello` at the repo root (gitignored). See
skilluv-strategy/push-to-trello.py for the original pattern.
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import time
from pathlib import Path

import requests

ROOT = Path(__file__).resolve().parent.parent
ENV_FILE = ROOT / ".env.trello"


def load_env() -> None:
    if not ENV_FILE.exists():
        sys.exit(f"missing {ENV_FILE} — copy from .env.trello template and fill creds")
    for line in ENV_FILE.read_text().splitlines():
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            os.environ.setdefault(k.strip(), v.strip())


load_env()
KEY = os.environ["TRELLO_KEY"]
TOKEN = os.environ["TRELLO_TOKEN"]
BOARD = os.environ["TRELLO_BOARD_ID"]
AUTH = {"key": KEY, "token": TOKEN}
BASE = "https://api.trello.com/1"
SLEEP = 0.2  # ~5 req/s, well under Trello's 100/10s per token cap.


# ─── HTTP ────────────────────────────────────────────────────────────
def req(method: str, path: str, params: dict | None = None, retry: int = 3) -> dict:
    p = {**AUTH, **(params or {})}
    for attempt in range(retry):
        r = requests.request(method, f"{BASE}{path}", params=p, timeout=30)
        if r.status_code == 200:
            time.sleep(SLEEP)
            return r.json() if r.text else {}
        if r.status_code in (429, 500, 502, 503, 504) and attempt < retry - 1:
            time.sleep(2 ** attempt)
            continue
        raise RuntimeError(f"{method} {path} -> {r.status_code}: {r.text[:200]}")
    raise RuntimeError("unreachable")


# ─── Board metadata (cached per run) ─────────────────────────────────
_meta_cache: dict = {}


def board_meta() -> dict:
    if _meta_cache:
        return _meta_cache
    lists = req("GET", f"/boards/{BOARD}/lists", {"fields": "name"})
    labels = req("GET", f"/boards/{BOARD}/labels", {"fields": "name,color"})
    _meta_cache.update(
        lists_by_name={l["name"]: l["id"] for l in lists},
        lists_by_id={l["id"]: l["name"] for l in lists},
        labels_by_name={l["name"]: l["id"] for l in labels if l.get("name")},
        labels_by_id={l["id"]: l["name"] for l in labels if l.get("name")},
    )
    return _meta_cache


def find_list_id(name_fragment: str) -> str | None:
    """Case-insensitive fuzzy match on list name. Trello board has 9 lists with
    accents that break exact match, so we compare on the normalized substring."""
    lower = name_fragment.lower()
    for name, lid in board_meta()["lists_by_name"].items():
        if lower in name.lower():
            return lid
    return None


def ensure_label(name: str, color: str | None = None) -> str:
    m = board_meta()
    if name in m["labels_by_name"]:
        return m["labels_by_name"][name]
    params = {"name": name, "idBoard": BOARD}
    if color:
        params["color"] = color
    lid = req("POST", "/labels", params)["id"]
    m["labels_by_name"][name] = lid
    m["labels_by_id"][lid] = name
    print(f"  + label '{name}' ({color or 'no color'})")
    return lid


# ─── list ────────────────────────────────────────────────────────────
def cmd_list(scope: str, list_filter: str | None) -> None:
    meta = board_meta()
    label_id = meta["labels_by_name"].get(f"scope:{scope}")
    if not label_id:
        sys.exit(f"no label 'scope:{scope}' on board")

    cards = req(
        "GET",
        f"/boards/{BOARD}/cards",
        {"fields": "name,idList,idLabels,shortUrl"},
    )
    matching = [c for c in cards if label_id in c.get("idLabels", [])]

    if list_filter:
        target = find_list_id(list_filter)
        if not target:
            sys.exit(f"no list matching '{list_filter}'")
        matching = [c for c in matching if c["idList"] == target]

    from collections import Counter
    by_list = Counter(meta["lists_by_id"].get(c["idList"], "?") for c in matching)
    print(f"scope:{scope} cards on board (total {len(matching)})\n")
    for lst, count in by_list.most_common():
        print(f"  {count:>3}  {lst}")
    print()
    for c in matching:
        labels = [meta["labels_by_id"].get(lid, "?") for lid in c.get("idLabels", [])]
        prio = next((l for l in labels if l in ("P0", "P1", "P2")), "  ")
        lst = meta["lists_by_id"].get(c["idList"], "?")[:18]
        print(f"  {prio}  [{lst:18}]  {c['name']}")
        print(f"          {c['shortUrl']}")


# ─── push ────────────────────────────────────────────────────────────
# Source .md format: sections separated by `## <ID> - <title>`, then
# description body until next `## `. Optional labels via `**Labels**: a, b, c`
# line inside the body. Priority (P0/P1/P2) auto-detected from the ID prefix
# or from the label list.
ID_LINE = re.compile(r"^##\s+([A-Z0-9-]+)\s*[—-]\s*(.+?)\s*$")
LABEL_LINE = re.compile(r"^\*\*Labels?\*\*\s*:\s*(.+)$", re.IGNORECASE)


def parse_md(path: Path) -> list[dict]:
    items: list[dict] = []
    current: dict | None = None
    for line in path.read_text(encoding="utf-8").splitlines():
        m = ID_LINE.match(line)
        if m:
            if current:
                items.append(current)
            current = {"id": m.group(1), "title": m.group(2), "body": [], "labels": []}
            continue
        if current is None:
            continue
        lm = LABEL_LINE.match(line)
        if lm:
            current["labels"].extend(x.strip() for x in lm.group(1).split(",") if x.strip())
            continue
        current["body"].append(line)
    if current:
        items.append(current)
    return items


def cmd_push(md_path: Path, scope: str, dry_run: bool) -> None:
    if scope not in ("back", "front", "admin"):
        sys.exit(f"scope must be back|front|admin, got '{scope}'")
    if not md_path.exists():
        sys.exit(f"source file not found: {md_path}")

    items = parse_md(md_path)
    if not items:
        sys.exit(f"no `## <ID> - <title>` sections found in {md_path}")

    meta = board_meta()
    backlog_id = find_list_id("backlog") or find_list_id("Backlog")
    if not backlog_id:
        sys.exit("no Backlog list on the board")

    # Existing cards by ID prefix (idempotence).
    existing = req("GET", f"/boards/{BOARD}/cards", {"fields": "name"})
    known_ids = set()
    for c in existing:
        m = re.match(r"^([A-Z0-9-]+)\s*[—-]", c["name"])
        if m:
            known_ids.add(m.group(1))

    scope_label = f"scope:{scope}"
    scope_color = {"back": "orange", "front": "blue", "admin": "purple"}[scope]

    print(f"Source: {md_path}")
    print(f"Scope:  {scope_label}\n")

    to_create, to_skip = [], []
    for it in items:
        if it["id"] in known_ids:
            to_skip.append(it)
        else:
            to_create.append(it)

    print(f"  {len(to_create)} to create, {len(to_skip)} skipped (id already on board)")
    if dry_run:
        print("\n[DRY RUN] cards that would be created:")
        for it in to_create:
            print(f"  + {it['id']} — {it['title']}  labels={it['labels'] or [scope_label]}")
        return

    ensure_label(scope_label, scope_color)
    for prio in ("P0", "P1", "P2"):
        ensure_label(prio, {"P0": "red", "P1": "orange", "P2": "sky"}[prio])
    ensure_label("type:bug", "red")
    ensure_label("type:impl", "green")

    for i, it in enumerate(to_create, 1):
        labels = set(it["labels"]) | {scope_label}
        # Auto-detect priority from ID (e.g. BE-P0-01 → P0).
        pm = re.search(r"[-_]P(\d)[-_]", it["id"])
        if pm:
            labels.add(f"P{pm.group(1)}")
        if "bug" in it["id"].lower() or "type:bug" in labels:
            labels.add("type:bug")

        label_ids = [ensure_label(l) for l in labels]
        params = {
            "idList": backlog_id,
            "name": f"{it['id']} — {it['title']}",
            "desc": "\n".join(it["body"]).strip(),
            "pos": "bottom",
            "idLabels": ",".join(label_ids),
        }
        req("POST", "/cards", params)
        print(f"  [{i:>3}/{len(to_create)}] + {it['id']} — {it['title'][:60]}")


# ─── move ────────────────────────────────────────────────────────────
def cmd_move(card_id_prefix: str, target_list: str) -> None:
    meta = board_meta()
    target = find_list_id(target_list)
    if not target:
        sys.exit(
            f"no list matching '{target_list}'. Available: "
            + ", ".join(meta["lists_by_name"].keys())
        )
    cards = req("GET", f"/boards/{BOARD}/cards", {"fields": "name,idList"})
    matches = [c for c in cards if c["name"].startswith(card_id_prefix)]
    if not matches:
        sys.exit(f"no card with ID prefix '{card_id_prefix}'")
    if len(matches) > 1:
        print(f"warning: {len(matches)} cards match prefix — moving all")
    for c in matches:
        req("PUT", f"/cards/{c['id']}", {"idList": target})
        list_name = next(
            n for n, lid in meta["lists_by_name"].items() if lid == target
        )
        print(f"  {c['name']}  →  {list_name}")


# ─── CLI ─────────────────────────────────────────────────────────────
def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)

    p_list = sub.add_parser("list", help="Show cards for a scope")
    p_list.add_argument("--scope", default="back", choices=["back", "front", "admin"])
    p_list.add_argument("--in", dest="in_list", help="Filter by list name fragment")

    p_push = sub.add_parser("push", help="Push cards from a .md source file")
    p_push.add_argument("file", type=Path)
    p_push.add_argument("--scope", required=True, choices=["back", "front", "admin"])
    p_push.add_argument("--dry-run", action="store_true")

    p_move = sub.add_parser("move", help="Move a card by ID prefix to another list")
    p_move.add_argument("id", help="Card ID prefix (e.g. BE-P0-01)")
    p_move.add_argument(
        "list", help="Target list name fragment (backlog|en cours|termine|fait)"
    )

    args = p.parse_args()

    if args.cmd == "list":
        cmd_list(args.scope, args.in_list)
    elif args.cmd == "push":
        cmd_push(args.file, args.scope, args.dry_run)
    elif args.cmd == "move":
        cmd_move(args.id, args.list)


if __name__ == "__main__":
    main()
