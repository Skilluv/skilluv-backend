#!/usr/bin/env python3
"""Wire a Discord server to `discord_channels`, without copying snowflakes by hand.

Migration 0257 routes every announcement through `discord_channels`, keyed by
`(purpose, skill_domain)`, and deliberately seeds nothing: every value there is
a snowflake from one specific server, and inventing one would send real
announcements into a room that does not exist.

So the rows have to come from a real server. Reading them off Discord's own API
beats right-clicking six channels and pasting ids into a chat window — it is
faster, it cannot transpose a digit, and the token never leaves this machine.

## The token

Read from `DISCORD_BOT_TOKEN` in the environment or in `.env`. It is never
printed, never passed on a command line, and never written to the SQL this
emits. `.env` is gitignored; keep it that way.

## What it does, and what it refuses to do by default

Creating channels changes a server other people can see, so it is opt-in:

    python scripts/discord-setup.py              # look, and report
    python scripts/discord-setup.py --create     # create what is missing
    python scripts/discord-setup.py --sql        # emit the INSERT statements

The default run touches nothing. It lists the guilds the bot is in, matches the
channels it needs by name, and tells you which are missing.

## Applying the SQL

Reviewed by a person before it lands, because these five rows decide where
real announcements go:

    python scripts/discord-setup.py --sql > /tmp/discord.sql
    docker exec -i skilluv-postgres psql -U skilluv -d skilluv < /tmp/discord.sql
"""

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request

API = "https://discord.com/api/v10"

# The five the code routes on, and the channel name each expects. `purpose` is
# what `services::discord_announce::Purpose` emits; the name is only a default
# — `--name` overrides one, and an existing channel is matched on it.
PURPOSES = [
    ("general", "annonces", "talent_featured — the weekly featuring"),
    ("contests", "concours", "contest_opened — a contest opens"),
    ("winners", "palmares", "contest_won — a contest is decided"),
    ("missions", "missions", "mission_posted — a paid mission is published"),
    ("promotions", "promotions", "rank_promotion, badge_earned"),
]


def token():
    """The bot token, from the environment or from `.env`. Never printed."""
    tok = os.environ.get("DISCORD_BOT_TOKEN")
    if tok:
        return tok.strip()

    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    env = os.path.join(here, ".env")
    if os.path.isfile(env):
        for line in open(env, encoding="utf-8", errors="ignore"):
            m = re.match(r"\s*DISCORD_BOT_TOKEN\s*=\s*(.+?)\s*$", line)
            if m:
                return m.group(1).strip().strip("'\"")

    sys.exit(
        "DISCORD_BOT_TOKEN is not set.\n"
        "Put it in .env (which is gitignored) as:\n"
        "    DISCORD_BOT_TOKEN=...\n"
        "or export it for this shell. Do not pass it on the command line — it\n"
        "would land in your shell history."
    )


def call(method, path, tok, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        API + path,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bot {tok}",
            "Content-Type": "application/json",
            # Discord asks for one, and a real one makes their logs useful to
            # them when something we do is wrong.
            "User-Agent": "SkilluvSetup (https://skill-uv.com, 1.0)",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=20) as r:
            return json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")[:400]
        if e.code == 401:
            sys.exit("Discord refused the token (401). It may have been reset.")
        if e.code == 403:
            sys.exit(
                f"Discord refused the action (403): {detail}\n"
                "The bot is in the server but lacks the permission. For --create\n"
                "it needs Manage Channels; for a read-only run it needs nothing\n"
                "beyond being a member."
            )
        sys.exit(f"Discord returned {e.code} on {method} {path}: {detail}")


