#!/usr/bin/env bash
#
# Read-only server security audit. Run it ON the VPS (the Coolify host behind
# api.skill-uv.com), as root or with sudo. It changes NOTHING -- it only reports
# whether the box is exposed to a brute-force and where it is soft.
#
#   sudo bash server-audit.sh
#
# The verdict at the bottom answers "can the first passer-by force my server?".
# For the fixes, see docs/runbooks/SERVER-HARDENING.md.

set -uo pipefail
ok(){ printf '  [ ok ] %s\n' "$*"; }
warn(){ printf '  [WARN] %s\n' "$*"; WARNS=$((WARNS+1)); }
bad(){ printf '  [BAD ] %s\n' "$*"; BADS=$((BADS+1)); }
WARNS=0; BADS=0

sshd_val(){ # effective sshd setting, config value or compiled default
  sshd -T 2>/dev/null | grep -i "^$1 " | awk '{print $2}' | head -1
}

echo "== SSH configuration =="
PORT=$(sshd_val port); echo "  port: ${PORT:-22}"
PWAUTH=$(sshd_val passwordauthentication)
ROOT=$(sshd_val permitrootlogin)
PUBKEY=$(sshd_val pubkeyauthentication)
[ "$PWAUTH" = "no" ] && ok "password auth disabled (key-only)" || bad "PasswordAuthentication=$PWAUTH -- passwords accepted, brute-forceable"
[ "$PUBKEY" = "yes" ] && ok "public-key auth enabled" || warn "PubkeyAuthentication=$PUBKEY"
case "$ROOT" in
  no|forced-commands-only) ok "root login restricted ($ROOT)";;
  prohibit-password|without-password) [ "$PWAUTH" = "no" ] && ok "root login key-only" || warn "root login allowed with a key; passwords elsewhere are on";;
  *) bad "PermitRootLogin=$ROOT -- root can log in";;
esac

echo "== Brute-force protection =="
if command -v fail2ban-client >/dev/null 2>&1; then
  if fail2ban-client status >/dev/null 2>&1; then
    ok "fail2ban is running"
    if fail2ban-client status sshd >/dev/null 2>&1; then
      banned=$(fail2ban-client status sshd 2>/dev/null | grep -i "Currently banned" | grep -oE '[0-9]+' | head -1)
      total=$(fail2ban-client status sshd 2>/dev/null | grep -i "Total failed" | grep -oE '[0-9]+' | head -1)
      ok "sshd jail active -- ${total:-0} failed attempts seen, ${banned:-0} IPs banned right now"
    else
      warn "fail2ban running but no sshd jail -- SSH is not protected"
    fi
  else
    bad "fail2ban installed but not running"
  fi
else
  [ "$PWAUTH" = "no" ] && warn "no fail2ban (acceptable while password auth is off)" \
                       || bad "no fail2ban AND passwords enabled -- this is the 'anyone can force it' case"
fi

echo "== Recent failed logins (last 24h) =="
if command -v journalctl >/dev/null 2>&1; then
  fails=$(journalctl -u ssh -u sshd --since "24 hours ago" 2>/dev/null | grep -ciE "failed password|invalid user|authentication failure")
  echo "  $fails failed SSH auth events in the journal (last 24h)"
  [ "${fails:-0}" -gt 200 ] && warn "high volume -- you are actively being scanned; fail2ban should be on"
fi
command -v lastb >/dev/null 2>&1 && { echo "  top source IPs of bad logins:"; lastb -a 2>/dev/null | awk '{print $NF}' | grep -E '^[0-9]' | sort | uniq -c | sort -rn | head -5 | sed 's/^/    /'; }

echo "== Open ports =="
if command -v ss >/dev/null 2>&1; then
  ss -tlnH 2>/dev/null | awk '{print $4}' | sed 's/.*://' | sort -un | tr '\n' ' '; echo
  echo "  (each public port is attack surface -- close what the app does not need)"
fi

echo "== ssh-audit (config quality) =="
if command -v ssh-audit >/dev/null 2>&1; then
  ssh-audit -l warn localhost 2>/dev/null | grep -iE "\(warn\)|\(fail\)" | head -10 || ok "ssh-audit found no warn/fail"
else
  warn "ssh-audit not installed -- run: pipx install ssh-audit  (or)  docker run --rm -it positronsecurity/ssh-audit <host>"
fi

echo "== lynis (host hardening) =="
if command -v lynis >/dev/null 2>&1; then
  echo "  run 'sudo lynis audit system' for the full report; hardening index is at the bottom"
else
  warn "lynis not installed -- apt-get install lynis, then: sudo lynis audit system"
fi

echo
echo "==================== VERDICT ===================="
if [ "$BADS" -gt 0 ]; then
  echo "  $BADS critical finding(s), $WARNS warning(s) -- a determined scanner can make progress. Fix the [BAD] lines (SERVER-HARDENING.md)."
  exit 1
elif [ "$WARNS" -gt 0 ]; then
  echo "  0 critical, $WARNS warning(s) -- solid, tighten the [WARN] lines."
else
  echo "  clean -- key-only auth, brute-force protection, no obvious exposure."
fi
