#!/usr/bin/env python3
"""Bring a Discord server to the state `ops/discord/server.toml` declares.

The same shape as `services::seed`: one command, run as often as you like, that
compares what should exist to what does and applies the difference. Running it
twice changes nothing the second time.

## What it can do

Categories, channels, roles, and the `discord_channels` routing rows that make
`contest_opened` land in #design-concours rather than nowhere.

## What it cannot, and says so instead of pretending

Four things depend on you, and the report names them every run:

  * **The bot token.** Yours to create and paste into `.env`. Never printed
    here, never passed on a command line, never written into the SQL.
  * **The invite.** A bot that is in no server has nothing to configure.
  * **Permissions.** Manage Channels to create rooms, Manage Roles to create
    roles, and for roles the bot must sit *above* them in the server's
    hierarchy — Discord refuses otherwise and there is no API to promote
    yourself.
  * **Granting roles to people.** Not implemented, and not a gap in this
    script. `users.discord_user_id` exists (migration 0138) and nothing fills
    it, so the platform cannot tell which Discord member is which account.
    Until something does, every role is created empty and handed out by you.

## Usage

    python scripts/discord-setup.py                 # look, report, touch nothing
    python scripts/discord-setup.py --create        # apply what is missing
    python scripts/discord-setup.py --sql           # emit the routing rows
    python scripts/discord-setup.py --only design   # one domain at a time

`--create` is opt-in because it changes a server other people can see.
"""

import argparse
import json
import os
import re
import sys
import time
import tomllib
import urllib.error
import urllib.request

API = "https://discord.com/api/v10"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPEC = os.path.join(ROOT, "ops", "discord", "server.toml")

CHANNEL_TEXT, CHANNEL_VOICE, CHANNEL_CATEGORY, CHANNEL_ANNOUNCEMENT = 0, 2, 4, 5

# A Windows console is cp1252 by default, and a server called "Skilluv Café"
# would take the whole run down on the first print. Best effort: where the
# runtime allows it the stream becomes UTF-8, and the report itself stays ASCII
# so that even where it does not, nothing crashes.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8")
    except (AttributeError, ValueError):
        pass


def die(msg):
    sys.exit(msg if msg.endswith("\n") else msg + "\n")


def token():
    """The bot token, from the environment or `.env`. Never printed."""
    tok = os.environ.get("DISCORD_BOT_TOKEN")
    if not tok:
        env = os.path.join(ROOT, ".env")
        if os.path.isfile(env):
            for line in open(env, encoding="utf-8", errors="ignore"):
                m = re.match(r"\s*DISCORD_BOT_TOKEN\s*=\s*(.+?)\s*$", line)
                if m:
                    tok = m.group(1).strip().strip("'\"")
                    break
    if not tok:
        die(
            "DISCORD_BOT_TOKEN is not set.\n\n"
            "Put it in .env, which is gitignored:\n"
            "    DISCORD_BOT_TOKEN=...\n\n"
            "Not on the command line: that lands in your shell history, and\n"
            "gitleaks runs on this repository."
        )
    return tok


