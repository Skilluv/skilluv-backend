-- SCIM Group → role mapping.
--
-- The owner (via `PUT /api/enterprise/sso/scim/groups/{id}/mapped-role`) can
-- flag a SCIM group as conferring a specific Skilluv role on its members. When
-- membership changes, the affected user's `enterprise_members.role` is
-- recomputed: it takes the highest-privilege mapped role across the groups
-- they're in, falling back to the SSO config's `default_role` when they
-- belong to no role-mapping group.
--
-- Role precedence (highest first): enterprise > recruiter.

ALTER TABLE scim_groups
    ADD COLUMN IF NOT EXISTS mapped_role VARCHAR(20)
        CHECK (mapped_role IS NULL OR mapped_role IN ('recruiter', 'enterprise'));

CREATE INDEX IF NOT EXISTS idx_scim_groups_mapped_role
    ON scim_groups (enterprise_id)
    WHERE mapped_role IS NOT NULL;
