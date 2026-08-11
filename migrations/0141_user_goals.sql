-- SKI-38 (Post-MVP T1-03) — measurable personal goals.
--
-- Turns "I signed up to look around" into "I signed up to reach X". The
-- goal itself stores only the target; progress is always DERIVED at read
-- time from the same tables the proof engine reads (deliverables,
-- user_skills, user_capabilities, user_ranks). Nothing here is a cached
-- counter, so a goal can never disagree with the profile it describes.
--
-- Four kinds, each with its own target shape:
--   rank           — target_value is a rank slug ('ranger', 'artisan', ...)
--   skill_level    — target_value is a proficiency level '1'..'5',
--                    target_skill_id names the skill
--   capability     — target_value is a capability slug ('mentor', ...)
--   artifact_count — target_value is a positive integer of verified
--                    deliverables
--
-- `target_value` is TEXT rather than four typed nullable columns because
-- three of the four kinds are genuinely textual slugs. `target_skill_id`
-- IS a real column: it needs a foreign key so a deleted skill takes its
-- goals with it, which a slug in TEXT could not express.

CREATE TABLE IF NOT EXISTS user_goals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(20) NOT NULL
        CHECK (kind IN ('rank', 'skill_level', 'capability', 'artifact_count')),
    target_value TEXT NOT NULL CHECK (length(target_value) BETWEEN 1 AND 60),
    -- Required for skill_level, forbidden otherwise (constraint below).
    target_skill_id UUID REFERENCES skill_nodes(id) ON DELETE CASCADE,
    -- Optional self-imposed deadline. DATE, not TIMESTAMPTZ: "by end of
    -- March" is a day, and a timezone-exact instant would be false
    -- precision.
    deadline DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set the first time progress reaches 100%. Never cleared: a goal that
    -- was reached stays reached even if the underlying rank system changes.
    achieved_at TIMESTAMPTZ,
    -- Set by the weekly archival job for goals that are done or whose
    -- deadline lapsed. Archived goals stay readable but drop out of the
    -- default listing.
    archived_at TIMESTAMPTZ,

    -- target_skill_id is meaningful for exactly one kind.
    CONSTRAINT user_goals_skill_id_matches_kind CHECK (
        (kind = 'skill_level' AND target_skill_id IS NOT NULL)
        OR (kind <> 'skill_level' AND target_skill_id IS NULL)
    ),

    -- Same goal twice is a user error, not two goals. The partial unique
    -- index below enforces this only among live goals, so a user can
    -- re-set a goal they previously achieved or abandoned.
    CONSTRAINT user_goals_achieved_before_archived CHECK (
        achieved_at IS NULL OR archived_at IS NULL OR archived_at >= achieved_at
    )
);

-- One live goal per (user, kind, target). COALESCE on target_skill_id
-- because NULLs never collide in a unique index, which would let a user
-- stack unlimited identical 'rank' goals.
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_goals_unique_live
    ON user_goals (
        user_id,
        kind,
        target_value,
        COALESCE(target_skill_id, '00000000-0000-0000-0000-000000000000'::UUID)
    )
    WHERE archived_at IS NULL;

-- Primary read path: "my live goals".
CREATE INDEX IF NOT EXISTS idx_user_goals_user_live
    ON user_goals (user_id, created_at DESC)
    WHERE archived_at IS NULL;

-- Archival job scan: live goals with a deadline that may have lapsed.
CREATE INDEX IF NOT EXISTS idx_user_goals_deadline_open
    ON user_goals (deadline)
    WHERE archived_at IS NULL AND deadline IS NOT NULL;
