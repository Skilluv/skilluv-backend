# Security Policy

## Reporting a Vulnerability

The Skilluv team takes security seriously. If you discover a security vulnerability, we appreciate your help in disclosing it responsibly.

**Please do NOT open a public GitHub issue for security vulnerabilities.**

### How to report

Send a detailed report to **security@skilluv.com** (or, if that inbox is not yet active, to the maintainer's email listed on the GitHub profile).

Alternatively, use GitHub's [Private Vulnerability Reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability) feature on this repository.

### What to include

- A clear description of the vulnerability
- Steps to reproduce (with a proof-of-concept if possible)
- The affected version(s) or commit SHA
- The potential impact
- Your suggested remediation (optional but appreciated)

### What to expect

- **Acknowledgment** within 5 business days
- **Initial assessment** within 10 business days
- **Coordinated disclosure timeline** discussed with the reporter — target 90 days from acknowledgment to public disclosure
- **Credit** in the security advisory unless you request anonymity

### Scope

In scope:
- The code hosted in this repository
- Any deployed instance operated by the Skilluv team (staging, production)

Out of scope:
- Third-party services (Stripe, GitHub, Judge0 upstream, etc.) — please report to the respective vendors
- Denial-of-service attacks against production infrastructure
- Social engineering attacks against Skilluv team members
- Issues in dependencies with existing published CVEs

### Recognition

Reporters of valid vulnerabilities may be listed in a `SECURITY-ACKNOWLEDGMENTS.md` file (with their consent) and, for significant findings, credited in the CVE advisory when applicable.

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

- Security reports : security@skilluv.com
- Maintainer : @jeremiezitti on GitHub
- Public discussion : never — use private disclosure channels above
