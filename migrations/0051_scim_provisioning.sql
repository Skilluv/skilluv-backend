-- SCIM 2.0 provisioning for enterprise B2B SSO.
--
-- IdPs (Okta, Azure AD, JumpCloud, Google Workspace, …) can push user + group
-- lifecycle events to Skilluv via SCIM instead of relying on JIT provisioning
-- at login time. Auth is per-enterprise via a bearer token stored hashed.

-- Bearer token for the SCIM endpoint. Only one active token per enterprise ;
-- rotating one invalidates the previous. `scim_last_used_at` is bumped on
-- every authenticated SCIM request for observability.
ALTER TABLE enterprise_sso_configs
    ADD COLUMN IF NOT EXISTS scim_token_hash BYTEA,
    ADD COLUMN IF NOT EXISTS scim_last_used_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_enterprise_sso_scim_token_hash
    ON enterprise_sso_configs (scim_token_hash)
    WHERE scim_token_hash IS NOT NULL;

-- External ID given by the IdP (e.g. Okta user ID). Lets us satisfy the SCIM
-- idempotency contract: POSTing the same externalId twice returns 409 Conflict.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS scim_external_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_scim_external
    ON users (scim_external_id)
    WHERE scim_external_id IS NOT NULL;

-- SCIM Groups: pure metadata for v1. We store them so the IdP can round-trip
-- its group tree, but membership in a Skilluv sense stays driven by
-- `enterprise_members`. A later iteration will let owners map group names to
-- Skilluv roles (recruiter / enterprise).
CREATE TABLE IF NOT EXISTS scim_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    external_id TEXT,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (enterprise_id, display_name)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_scim_groups_external
    ON scim_groups (enterprise_id, external_id)
    WHERE external_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS scim_group_members (
    group_id UUID NOT NULL REFERENCES scim_groups(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_scim_group_members_user
    ON scim_group_members (user_id);
