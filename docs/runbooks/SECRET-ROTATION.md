# Secret rotation & storage (SE-03)

The tests prove a secret is not in the tree and cannot be pulled at runtime.
This is the other half: what to do when one leaks anyway, and where secrets
live in the meantime.

## JWT_SECRET rotation — the containment gesture

Rotating `JWT_SECRET` is how you kill every session at once. Access tokens are
stateless HS256 JWTs signed with it (`AuthService::generate_access_token` /
`verify_access_token`), so once the secret changes, **every existing access
token fails verification** — the whole fleet is logged out on the next request.

Refresh tokens are server-side, in Redis (`refresh:<user_id>`). They are *not*
invalidated by the JWT rotation alone, so a full session kill is two steps:

1. **Rotate the secret.** Set a new 32+ byte random `JWT_SECRET` on the host
   (Coolify → env), redeploy. New value:
   `openssl rand -base64 48`.
2. **Purge refresh tokens.** `redis-cli --scan --pattern 'refresh:*' | xargs redis-cli del`
   (or `FLUSHDB` on the sessions DB if it is dedicated). Without this, a client
   holding a refresh token can mint a new access token under the new secret.

**Expected result:** every user must log in again. Verify on the mirror:
capture a valid session, rotate + purge, replay the old cookie → 401. Time it;
it should be under a minute end to end.

Provider keys (Stripe, Brevo, GitHub OAuth, FedaPay, …) rotate at the provider
console, then update the host env and redeploy. There is no session equivalent
— the old key simply stops working the moment it is revoked upstream.

## If SE-01 / trufflehog finds a secret in history

Purging history does **not** un-leak a value that has already been cloned.
Order matters:

1. **Rotate first.** Revoke the exposed credential at the provider. This is the
   only step that actually contains the leak.
2. **Then scrub.** `git filter-repo --replace-text <(echo 'THE_SECRET==>REMOVED')`,
   force-push, and tell every collaborator to re-clone (a rebase on the old
   history reintroduces the blob).
3. **Record it** in `.gitleaks.toml`'s `[allowlist].commits` only if the commit
   genuinely cannot be scrubbed, with a note that the value was rotated.

## Runtime secret storage — the decision

**Decision: keep secrets as host environment variables on Coolify. Revisit at
either (a) more than ~5 people with deploy access, or (b) the first compliance
requirement that asks for an audit trail on secret access.**

Rationale. At three people, env-vars-on-host is defensible: the attack surface
is the host itself, which already holds the running process and its memory, so
a manager (SOPS/Doppler/Infisical) would add a moving part without removing the
thing an attacker who owns the host already has. What a manager buys — access
audit, rotation workflow, per-environment separation — matters at team scale
and under compliance, not yet. This is a choice, not a default: the trigger
conditions above are when to change it.

`.env.example` is the contract of what must be set; the SE-03 check
(`scripts/check-env-example.sh`) fails CI if the code reads a variable the
example does not document.