def pick_guild(tok, wanted):
    guilds = call("GET", "/users/@me/guilds", tok)
    if not guilds:
        sys.exit(
            "The bot is not in any server yet. Invite it first:\n"
            "  https://discord.com/api/oauth2/authorize"
            "?client_id=<APP_ID>&permissions=2064&scope=bot%20applications.commands\n"
            "(2064 = Send Messages + Manage Channels, the latter only needed for --create)"
        )
    if wanted:
        for g in guilds:
            if g["id"] == wanted or g["name"] == wanted:
                return g
        sys.exit(f"No server called {wanted!r}. The bot is in: " + ", ".join(g["name"] for g in guilds))
    if len(guilds) > 1:
        sys.exit(
            "The bot is in several servers; name the one you mean with --guild:\n  "
            + "\n  ".join(f"{g['name']}  ({g['id']})" for g in guilds)
        )
    return guilds[0]


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--guild", help="Server name or id, when the bot is in more than one.")
    p.add_argument("--create", action="store_true", help="Create the channels that are missing.")
    p.add_argument("--sql", action="store_true", help="Emit the INSERT statements and nothing else.")
    p.add_argument(
        "--domain",
        default="",
        help="Scope these rows to one skill domain. Empty (the default) is the "
        "catch-all row every announcement falls back to.",
    )
    p.add_argument(
        "--name",
        action="append",
        default=[],
        metavar="PURPOSE=CHANNEL",
        help="Override a channel name, e.g. --name winners=hall-of-fame. Repeatable.",
    )
    args = p.parse_args()

    overrides = dict(n.split("=", 1) for n in args.name if "=" in n)
    tok = token()
    guild = pick_guild(tok, args.guild)
    channels = call("GET", f"/guilds/{guild['id']}/channels", tok)
    # type 0 is a text channel; a voice room cannot receive an announcement.
    by_name = {c["name"]: c for c in channels if c.get("type") == 0}

    if not args.sql:
        print(f"Server: {guild['name']}  ({guild['id']})")
        print()

    resolved, missing = [], []
    for purpose, default_name, what in PURPOSES:
        name = overrides.get(purpose, default_name)
        found = by_name.get(name)

        if found is None and args.create:
            found = call(
                "POST",
                f"/guilds/{guild['id']}/channels",
                tok,
                {"name": name, "type": 0, "topic": f"Skilluv — {what}"},
            )
            if not args.sql:
                print(f"  created  #{name}")

        if found is None:
            missing.append((purpose, name, what))
        else:
            resolved.append((purpose, name, found["id"], what))
            if not args.sql:
                print(f"  found    #{name:<14} {found['id']}  → {purpose}")

    if missing and not args.sql:
        print()
        for purpose, name, what in missing:
            print(f"  MISSING  #{name:<14} {'':18}  → {purpose}   ({what})")
        print()
        print("Create them yourself, or re-run with --create (needs Manage Channels).")

    if not resolved:
        sys.exit(1 if missing else 0)

    if args.sql or not missing:
        out = sys.stdout if args.sql else sys.stderr
        if not args.sql:
            print()
            print("Every channel resolved. The rows to apply:")
            print()
            out = sys.stdout
        print("-- Generated by scripts/discord-setup.py.")
        print(f"-- Server: {guild['name']} ({guild['id']}).")
        print("-- `skill_domain = ''` is the catch-all: one row per purpose covers")
        print("-- every domain. Add domain-scoped rows later to split them.")
        print("INSERT INTO discord_channels (purpose, skill_domain, channel_id, label) VALUES")
        rows = [
            f"    ('{purpose}', '{args.domain}', '{cid}', '#{name}')"
            for purpose, name, cid, _ in resolved
        ]
        print(",\n".join(rows))
        print("ON CONFLICT (purpose, skill_domain) DO UPDATE")
        print("    SET channel_id = EXCLUDED.channel_id,")
        print("        label      = EXCLUDED.label,")
        print("        updated_at = NOW();")

    if not args.sql:
        print()
        print("Then, on the deployment:")
        print(f"    DISCORD_GUILD_ID={guild['id']}")
        print("    DISCORD_BOT_TOKEN=(the one in your .env, set in Coolify)")
        print()
        print("Run `skilluv-discord-bot`, not `skilluv-discord-notifier`: the")
        print("notifier is the v1 webhook fallback and cannot route these five")
        print("purposes. It registers the slash commands itself on first connect.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
