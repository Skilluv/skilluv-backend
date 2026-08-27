# Testing & security hardening — status and handoff

Branch **`test/security-hardening`** (worktree `../skilluv-backend-security`,
based on `master`, **not pushed**). Backlog:
`skilluv-strategy/pm-backlog-md/testing-security/`.

The dominant constraint: this is a 160k-line Rust backend on a RAM-limited
Windows box. A lib rebuild OOM-kills at ~3.7 GB free, so anything that compiles
Rust is authored here and **verified in a RAM window or in CI (Linux)**, not on
the dev box. Anything that does not compile Rust (shell/YAML/Python gates) is
verified locally, now. Every commit says which it is.

Build note: this worktree needs `OPENSSL_DIR="C:/Program Files/FireDaemon
OpenSSL 3"` (webauthn-rs → openssl-sys), or the build fails on MSVC.

---

## Commits (newest first)

| Commit | Ticket | Verified? |
|---|---|---|
| `e7109426` | AZ-05 — auth cookies stay SameSite=Strict (the real CSRF defense) | ✅ local |
| `7aef5678` | AZ-02 IDOR — a reporter can't withdraw another's finding | ⏳ RAM window |
| `e44c2b18` | AZ-03 — the admin 2FA gate holds | ⏳ RAM window |
| `54c99762` | AZ-01 runtime matrix — no non-admin reaches an admin route | ⏳ RAM window |
| `e77bd932` | AZ-01 static gate **+ fixed a real escalation** | ✅ gate local; fix ⏳ |
| `95753aed` | SA-04 — osv-scanner over Cargo.lock | ✅ YAML local |
| `89e4ea17` | SA-03 — threat-model grep gate | ✅ local |
| `e527945f` | FZ-02 — embargo-expiry invariant test | ⏳ RAM window |
| `804bee0d` | FZ/MU — proptest core + fuzz crate + mutants CI + parse_scope fix | ✅ core (10 tests) + fix logic (200k); ⏳ 6 tests |
| `2a3f0fe6` | CI-01 — one workflow owns each required check | ✅ local |

---

## 🔴 / 🟠 Needs your attention

1. **FIXED — real escalation.** `POST /api/admin/validators/invite` (`admin_invite`)
   had no authorization at all (not in the handler, not in the
   `validator_applications::invite` service), while its sibling `approve`
   required the `"admin"` capability. Any authenticated user could invite a
   validator — a self-grant path to validator status. Fixed in `e77bd932`
   (added `require_capability(…, "admin")`). Static evidence; the runtime
   matrix (`54c99762`) will confirm it once run.

2. **DECISION — 122 admin routes skip `AdminGate`.** `lib.rs`'s `admin_gate`
   wrapper is a **no-op** (`|r| r`); real admin protection is the per-handler
   `AdminGate` extractor (admin-origin + mandatory 2FA). Core admin files use
   it; ~122 domain-admin routes (tournament, engagements, brand, consultations…)
   authorize via `require_*`/`role != "admin"` but **skip `AdminGate`** — so a
   non-admin is still refused, but the **2FA mandate and admin-origin check do
   not apply** to them. Your `THREAT_MODEL` says admins without 2FA are refused;
   today that only holds on the gated subset. Run `python scripts/check-admin-guards.py`
   for the list. **Decide:** gate them too, or is this deliberate?

3. **DOC — CSRF.** The double-submit `require_csrf` middleware is defined but
   **wired nowhere**. That is fine: the `access_token` cookie is
   `SameSite=Strict`, which blocks the classic CSRF path (locked by AZ-05).
   But the `THREAT_MODEL` doc claims a double-submit cookie on mutating routes —
   reconcile the doc with reality (SameSite=Strict, token unwired).

4. **NOTE — audit immutability (AZ-04) is a prod config guarantee.** Append-only
   is enforced by `REVOKE UPDATE/DELETE` from the app role (migration 0099). In
   dev/test the `skilluv` role is superuser and bypasses the REVOKE, so it is
   **not test-provable here** — verify in prod that the app role is NOSUPERUSER.

