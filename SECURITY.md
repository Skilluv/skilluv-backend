# Security Policy

## Reporting a Vulnerability

The Skilluv team takes security seriously. If you discover a security vulnerability, we appreciate your help in disclosing it responsibly.

**Please do NOT open a public GitHub issue for security vulnerabilities.**

### How to report

Send a detailed report to **security@skill-uv.com** (or, if that inbox is not yet active, to the maintainer's email listed on the GitHub profile).

Alternatively, use GitHub's [Private Vulnerability Reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability) feature on this repository.

### What to include

- A clear description of the vulnerability
- Steps to reproduce (with a proof-of-concept if possible)
- The affected version(s) or commit SHA
- The potential impact
- Your suggested remediation (optional but appreciated)

### What to expect

These are the numbers the platform actually enforces, not aspirations — each one
is a constant or a worker, named so you can check it.

- **Acknowledgement**: immediate and automated, on submission.
- **Triage by a person**: within **7 days** (`TRIAGE_SLA_DAYS` in
  `src/services/security_findings.rs`), with a written reason in every case
  including a refusal.
- **Coordinated disclosure**: **90 days** from confirmation by default, per
  finding (`disclosure_policy_days`). Reminders at 30, 7 and 1 days. Nothing is
  published automatically at the end — the embargo sweep flags it for a person,
  because publishing a vulnerability is irreversible and a cron job is the wrong
  thing to be holding that decision.
- **Credit** on the public hall of fame, unless you ask for anonymity, which is
  a checkbox on the report.
- **No money.** This platform has no revenue. What it gives is a verifiable
  attestation, fragments, and its hall of fame. Saying so plainly beats letting
  anybody hope.

### Safe harbour

**We will not pursue anybody who follows the published scope in good faith.**

That includes somebody who crosses a boundary by accident and tells us. It is
the case the commitment exists for: an undertaking that only covered people who
never made a mistake would be an undertaking nobody could rely on.

What it covers: no legal action, no complaint to your employer or your
university, no report to a registrar or a hosting provider, and no attempt to
identify you if you asked to stay anonymous.

What it cannot cover: a third party's system you reached by pivoting — which is
why the scope forbids pivoting — and behaviour that was not in good faith.
Deliberate destruction, extortion, or selling what you found are not
disclosure, and the safe harbour is not a shield for them.

This is our undertaking, following the [disclose.io](https://disclose.io/)
baseline. It is written by the people who built the platform and **no lawyer has
reviewed it** — see `docs/security/LEGAL.md`, which says so in the same words
and lists what a review would settle first.

### Scope

The full, current scope — the exact hosts, the vulnerability classes we want,
the ones we do not, and the rules of engagement — is
**[`docs/security/SCOPE.md`](docs/security/SCOPE.md)**, and in machine-readable
form at `GET /api/security/scope` (no authentication: a researcher decides what
to touch before they have an account).

In summary. In scope:

- `staging.skill-uv.com` — **the preferred target**, fake data, reset nightly
- `api.skill-uv.com`, `skill-uv.com`, `admin.skill-uv.com`
- `ctf.skill-uv.com` — deliberately vulnerable; nothing found there is a finding
- The source of the four public repositories, for reading

Out of scope:

- Third-party services (Stripe, GitHub, Cloudflare, Brevo, Judge0 upstream) —
  report to them
- **Denial of service of any kind**, including load testing. The one
  prohibition whose breach ends the relationship immediately
- Social engineering, phishing, physical attacks
- Missing security headers with no demonstrated impact — send them together as
  hardening
- A dependency advisory with no reachability shown here
- Raw scanner output

The list of hosts here is a copy. The authoritative one is
`DEFAULT_SCOPE_HOSTS` in `src/services/security_findings.rs`, which is what
refuses a submission.

### Testing without fighting the rate limiter

The limiter is tuned for a person signing up, not for a hundred payloads at one
form. `POST /api/security/research-token` returns a token that multiplies your
ceiling by ten and grants nothing else — see
[`docs/security/RESEARCH-MODE.md`](docs/security/RESEARCH-MODE.md).

It multiplies rather than removes, so denial of service stays out of scope in
fact and not only in this document.

### Reporting through the platform

`POST /api/security/reports` from an account, which gives you the whole flow:
notifications at every transition, the reviewer's reasoning on the record, an
attestation with a verification code when the finding is confirmed, and your
name on the hall of fame when it is published.

Proof files go first to `POST /api/security/reports/uploads`, which returns a
key rather than a URL — proof of an unfixed vulnerability is not a public
document.

What happens next, with the clocks and the state machine:
[`docs/security/DISCLOSURE-POLICY.md`](docs/security/DISCLOSURE-POLICY.md).

### Recognition

A confirmed finding earns, automatically:

- an **attestation** with a verification code anybody can check without an
  account, naming the severity and the weakness class;
- **fragments**, scaled by the severity a validator settled on — 1000 for a
  critical, 5 for an informational, so there is no volume strategy;
- a place on the **public hall of fame** (`GET /api/security/hall-of-fame`),
  unless you asked for anonymity;
- credit in the **write-up** when the finding is published, with a sentence
  about what was good about the report.

A confirmed vulnerability also counts towards your platform rank exactly as a
merged pull request does. That is what one cross-domain rank means, and it is
why `deliverables` grew a `security_finding_id` rather than this domain growing
a counter of its own.

Two people who find the same thing: first-to-file decides the finding, and the
second gets an **independent co-discovery** attestation with the timestamps that
show it was not copied. Nothing is merged by a machine — a merge decides who is
credited, and a similarity score does not get to.

Thank you for helping keep Skilluv and its community safe.

---

## Security architecture

### Supply-chain integrity

Every commit that lands on `master` goes through :

| Layer | Tool | What it catches |
|-------|------|-----------------|
| Dependency CVE | `cargo-deny` + Trivy FS | Known-vulnerable crates (two independent DBs) |
| License compliance | `cargo-deny` | Non-allowlisted licenses (see `deny.toml`) |
| Secret leak | `gitleaks` | Hardcoded secrets in git history |
| Source SAST | GitHub CodeQL (weekly) | SQL injection, unsafe deserialization, CWE patterns |
| Docker CVE | Trivy image scan | Vulnerable base image + system packages |
| Workflow security | zizmor + actionlint | Malicious PR injection, over-privileged perms |
| SBOM | syft (CycloneDX + SPDX) | Full inventory attached to every image |
| Provenance | SLSA L3 via slsa-framework | Cryptographic proof of build origin |
| Signature | cosign keyless (Sigstore) | Every image signed by our GH Actions workflow |

### Runtime hardening

- **Non-root container** : image runs as `skilluv:skilluv` (UID 10001)
- **tini** as PID 1 for signal handling + zombie reaping
- **HEALTHCHECK** on `/api/health`
- **RLS** (Row-Level Security) enforced when `SKILLUV_RLS_ENFORCED=1` (P14)
- **Argon2id** password hashing
- **SSO client_secret** encrypted at rest via ChaCha20-Poly1305 (`SSO_ENCRYPTION_KEY`)
- **JWT** short-lived + rotating refresh tokens with SHA-256 hash in DB
- **CORS** allowlist explicit (`ALLOWED_ORIGINS`), no wildcard
- **Rate limiting** on auth endpoints (Redis-backed sliding window)
- **CSRF token** double-submit cookie on state-changing routes
- **Security headers middleware** (HSTS, CSP, X-Frame-Options, etc.)

### Deployment verification

Before pulling any Skilluv backend image, verify its signature :

```bash
scripts/verify-image.sh ghcr.io/skilluv/skilluv-backend:master-<sha>
```

The script checks :
1. cosign signature was issued by our GitHub Actions workflow (keyless)
2. CycloneDX SBOM attestation is present
3. SLSA L3 provenance is present

Non-signed or non-attested images are rejected — the deploy pipeline exits non-zero.

#### Wiring the check in Coolify (BE-P2-OPS-DEPLOY)

To enforce this before every prod deploy :

1. Coolify UI → your `skilluv-backend` service → **Configuration** → **Pre-deploy Command**
2. Paste :
   ```bash
   bash scripts/verify-image.sh "$COOLIFY_IMAGE_TAG" || exit 1
   ```
   (Coolify exposes `$COOLIFY_IMAGE_TAG` — the specific tag it's about to pull.)
3. Save + trigger a deploy. If the tag isn't signed, the deploy aborts and the previous healthy container stays live.

**Test the guard once** by manually pushing an unsigned image with the same tag and observing the deploy refuses. Then never worry about it again unless the signing key rotates.

For emergency rollback to a known-good signed tag, see §Incident response scenario 4 below.

### CI security workflows

All in `.github/workflows/` :

| Workflow | Trigger | Rôle |
|---|---|---|
| `ci.yml` | PR + push master | fmt, clippy, machete, deny, gitleaks, tests ×2 shards, doctests |
| `codeql.yml` | weekly + dispatch | SAST source-level |
| `coverage.yml` | weekly + dispatch | llvm-cov + Codecov |
| `docker-scan.yml` | PR path-filtered + weekly | Trivy image scan + SBOM |
| `trivy.yml` | PR path-filtered + weekly | Trivy FS + IaC |
| `image-sign.yml` | post-CI master | cosign keyless + SBOM attestation |
| `slsa-provenance.yml` | post-CI master | SLSA L3 build provenance |
| `workflow-lint.yml` | PR touching .github/ | actionlint + zizmor |
| `pr-lint.yml` | PR opened/edited | Conventional-commit title enforcement |
| `load-test.yml` | dispatch (nightly ready) | k6 smoke perf test |
| `release-please.yml` | push master | Auto CHANGELOG + version bump |
| `dependabot-auto-merge.yml` | Dependabot PR | Auto-merge minor/patch after CI |

---

## Incident response runbook

### 1. Suspected compromise (secret leaked, unauthorized access, weird traffic)

1. **Rotate the affected credential immediately** :
   - JWT_SECRET → generate new + redeploy → all existing sessions invalidated
   - SSO_ENCRYPTION_KEY → generate new + re-encrypt DB rows (script needed)
   - Stripe/OAuth secrets → rotate in provider dashboard + update `.env`
2. **Revoke suspicious sessions** :
   ```sql
   UPDATE user_sessions SET revoked_at = NOW()
   WHERE user_id IN (<suspected>) OR last_ip = '<attacker IP>';
   ```
3. **Snapshot logs** : Coolify → download last 24h of container logs before rotation
4. **Public disclosure** : if user data affected, follow RGPD Art. 33 (72h notification to CNIL)

### 2. CVE discovered in a dependency

1. `cargo deny check` locally to confirm the finding
2. If a fix is available : `cargo update -p <crate>` + PR
3. If no fix : evaluate exploitability → temporary mitigation (feature flag, WAF rule) → open upstream issue
4. Document in SECURITY-ACKNOWLEDGMENTS.md

### 3. CI security check bypass detected

1. Check `.github/workflows/*.yml` for unauthorized modifications (git blame)
2. Verify branch protection settings weren't tampered with
3. Force re-run all security workflows on affected commits
4. Rotate GitHub PATs / deploy keys if suspected

### 4. Compromised deployment image

1. **Stop the compromised container immediately** (Coolify UI)
2. `cosign verify` the previous known-good image tag
3. Rollback via Coolify to that tag
4. Investigate : GitHub Actions logs, GHCR audit log
5. If signature key compromised : cosign supports key revocation via Sigstore transparency log

---

## Threat model (summary)

> The full model — the diagram, the assets ranked by what an attacker wants, the
> STRIDE tables and the list of what is **not** mitigated — is
> [`THREAT_MODEL.md`](THREAT_MODEL.md). What follows is the summary that predates
> it and stays because it is the operational view.


**In scope for our controls** :
- OWASP Top 10 web (injection, broken auth, XSS, CSRF, insecure deserialization, etc.)
- Supply-chain attacks (typosquatting, malicious deps, compromised registry)
- CI/CD tampering (workflow injection, secret theft via PR)
- Container escape via vulnerable base image
- Session hijacking via network sniffing (TLS everywhere, secure cookies)
- Credential stuffing (rate limiting + Argon2 + WebAuthn opt-in)

**Assumed threats (not fully mitigated)** :
- Nation-state actor with physical access to Hetzner/Netcup datacenter
- Compromised developer workstation (own the maintainer's laptop = own the deploys)
- Zero-day in Postgres, Redis, Docker Engine (patched within 24h of upstream fix)

**Explicitly out of scope** :
- DDoS L7 → offloaded to Cloudflare
- Application-layer runtime monitoring (falco, tetragon) → not warranted at solo VPS scale

---

## Contact

- Security reports : security@skill-uv.com
- Maintainer : @jeremiezitti on GitHub
- Public discussion : never — use private disclosure channels above
