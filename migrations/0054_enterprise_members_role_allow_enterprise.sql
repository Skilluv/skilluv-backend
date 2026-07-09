-- Extend `enterprise_members.role` to accept the 'enterprise' value.
--
-- Rationale: the enterprise SSO layer expresses two provisioning tiers,
-- `recruiter` (standard) and `enterprise` (elevated / admin-of-workspace),
-- and both `enterprise_sso_configs.default_role` and the SCIM group role
-- mapping already CHECK against ('recruiter', 'enterprise'). The membership
-- table was still stuck on the pre-SSO ('owner', 'recruiter') domain, causing
-- provisioning of an 'enterprise'-tier user to fail with a check violation.
--
-- The 'owner' value stays supported ; it's only used at register time by
-- `POST /enterprise/register` for the sole workspace owner.

ALTER TABLE enterprise_members
    DROP CONSTRAINT IF EXISTS enterprise_members_role_check;

ALTER TABLE enterprise_members
    ADD CONSTRAINT enterprise_members_role_check
        CHECK (role IN ('owner', 'recruiter', 'enterprise'));