5. **AUDIT — AZ-01 runtime matrix first run.** `tests/authz_admin_matrix.rs`
   asserts no non-admin reaches any documented `/admin/*` route. Its first run
   is an audit: any route it lists (watch the 4 dashboard endpoints —
   `overview`/`financial`/`health`/`moderation-queue` — which gate but show no
   in-handler authz) is either an escalation to fix or an intentional exception
   to add to `ALLOW` with a reason.

---

## Verify the pending Rust work (in a RAM window)

Pause parallel compiles first (needs ~2–3 GB more than free). Run **one file at
a time** (running all at once OOM-links).

```bash
cd C:/Users/KPS/flemart/skilluv-backend-security
export OPENSSL_DIR="C:/Program Files/FireDaemon OpenSSL 3"

# No DB needed (pure parsers + the parse_scope fix): 16 tests
cargo test --test prop_pure_parsers

# Need Docker skilluv-* on 5433 (spawn + Postgres):
cargo test --test test_security_domain an_expired_embargo          # FZ-02 embargo
cargo test --test test_security_domain a_reporter_cannot_withdraw  # AZ-02 IDOR
cargo test --test authz_admin_2fa                                  # AZ-03 2FA gate
cargo test --test authz_admin_matrix                               # AZ-01 runtime audit
```

Two one-line src fixes ride along and compile with the above: `parse_scope`
(empty-scope lockout) and `admin_invite` (the escalation). Both mirror existing
working code.

The OOM-free gates run with no build:

```bash
bash   scripts/check-threat-model.sh      # SA-03 + CSRF SameSite
python scripts/check-admin-guards.py      # AZ-01 static (lists the 122)
python scripts/check-required-check-names.py
bash   scripts/detect-code-changes.sh     # CI-01 helper (needs EVENT/BASE_SHA env)
```

---

## Per-ticket status

- **CI** — CI-01 ✅. CI-02/03 largely covered (`mutants-pr.yml` advisory `--in-diff`,
  `fuzz-nightly.yml` nightly fuzz + full mutants, osv in CI). CI-04 (ephemeral
  DAST env), CI-05 (cargo-vet gate), CI-06 (11 schemathesis findings) — open.
- **FZ / MU** — FZ-01 (fuzz crate, CI-nightly) ✅ authored; FZ-02 pure invariants
  ✅ + DB invariants covered by `test_security_domain.rs` + embargo gap filled;
  MU-01 config + CI ✅; MU-02 (coverage.yml) and MU-03 (nextest) already present.
  Fragments-monotonicity is a mutation target (no decrease endpoint).
- **SA** — SA-03 ✅ (grep gate over semgrep, which is unverifiable on Windows);
  SA-04 osv ✅, rest (trivy/SBOM/cosign/SLSA) already existed; cargo-vet open;
  SA-01 (clippy pedantic) and SA-02 (geiger) need a compile.
- **AZ** — AZ-01 static ✅ + runtime ✅ + **found & fixed an escalation**; AZ-02
  IDOR (security domain) ✅; AZ-03 2FA ✅; AZ-04 noted (prod config); AZ-05 ✅.
- **Not started** — DA (pentest, needs the mirror server), CC/DB/LO (concurrency,
  volume, chaos — need running stack), IA (separate service), FE/AD (other repos).

---

## Guidance for continuing

- The OOM-free static-analysis vein is largely mined. Remaining high-value work
  (CI-06 schemathesis, clippy pedantic, concurrency/volume) needs the Rust build
  or a running stack.
- Merge order: the CI-01 change alters the merge gate for **every** branch, so
  land it (or at least review it) before the others.
- When adding a domain, the cross-domain invariants bite (see the rollout memory):
  re-run the gates above after any route or schema change.