def call(method, path, tok, body=None, retries=5):
    """One API call, with Discord's rate limit honoured rather than fought."""
    for attempt in range(retries):
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(
            API + path,
            data=data,
            method=method,
            headers={
                "Authorization": f"Bot {tok}",
                "Content-Type": "application/json",
                "User-Agent": "SkilluvSetup (https://skill-uv.com, 1.0)",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            raw = e.read().decode(errors="replace")
            # 429 is expected: creating 152 channels is exactly the shape
            # Discord rate-limits. Wait what it asks and carry on.
            if e.code == 429 and attempt < retries - 1:
                try:
                    wait = float(json.loads(raw).get("retry_after", 1)) + 0.5
                except Exception:
                    wait = 2.0
                time.sleep(wait)
                continue
            if e.code == 401:
                die("Discord refused the token (401). It may have been reset.")
            if e.code == 403:
                die(
                    f"Discord refused the action (403).\n{raw[:300]}\n\n"
                    "The bot is a member but lacks the permission. Creating\n"
                    "channels needs Manage Channels; creating roles needs Manage\n"
                    "Roles AND the bot's own role sitting above them in Server\n"
                    "Settings > Roles. Neither can be granted from here."
                )
            die(f"Discord returned {e.code} on {method} {path}:\n{raw[:400]}")
    die("Discord kept rate-limiting; try again in a minute.")


def pick_guild(tok, wanted):
    guilds = call("GET", "/users/@me/guilds", tok)
    if not guilds:
        die(
            "The bot is in no server. Invite it, then run this again:\n\n"
            "  https://discord.com/api/oauth2/authorize"
            "?client_id=<APP_ID>&permissions=268435472&scope=bot%20applications.commands\n\n"
            "268435472 = Send Messages + Manage Channels + Manage Roles."
        )
    if wanted:
        for g in guilds:
            if wanted in (g["id"], g["name"]):
                return g
        die(f"No server {wanted!r}. The bot is in: " + ", ".join(g["name"] for g in guilds))
    if len(guilds) > 1:
        die(
            "The bot is in several servers; name one with --guild:\n  "
            + "\n  ".join(f"{g['name']}  ({g['id']})" for g in guilds)
        )
    return guilds[0]


def main():
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    p.add_argument("--guild", help="Server name or id, if the bot is in several.")
    p.add_argument("--create", action="store_true", help="Apply what is missing.")
    p.add_argument("--sql", action="store_true", help="Emit the routing rows only.")
    p.add_argument("--only", metavar="DOMAIN", help="One domain, e.g. design.")
    p.add_argument("--no-roles", action="store_true", help="Channels only.")
    args = p.parse_args()

    if not os.path.isfile(SPEC):
        die(f"No spec at {SPEC}")
    spec = tomllib.load(open(SPEC, "rb"))

    cats = spec["categories"]
    roles_wanted = [] if args.no_roles else spec.get("roles", [])
    if args.only:
        cats = [c for c in cats if c["domain"] == args.only]
        roles_wanted = [r for r in roles_wanted if r.get("domain") == args.only]
        if not cats:
            die(f"No domain {args.only!r} in the spec.")

    tok = token()
    guild = pick_guild(tok, args.guild)
    gid = guild["id"]
    say = (lambda *a: None) if args.sql else print

    existing = call("GET", f"/guilds/{gid}/channels", tok)
    have_cat = {c["name"].upper(): c for c in existing if c["type"] == CHANNEL_CATEGORY}
    # Text and announcement channels share a namespace and are interchangeable
    # by a PATCH, so they are matched together; a voice room may legitimately
    # carry the same name as a text one.
    have_txt = {
        c["name"]: c
        for c in existing
        if c["type"] in (CHANNEL_TEXT, CHANNEL_ANNOUNCEMENT)
    }
    have_voice = {c["name"]: c for c in existing if c["type"] == CHANNEL_VOICE}
    have_role = {r["name"]: r for r in call("GET", f"/guilds/{gid}/roles", tok)}

    # Announcement channels and forums exist only on a Community server, and
    # Community is a wizard in Server Settings — no API turns it on. Where it is
    # off, an announcement channel is created as ordinary text and reported, so
    # the run succeeds rather than failing on a setting nobody can change from
    # here.
    community = "COMMUNITY" in call("GET", f"/guilds/{gid}", tok).get("features", [])

    say(f"Server: {guild['name']}  ({gid})")
    say(f"Community mode: {'on' if community else 'off'}")
    say("")

    created, present, missing, routing, pending_announce = 0, 0, [], [], []

    for cat in cats:
        parent = have_cat.get(cat["name"].upper())
        if parent is None:
            if args.create:
                parent = call(
                    "POST",
                    f"/guilds/{gid}/channels",
                    tok,
                    {"name": cat["name"], "type": CHANNEL_CATEGORY},
                )
                have_cat[cat["name"].upper()] = parent
                say(f"  + category  {cat['name']}")
                created += 1
            else:
                missing.append(f"category {cat['name']}")

        for ch in cat["channels"]:
            voice = ch.get("kind") == "voice"
            # An announcement channel needs Community; without it the room is
            # still wanted, just as ordinary text.
            wants_announce = bool(ch.get("announcement")) and not voice
            ctype = (
                CHANNEL_VOICE
                if voice
                else CHANNEL_ANNOUNCEMENT
                if (wants_announce and community)
                else CHANNEL_TEXT
            )
            pool = have_voice if voice else have_txt
            found = pool.get(ch["name"])

            if found is None and args.create:
                body = {"name": ch["name"], "type": ctype}
                if parent:
                    body["parent_id"] = parent["id"]
                found = call("POST", f"/guilds/{gid}/channels", tok, body)
                pool[ch["name"]] = found
                mark = "voice " if voice else "annonce" if ctype == CHANNEL_ANNOUNCEMENT else "channel"
                say(f"  + {mark:9} {'' if voice else '#'}{ch['name']}")
                created += 1
            elif found is None:
                missing.append(("" if voice else "#") + ch["name"] + (" (voice)" if voice else ""))
            else:
                present += 1
                # Community may have been switched on since the last run. A
                # text channel that should be an announcement one is upgraded
                # in place rather than left behind, which is the whole point of
                # a command you re-run.
                if (
                    args.create
                    and wants_announce
                    and community
                    and found.get("type") == CHANNEL_TEXT
                ):
                    call(
                        "PATCH",
                        f"/channels/{found['id']}",
                        tok,
                        {"type": CHANNEL_ANNOUNCEMENT},
                    )
                    say(f"  ^ annonce   #{ch['name']} (upgraded from text)")
                elif wants_announce and not community:
                    pending_announce.append(ch["name"])

            if found and "purpose" in ch:
                routing.append((ch["purpose"], cat["domain"], found["id"], ch["name"]))
            if found and "also_purpose" in ch:
                routing.append((ch["also_purpose"], cat["domain"], found["id"], ch["name"]))

    for role in roles_wanted:
        if role["name"] in have_role:
            present += 1
            continue
        if args.create:
            call("POST", f"/guilds/{gid}/roles", tok, {"name": role["name"], "mentionable": True})
            say(f"  + role      @{role['name']}")
            created += 1
        else:
            missing.append(f"@{role['name']}")

    say("")
    say(f"{present} already there, {created} created, {len(missing)} missing.")
    if missing and not args.create:
        say("")
        for m in missing[:40]:
            say(f"  missing  {m}")
        if len(missing) > 40:
            say(f"  ... and {len(missing) - 40} more")
        say("")
        say("Re-run with --create to make them.")

    # ── The routing rows ────────────────────────────────────────────
    if routing and (args.sql or args.create or not missing):
        if not args.sql:
            say("")
            say("Routing rows to apply — review before running them, these five")
            say("decide where real announcements land:")
            say("")
        print("-- Generated by scripts/discord-setup.py from ops/discord/server.toml.")
        print(f"-- Server: {guild['name']} ({gid}).")
        print("INSERT INTO discord_channels (purpose, skill_domain, channel_id, label) VALUES")
        rows = sorted({(p, d, cid, n) for p, d, cid, n in routing})
        # NULL, not the empty string: migration 0440 replaced that sentinel
        # and pointed the column at skill_domains, so '' is now rejected
        # outright rather than merely never matched.
        print(
            ",\n".join(
                "    ('%s', %s, '%s', '#%s')" % (p, f"'{d}'" if d else "NULL", cid, n)
                for p, d, cid, n in rows
            )
        )
        # The primary key went with the sentinel; uniqueness is a unique index
        # over COALESCE, and an ON CONFLICT target must name that expression.
        print("ON CONFLICT (purpose, COALESCE(skill_domain, '')) DO UPDATE")
        print("    SET channel_id = EXCLUDED.channel_id,")
        print("        label      = EXCLUDED.label,")
        print("        updated_at = NOW();")

    if args.sql:
        return 0

    # ── What is yours ───────────────────────────────────────────────
    say("")
    say("=" * 62)
    say("  Yours to do — this script cannot")
    say("=" * 62)
    fallback = {n: cid for pp, dd, cid, n in routing if not dd and pp in ("general", "promotions")}
    say("  1. Set these on the deployment, beside DISCORD_BOT_TOKEN:")
    say(f"       DISCORD_GUILD_ID={gid}")
    for _name, _var in (("annonces", "DISCORD_ANNONCES_CHANNEL_ID"),
                        ("promotions", "DISCORD_PROMOTIONS_CHANNEL_ID")):
        if _name in fallback:
            say(f"       {_var}={fallback[_name]}")
    say("     The bot refuses to start without the last two. They are where an")
    say("     announcement goes when its domain has no room of its own, and")
    say("     posting in the default beats dropping it.")
    say("")
    say("  2. Run `skilluv-discord-bot`, NOT `skilluv-discord-notifier`.")
    say("     The notifier is the v1 webhook fallback and cannot route the")
    say("     five purposes above. The bot registers its slash commands on")
    say("     its first connection — nothing to do for those.")
    say("")
    say("  3. Apply the SQL above, once you have read it.")
    say("")
    manual = [r["name"] for r in roles_wanted if r.get("manual")]
    if not community:
        say("  4. Turn on Community mode if you want real announcement")
        say("     channels (Server Settings > Enable Community). It is a")
        say("     wizard, not an API call. These were created as ordinary")
        say("     text and are upgraded in place the next time you run this:")
        wanted = sorted(set(pending_announce))
        for i in range(0, min(len(wanted), 12), 6):
            say("       " + ", ".join("#" + n for n in wanted[i:i + 6]))
        if len(wanted) > 12:
            say(f"       ... and {len(wanted) - 12} more")
        say("")

    step = "5" if not community else "4"
    say(f"  {step}. Hand out every role yourself, for now. Nothing fills")
    say("     users.discord_user_id, so the platform cannot tell which")
    say("     Discord member is which account, and a role granted on a guess")
    say("     is an authority nobody earned.")
    if manual:
        say("     These have no platform meaning at all and are yours for good:")
        say("       " + ", ".join(f"@{m}" for m in manual))
    say("=" * 62)
    return 1 if (missing and not args.create) else 0


if __name__ == "__main__":
    raise SystemExit(main())
