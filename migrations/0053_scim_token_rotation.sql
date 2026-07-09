-- SCIM token rotation with a grace period.
--
-- When the owner generates a new SCIM token, the previous one is not killed
-- immediately — it stays valid for `previous_scim_token_expires_at` (24 h by
-- default) so operators can rotate the token in their IdP without racing a
-- window of failed sync attempts.

ALTER TABLE enterprise_sso_configs
    ADD COLUMN IF NOT EXISTS previous_scim_token_hash BYTEA,
    ADD COLUMN IF NOT EXISTS previous_scim_token_expires_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_enterprise_sso_previous_scim_hash
    ON enterprise_sso_configs (previous_scim_token_hash)
    WHERE previous_scim_token_hash IS NOT NULL;
