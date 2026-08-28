# Server hardening — the VPS behind api.skill-uv.com

The single Coolify host is both the test target and the future prod. Coolify
runs the app in Docker; this is about the **host OS** underneath — its SSH door,
which Coolify does not harden for you.

> **Before you change anything about SSH, open a SECOND ssh session and keep it
> open.** If a config change locks you out, the open session is how you undo it.
> Never close your last session right after editing `sshd_config`.

Run the audit first — it changes nothing and tells you where you stand:

```bash
sudo bash scripts/server-audit.sh
```

Fix the `[BAD]` lines below in order.

## 1. Brute-force protection — fail2ban (the "can the first passer-by force it" fix)

Failed SSH logins are constant background noise on any public IP. fail2ban
watches the auth log and bans an IP after a few failures.

```bash
sudo apt-get update && sudo apt-get install -y fail2ban
sudo tee /etc/fail2ban/jail.local >/dev/null <<'EOF'
[DEFAULT]
# Never ban yourself or your test source. Put YOUR home/office IP and, while
# you are pentesting api.skill-uv.com, the machine you scan from -- otherwise
# your own scan gets you banned mid-test.
ignoreip = 127.0.0.1/8 ::1 YOUR.IP.HERE
bantime  = 1h
findtime = 10m
maxretry = 5

[sshd]
enabled = true
EOF
sudo systemctl enable --now fail2ban
sudo fail2ban-client status sshd     # confirm the jail is up
```

`bantime = 1h`, `maxretry = 5`: five wrong tries in ten minutes → banned an
hour. Raise `bantime` to `24h` or `-1` (permanent) once you trust the ignoreip.

## 2. Key-only SSH (remove the thing they brute-force)

fail2ban slows a brute-force; **disabling password auth ends it** — a password
that cannot be sent cannot be guessed. Do this only after confirming your key
works.

```bash
# 1. From your laptop, confirm key auth works (in a NEW session):
ssh -o PreferredAuthentications=publickey you@api.skill-uv.com   # must succeed WITHOUT a password prompt

# 2. Only then, on the server:
sudo sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config
sudo sed -i 's/^#\?PermitRootLogin.*/PermitRootLogin prohibit-password/' /etc/ssh/sshd_config
sudo sshd -t && sudo systemctl reload ssh   # sshd -t validates BEFORE reload; if it errors, do not reload
```

With password auth off, `PermitRootLogin prohibit-password` is fine (root can
only key in). If you have a non-root sudo user, `PermitRootLogin no` is tighter.

## 3. Audit the SSH crypto — ssh-audit

Weak ciphers/MACs/kex are a real finding even with key-only auth.

```bash
pipx install ssh-audit    # or: docker run --rm -it positronsecurity/ssh-audit api.skill-uv.com
ssh-audit api.skill-uv.com
```

Act on any `(fail)`/`(warn)`: remove deprecated algorithms in `sshd_config`
(`KexAlgorithms`, `Ciphers`, `MACs`), `sshd -t`, reload.

## 4. Whole-host audit — lynis

```bash
sudo apt-get install -y lynis
sudo lynis audit system
```

Read the "Hardening index" at the end and the suggestions above it — firewall,
kernel params, package hygiene. Fix the cheap ones; note the rest.

## 5. Firewall — only what the app needs

```bash
sudo ufw default deny incoming
sudo ufw allow OpenSSH        # or your custom SSH port
sudo ufw allow 80,443/tcp     # Coolify's reverse proxy
sudo ufw enable
```

Everything the app does not serve (a stray Postgres/Redis/MinIO port bound to
0.0.0.0) is attack surface — `ss -tlnp` shows what is listening; bind internal
services to `127.0.0.1` or block them here.

---

**Re-run `scripts/server-audit.sh` after each step** — the verdict should walk
from `[BAD]` to clean. And remember the whitelist: while you scan
api.skill-uv.com for the pentest work, your scan source must be in
fail2ban's `ignoreip`, or the scan bans itself.
