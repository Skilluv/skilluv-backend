-- Design tools Skilluv does not own.
--
-- ## One table, not one per provider
--
-- W-03 asked for `figma_connections` and W-04 for
-- `design_cloud_connections`, with Figma folded into the second "for
-- coherence". Two tickets, one shape — so it is built once, and Figma is a
-- row in it rather than a table of its own.
--
-- The alternative is four tables that differ by the provider's name and by
-- nothing else, and a token refresh written four times.
--
-- ## Why the tokens are here and not in a vault
--
-- The same reason `enterprise_sso` and `github` keep theirs in Postgres:
-- encrypted at rest with a key that is not in the database, which is the
-- protection that matters against a dump. A vault is a service to run, and
-- the threat it defends against — an attacker with live database *and*
-- application access — already owns the request path.
--
-- ## What this table cannot do on its own
--
-- Nothing here reaches Figma, Miro or Webflow. Every one of them needs a
-- client id and secret from a developer portal, and Skilluv has none: the
-- accounts do not exist yet. The flow, the storage, the URL parsing and the
-- refusals are all here and all testable; the two calls that need a secret
-- answer `502` with a message saying which credential is missing, rather than
-- pretending.

CREATE TABLE IF NOT EXISTS design_cloud_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Only providers with a real OAuth flow. Framer, Adobe XD and InVision
    -- are deliberately absent: they have no public OAuth, so a "connection"
    -- to them would be a row that means nothing. Those are handled as a URL
    -- on the deliverable, which is what they actually are.
    provider VARCHAR(20) NOT NULL CHECK (provider IN ('figma', 'miro', 'webflow')),

    -- Encrypted with a key derived from the application secret, never stored
    -- alongside. Bytea rather than text: a token that has been through base64
    -- is a token somebody has looked at.
    access_token_ciphertext BYTEA NOT NULL,
    access_token_nonce BYTEA NOT NULL,
    -- Absent where the provider issues no refresh token. Figma does, Miro
    -- does, Webflow's v2 tokens do not expire.
    refresh_token_ciphertext BYTEA,
    refresh_token_nonce BYTEA,

    -- What was actually granted, not what was asked for. A scope the person
    -- declined has to be visible, or a later failure reads as a bug.
    scopes TEXT[] NOT NULL DEFAULT '{}',

    -- Null when the provider issues tokens that do not expire.
    expires_at TIMESTAMPTZ,

    -- The account on the other side, for showing "connected as" without
    -- spending a call to find out.
    remote_handle VARCHAR(120),

    connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when somebody disconnects. The row is kept rather than deleted so
    -- a later question about what was fetched and when has an answer; the
    -- tokens are wiped at the same moment.
    revoked_at TIMESTAMPTZ,

    CONSTRAINT cloud_connection_refresh_pair CHECK (
        (refresh_token_ciphertext IS NULL) = (refresh_token_nonce IS NULL)
    )
);

-- One live connection per provider per person. A second would leave two
-- tokens and no rule saying which one a fetch should use.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_live_cloud_connection
    ON design_cloud_connections (user_id, provider)
    WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_cloud_connections_expiring
    ON design_cloud_connections (expires_at)
    WHERE revoked_at IS NULL AND expires_at IS NOT NULL;

COMMENT ON TABLE design_cloud_connections IS
    'OAuth connections to design tools Skilluv does not own. One row per '
    'person per provider; tokens encrypted with a key held outside the '
    'database. Providers without a public OAuth flow are not here — a link '
    'to one of those is a URL on the deliverable, not a connection.';

-- ═══════════════════════════════════════════════════════════════════
-- Where a deliverable actually lives
-- ═══════════════════════════════════════════════════════════════════

-- A design deliverable often is not a file: it is a Figma frame, a Miro
-- board, a Webflow page. `design_external_url` already holds the link; what
-- was missing is which tool it points at, so a reader knows whether it opens
-- without an account and a fetch knows which client to use.
ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS cloud_source_provider VARCHAR(20);

-- Wider than the connections list on purpose: a Framer link is a real
-- deliverable location even though nobody can connect to Framer.
ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_cloud_source_provider_check;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_cloud_source_provider_check
    CHECK (cloud_source_provider IS NULL OR cloud_source_provider IN (
        -- Connectable.
        'figma', 'miro', 'webflow',
        -- URL only, no public OAuth. Named rather than lumped into 'other'
        -- because which tool it is decides whether a reviewer can open it.
        'framer', 'adobe_xd', 'invision', 'sketch_cloud',
        'other'
    ));

COMMENT ON COLUMN project_slices.cloud_source_provider IS
    'Which tool `design_external_url` points at. Decides whether a reviewer '
    'can open it without an account, which is the difference between a '
    'review queue that moves and one that does not.';
