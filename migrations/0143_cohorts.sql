-- SKI-40 (Post-MVP T2-01) — time-boxed study cohorts.
--
-- ## How a cohort differs from the two group primitives we already have
--
--   * a **team** exists to ship one shared artifact and dissolves with it;
--   * a **guild** is a long-lived identity with no end date;
--   * a **cohort** is a *bounded learning cycle*: "Rust bootcamp Q3 2026",
--     "Reconversion pentest sept-nov". It has a start, an end, collective
--     milestones, and it is expected to be over.
--
-- That end date is the whole point, so `starts_at` / `ends_at` are NOT
-- NULL and ordered by a CHECK. A cohort without an end would just be a
-- worse guild.
--
-- Not to be confused with `tenant_cohorts` (migration 0045), which groups
-- B2B tenant members and is unrelated.
--
-- ## Group chat
--
-- The ticket suggested extending `dm_conversations` with a nullable group
-- id. That table is built around a two-party invariant — `user_a_id <
-- user_b_id`, `UNIQUE (user_a_id, user_b_id)`, and a CHECK that they
-- differ — all of which would have to be dropped to admit an N-party row,
-- weakening the guarantees that direct messages currently rely on. A
-- dedicated `cohort_messages` table costs one more table and leaves DMs
-- exactly as safe as they are today.

CREATE TABLE IF NOT EXISTS cohorts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(60) NOT NULL UNIQUE
        CHECK (slug ~ '^[a-z0-9-]+$' AND length(slug) BETWEEN 3 AND 60),
    name VARCHAR(120) NOT NULL CHECK (length(name) BETWEEN 3 AND 120),
    description TEXT NOT NULL DEFAULT '' CHECK (length(description) <= 4000),
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    -- Upper bound on membership. Small-group dynamics are the product:
    -- past ~30 people a cohort stops being a cohort and becomes a forum.
    max_members INTEGER NOT NULL DEFAULT 20
        CHECK (max_members BETWEEN 2 AND 30),
    -- Optional thematic anchor. RESTRICT rather than CASCADE: deleting an
    -- orientation must not silently delete the cohorts organised around it.
    orientation_id UUID REFERENCES orientations(id) ON DELETE RESTRICT,
    -- The creator is also seeded as the first organizer in cohort_members.
    -- SET NULL keeps the cohort alive if the account is deleted; the
    -- remaining organizers carry on.
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    is_public BOOLEAN NOT NULL DEFAULT TRUE,
    -- Set when an organizer archives the cohort early. Archived cohorts are
    -- readable but frozen: no joins, no messages, no milestone edits.
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT cohorts_window_ordered CHECK (ends_at > starts_at)
);

-- Discovery: public cohorts for an orientation, soonest first.
CREATE INDEX IF NOT EXISTS idx_cohorts_discovery
    ON cohorts (orientation_id, starts_at DESC)
    WHERE is_public = TRUE AND archived_at IS NULL;

CREATE TABLE IF NOT EXISTS cohort_members (
    cohort_id UUID NOT NULL REFERENCES cohorts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(10) NOT NULL DEFAULT 'member'
        CHECK (role IN ('member', 'organizer')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (cohort_id, user_id)
);

-- "Which cohorts am I in", for the profile and the for-you feed.
CREATE INDEX IF NOT EXISTS idx_cohort_members_by_user
    ON cohort_members (user_id, joined_at DESC);

-- Collective deliverables: what the group commits to, together.
CREATE TABLE IF NOT EXISTS cohort_milestones (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cohort_id UUID NOT NULL REFERENCES cohorts(id) ON DELETE CASCADE,
    title VARCHAR(200) NOT NULL CHECK (length(title) BETWEEN 3 AND 200),
    description TEXT NOT NULL DEFAULT '' CHECK (length(description) <= 4000),
    target_date DATE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cohort_milestones_by_cohort
    ON cohort_milestones (cohort_id, target_date ASC);

CREATE TABLE IF NOT EXISTS cohort_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cohort_id UUID NOT NULL REFERENCES cohorts(id) ON DELETE CASCADE,
    -- SET NULL rather than CASCADE: deleting an account must not punch
    -- holes in a conversation other members are still reading.
    sender_id UUID REFERENCES users(id) ON DELETE SET NULL,
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The only read path: one cohort's history, newest first.
CREATE INDEX IF NOT EXISTS idx_cohort_messages_by_cohort
    ON cohort_messages (cohort_id, created_at DESC);
