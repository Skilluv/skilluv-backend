# Research mode

The rate limiter is tuned for a person signing up. You are about to send a
hundred payloads at one form. This is how those two coexist.

## Get a token

```bash
curl -X POST https://api.skill-uv.com/api/security/research-token \
     -H 'Content-Type: application/json' \
     -b "$YOUR_SESSION_COOKIE" \
     -d '{"label":"burp on the laptop","days":30}'
```

```json
{
  "data": {
    "token": "srt_9f3c…",
    "details": { "token_prefix": "srt_9f3c81ab", "expires_at": "…" },
    "header": "X-Security-Research-Token"
  }
}
```

**Shown once.** Only its hash is stored, for the same reason a session or an
API key is: a dump of that table must not hand somebody else the raised
ceiling.

You have one live token at a time. Asking for another replaces it — recorded as
`superseded` — because two live tokens means a revocation that does not stop the
traffic, which is the one thing this is for.

## Use it

Two headers on every request:

```
X-Security-Research-Token: srt_9f3c…
X-Security-Research: your-skilluv-handle
```

The first changes behaviour. The second changes nothing and is the courteous
thing to do: it appears in the access log, and it is the difference between
somebody reviewing an afternoon of testing and somebody reviewing an incident.

### Burp Suite

Proxy → Options → **Match and Replace** → Add:

- Type: `Request header`
- Match: leave empty
- Replace: `X-Security-Research-Token: srt_9f3c…`
- Then a second rule for `X-Security-Research: your-handle`

Scope it to `*.skill-uv.com` so the token does not travel to every site you
browse through the proxy. It is a secret, and a proxy is very good at sending
secrets everywhere.

### ZAP

Tools → Options → **Replacer** → Add, with the same two headers and `Enabled`
for requests only.

### curl and scripts

```bash
export SRT='srt_9f3c…'
curl -H "X-Security-Research-Token: $SRT" \
     -H 'X-Security-Research: your-handle' \
     https://staging.skill-uv.com/api/…
```

## What it does

Multiplies the rate-limit ceiling by **ten**. Twenty registrations an hour
becomes two hundred; five reports an hour becomes fifty.

## What it does not do

- It grants **no capability**. Not a route, not a field, not a byte of data you
  could not reach without it. A holder can do exactly what an anonymous visitor
  can do, more times an hour.
- It does **not remove** the limit. Denial of service is out of scope in the
  policy, and a token that lifted the ceiling entirely would make that sentence
  unenforceable. Two hundred registrations an hour is testing; two thousand a
  minute is a stress test, which is the thing the policy forbids.
- It does **not widen the scope**. `SCOPE.md` is unchanged by holding one.

## The rule that revokes it

**Over five hundred requests in a minute** under one token, and the token
revokes itself with `abnormal_volume` recorded.

Five hundred is not a magic number. The decision has to be made at three in the
morning by something that is not a person, and a documented threshold that
occasionally stops an enthusiastic fuzzing run is better than an undocumented
one that never fires. The cost of a false positive is one request:

```bash
curl -X POST https://api.skill-uv.com/api/security/research-token …
```

If you are hitting it repeatedly, tell us — either the threshold is wrong for
what you are doing, or what you are doing is a load test.

## Revoking it yourself

```bash
curl -X DELETE https://api.skill-uv.com/api/security/research-token -b "$COOKIE"
```

Do this if you think the token has leaked — into a shared Burp project file, a
screenshot, a paste. It is not a credential to your account and it is still
attributable to you, which means somebody else's traffic under your token is
your name in the log.

## What we see

Every request under a token is logged with `security_research=true`, your user
id, and the handle you declared. Use is counted in batches — the count on the
token row is approximate on purpose, because the whole point of the token is to
permit a great many requests and an exact figure would mean a database write
per request.

An operator can revoke any token
(`POST /api/admin/security/research-tokens/{id}/revoke`). If yours is revoked
for a reason other than volume, you will hear why.

## Where the implementation is

- `src/services/security_research.rs` — issue, verify, revoke, the volume rule.
- `src/middleware/security_research.rs` — the request-scoped resolution.
- `src/middleware/rate_limit.rs` — where the multiplier is applied.

It is resolved through a task-local rather than a request extension, because
`RateLimiter::check` is called from a hundred handlers and never sees a request.
Threading an extractor through all of them would be ninety-nine correct edits
and one that gets forgotten — and the one that gets forgotten keeps the low
ceiling silently.
