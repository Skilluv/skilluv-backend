-- SKI-79 (P26 v2 C-01) — orientation sensitivity on project_slices.
--
-- A slice can now declare a non-empty `required_orientation_slugs` array.
-- When set, only users who hold an active `user_orientation` matching one
-- of the listed slugs may claim it. Empty (the default) → no restriction.
--
-- Rationale: some challenges only make sense within a specific career
-- orientation (a "senior backend rust" refactor is a bad first challenge
-- for someone whose 3 active orientations are all frontend/design). This
-- lets slice authors gate access without gating discovery — the slice
-- still appears in listings; claiming is what fails.
--
-- Admin override: the enforcement lives in the service layer, so an admin
-- endpoint (P26 v2 SKI-P26-ADMIN, later) can bypass by directly calling
-- SQL. No trigger — the CHECK constraint would refuse the admin path.

ALTER TABLE project_slices
    ADD COLUMN IF NOT EXISTS required_orientation_slugs TEXT[] NOT NULL DEFAULT '{}';

-- Slug shape (mirrors orientations.slug from migration 0088) is validated
-- at the service layer (see `SlicesService::assert_orientation_access`);
-- we deliberately do NOT foreign-key to orientations.slug because a
-- future rename of an orientation must not silently orphan a slice.
--
-- Partial index: only pay the cost for slices that actually use the gate.
CREATE INDEX IF NOT EXISTS idx_slices_orientation_gated
    ON project_slices USING GIN (required_orientation_slugs)
    WHERE array_length(required_orientation_slugs, 1) > 0;
