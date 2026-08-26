-- A token that says "this traffic is a person testing us, not an attack".
--
-- ## The problem it solves
--
-- The published scope invites people to attack the staging deployment. The
-- rate limiter of 0030 allows twenty registrations an hour per address, which
-- is right for a person signing up and useless for somebody sending a hundred
-- payloads at the same form. Without something like this, the platform invites
-- research and then blocks it after thirty seconds, and the researcher
-- concludes — correctly — that the invitation was decorative.
--
-- ## Why a token and not an allow-list of addresses
--
-- Addresses move. A researcher works from a laptop, a cloud box and a phone
-- tether in one afternoon, and an allow-list means three support requests.
-- More importantly an address says nothing about who: a token is issued to an
-- account, so every request made under it is attributable, and the audit trail
-- says which person's research the traffic was.
--
-- ## What it does not do
--
-- It does not disable the limiter. It multiplies the ceiling, which keeps
-- denial of service out of scope in fact and not only in the policy document:
-- a researcher gets two hundred registrations an hour, not unlimited. And it
-- is revocable in one statement, by the holder or by an operator, which an
-- allow-list entry in a configuration file is not.
--
-- ## One active token per person
--
-- Enforced by a partial unique index rather than by the service. Two live
-- tokens means a revocation that does not stop the traffic, which is the one
-- thing this table exists to be able to do.
--
-- ## The token is stored as a hash
--
-- Same reason as an API key or a session: a leaked dump of this table must not
-- hand somebody else the raised ceiling. The plaintext is shown once, at
-- issue, and never again.

CREATE TABLE security_research_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- SHA-256 of the plaintext, hex. Unique across the table: two accounts
    -- cannot end up sharing a token even by accident.
    token_hash CHAR(64) NOT NULL UNIQUE,
    -- The first characters of the plaintext, so a holder can tell which token
    -- a log line refers to without the token being recoverable.
    token_prefix VARCHAR(12) NOT NULL,
    -- What the holder called it. "Burp on the laptop".
    label VARCHAR(80),

    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoked_reason VARCHAR(40)
        CHECK (revoked_reason IS NULL OR revoked_reason IN (
            'by_holder',        -- they asked
            'by_operator',      -- somebody here decided
            'abnormal_volume',  -- the rule below tripped
            'scope_violation',  -- they tested something out of scope
            'superseded'        -- they issued a new one
        )),

    -- Read by the operator answering "is anybody using this". Updated on use
    -- rather than counted per request: an exact count would mean a write on
    -- every request the token covers, which is the traffic pattern it exists
    -- to allow a lot of.
    last_used_at TIMESTAMPTZ,
    -- Requests seen under this token, incremented in batches by the
    -- middleware. Approximate on purpose, and enough to notice somebody
    -- running a stress test.
    requests_seen BIGINT NOT NULL DEFAULT 0 CHECK (requests_seen >= 0),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT a_token_expires_after_it_is_issued CHECK (expires_at > issued_at),
    CONSTRAINT a_revocation_says_why CHECK (
        (revoked_at IS NULL) = (revoked_reason IS NULL)
    )
);

COMMENT ON TABLE security_research_tokens IS
    'Raises the rate limit for traffic a named person is generating on '
    'purpose. Multiplies the ceiling rather than removing it, so that denial '
    'of service stays out of scope in fact and not only in the policy.';

COMMENT ON COLUMN security_research_tokens.requests_seen IS
    'Approximate. Counted in batches because the point of the token is to '
    'permit a lot of requests, and an exact figure would mean a write per '
    'request.';

-- One live token per person. The partial index is what makes a revocation
-- actually stop the traffic.
CREATE UNIQUE INDEX uniq_one_live_research_token_per_user
    ON security_research_tokens (user_id)
    WHERE revoked_at IS NULL;

-- The middleware looks up by hash on every request carrying the header, which
-- the UNIQUE constraint on `token_hash` already indexes. No second index here:
-- a partial one on the same column would be a duplicate that only looks
-- narrower.
