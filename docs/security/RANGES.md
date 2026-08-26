# Hosting the ranges

For operators. What this platform runs, what it does not, and the three things
that were asked for and are not built.

---

## What is hosted: one Juice Shop, shared

`ctf.skill-uv.com` — one OWASP Juice Shop instance, shared, reset nightly.

### Why one and not one per person

Because one is a container and per-person is a system. The per-person version
needs a Docker socket mounted into an application container, dynamic Traefik
routing, a wildcard certificate, a spawn quota, network isolation and a reaper —
which is a fortnight of somebody's time and a permanent operational surface, for
a range whose whole purpose is to be broken.

Shared has two real costs, and both are acceptable:

- Juice Shop's own scoreboard shows what everybody has solved, so it spoils.
  Mitigated by telling people: the objectives in this platform's catalogue are
  described behaviourally, and reading the range's own scoreboard is reading
  the answers.
- One person can break it for everybody. Mitigated by the nightly reset and by
  the scope, which puts denial of service out of bounds.

### Coolify configuration

| | |
|---|---|
| Image | `bkimminich/juice-shop` — pin a tag, do not track `latest` |
| Port | 3000 internal, published through Traefik on 443 |
| Domain | `ctf.skill-uv.com`, CNAME through Cloudflare |
| Certificate | Traefik ACME, HTTP-01 |
| Memory | 256 MB is enough; 512 MB if a competition is running |
| CPU | 0.5 |
| Restart | `unless-stopped` |

**Environment**: none required. Do not set `NODE_ENV=production` — Juice Shop
disables some challenges when it is set.

### The nightly reset

A cron at 04:00 UTC that recreates the container:

```bash
docker restart skilluv-juice-shop
```

A restart is enough: Juice Shop keeps its state in an in-memory database that
is rebuilt on boot. Progress is per browser session, so a reset costs anybody
mid-way through a challenge their session — which is why it runs at 04:00 and
is written on the range's own page.

### Isolation, which is the part not to skip

The container must not be able to reach anything else you run. It is
deliberately vulnerable, and a remote-code-execution challenge in it is a shell
on your network.

- A dedicated Docker network, not the default bridge and not the one the
  application containers are on.
- No route to the database, Redis, MinIO, or the backend.
- Egress to the public internet only. Verify it: from inside the container,
  `curl` the API's internal address and confirm it fails.
- Nothing sensitive in its environment.

Test the isolation the day you deploy it, and again after any network change.
A range that can reach production is worse than no range.

---

## What is not hosted, and why

### One instance per person (C-06)

Not built. The reasoning is above; the honest summary is that it is a system
rather than a feature, and the two problems it solves are the smaller ones.

If it is ever built, what would be needed: a `ctf_instances` table with a
container id, a subdomain and an expiry; a spawn endpoint with a per-user cap
and a global cap; a heartbeat so an idle instance is reaped; a wildcard
certificate via DNS-01; and network isolation per instance. Roughly three days
of work and a permanent thing to operate.

**What it would take to justify it:** the shared instance being unusable
because of spoiling or contention. That is a measurable condition and nobody
has measured it yet, because nobody has used it yet.

### A hosted analysis sandbox (B-06)

Not built. The proposal was JupyterHub with Wireshark, Volatility and pandas
preinstalled, so that somebody with nothing installed could start.

It is a good idea and it is a second authentication surface, a second thing to
patch, and per-user containers again — for a barrier that is real but lower
than it looks: the defensive labs are text and captures, and `tshark` plus
Python is a ten-minute install on any operating system.

The intermediate step, which is built: the onboarding wizard asks what somebody
can actually run (`security_lab_setup`), and `browser_only` is one of the
answers. What that unlocks today is the hosted material — the PortSwigger
Academy, TryHackMe, the reading. What it does not unlock is the defensive labs,
and that is a real gap, stated rather than papered over.

### Skilluv's own vulnerable application

Not built, and not planned. Writing a deliberately vulnerable application that
teaches better than Juice Shop is a year of somebody's work, and Juice Shop is
maintained by OWASP with a hundred contributors. Contributing a challenge to it
is better spent effort, which is why it is a terrain rather than a target.

---

## Adding a range

Anything you host and are allowed to have attacked can carry `ctf_flag`
challenges. Two requirements:

1. **Isolation**, as above. Non-negotiable.
2. **You know the flags**, because you planted them. See `CTF-AUTHORING.md` for
   why this is the line that decides everything else.

Anything you do *not* host — a retired machine, a public dataset — is linked and
carries a human-checked challenge instead. Do not rehost somebody else's
licensed material to make the verification easier.

## Monitoring

Two things worth an alert:

- **The range being down**, because a challenge with an unreachable target
  looks like a broken platform. `/api/health` on the backend does not cover
  it — check the range's own root separately.
- **The range's egress**, if you can. Traffic from that container to anywhere
  other than the public internet means the isolation has broken.

During a competition, watch memory. Ten simultaneous participants with scanners
running is not ten times one person browsing.
