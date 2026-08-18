-- The same lesson as 0228, on the constraint it will happen to next.
--
-- Migration 0228 wrote down why `tournaments_kind_check` lost half its values:
-- a CHECK cannot be extended, only replaced, so every addition is a chance to
-- silently delete somebody else's. Five migrations restate
-- `user_capabilities_capability_check` — 0098, 0117, 0120, 0176, 0210 — and
-- the sixth will be whichever domain gets review rights next.
--
-- Nothing catches it. `require_reviewer_for_orientation` builds the
-- capability name from the orientation row at runtime, so a dropped value
-- does not fail to compile: it produces a grant the database refuses, and the
-- error names a constraint rather than the trade nobody can review.
--
-- Two things are added here, and only one of them is this file. The comment
-- puts the warning where somebody about to rewrite the list will read it. The
-- guard is a test — `every_derivable_reviewer_capability_is_grantable` —
-- which walks the orientations table and asserts the CHECK accepts what
-- `{primary_domain}_reviewer:{reviewer_group}` produces for each. A domain
-- adding trades without adding its capabilities fails there, in CI, before
-- anybody discovers it by being refused.

COMMENT ON CONSTRAINT user_capabilities_capability_check ON user_capabilities IS
    'Every capability, from every domain. Restating this list drops whatever '
    'is missing from it, and the symptom is a reviewer who cannot be granted '
    'rights over a trade that exists — see the test '
    '`every_derivable_reviewer_capability_is_grantable`.';
