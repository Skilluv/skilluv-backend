-- SKI-41 (Post-MVP T2-02) — structured peer-to-peer coaching.
--
-- Distinct from `mentorship_sessions`, which are paid, formal, and
-- senior->junior. A peer match is free, informal, and between equals: the
-- self-taught / career-change wedge needs classmates as much as it needs
-- mentors, and a mentor cannot supply the "someone else is stuck on this
-- too" that keeps people going.
--
-- ## Where the matching inputs live
--
-- Nothing here duplicates profile data. Timezone and working languages
-- already exist on `user_orientations` (migration 0089) and rank on
-- `user_ranks` (0092); the matcher joins them. This table stores only what
-- is genuinely new: the intent to be matched, and at what cadence.

CREATE TABLE IF NOT EXISTS peer_matching_enrollments (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Enrollment is per orientation: someone retraining into security and
    -- keeping a foot in frontend wants a different partner for each.
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE CASCADE,
    -- Sessions per week the user is signing up for. 1 is the ticket's
    -- default; the cap keeps "peer coaching" from turning into a job.
    weekly_cadence SMALLINT NOT NULL DEFAULT 1
        CHECK (weekly_cadence BETWEEN 1 AND 5),
    -- Soft toggle rather than a delete, so pausing and resuming does not
    -- lose the cadence the user picked.
    active BOOLEAN NOT NULL DEFAULT TRUE,
    enrolled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (user_id, orientation_id)
);

-- The matcher's scan: everyone still looking, for one orientation.
CREATE INDEX IF NOT EXISTS idx_peer_enrollments_pool
    ON peer_matching_enrollments (orientation_id)
    WHERE active = TRUE;

CREATE TABLE IF NOT EXISTS peer_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Ordered pair, same trick as dm_conversations: storing (a,b) with
    -- a < b makes "is there already a match between these two" a single
    -- unique-index lookup instead of two OR-ed comparisons, and makes the
    -- duplicate impossible rather than merely unlikely.
    user_a UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    orientation_id UUID NOT NULL REFERENCES orientations(id) ON DELETE CASCADE,
    weekly_cadence SMALLINT NOT NULL DEFAULT 1
        CHECK (weekly_cadence BETWEEN 1 AND 5),
    matched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    ended_at TIMESTAMPTZ,
    -- Snapshot of why the algorithm paired these two, kept for the "why am
    -- I seeing this person" affordance and for tuning the weights later.
    match_reason JSONB NOT NULL DEFAULT '{}'::JSONB,

    CONSTRAINT peer_matches_distinct CHECK (user_a <> user_b),
    CONSTRAINT peer_matches_ordered CHECK (user_a < user_b),
    CONSTRAINT peer_matches_ended_coherent
        CHECK ((active = TRUE AND ended_at IS NULL) OR (active = FALSE AND ended_at IS NOT NULL))
);

-- At most one live match per pair per orientation. Partial, so the same
-- two people can be re-matched after ending a previous run.
CREATE UNIQUE INDEX IF NOT EXISTS idx_peer_matches_unique_active
    ON peer_matches (user_a, user_b, orientation_id)
    WHERE active = TRUE;

CREATE INDEX IF NOT EXISTS idx_peer_matches_by_user_a
    ON peer_matches (user_a, matched_at DESC);
CREATE INDEX IF NOT EXISTS idx_peer_matches_by_user_b
    ON peer_matches (user_b, matched_at DESC);

CREATE TABLE IF NOT EXISTS peer_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES peer_matches(id) ON DELETE CASCADE,
    session_at TIMESTAMPTZ NOT NULL,
    -- Per-side check-in. Each participant writes their own column and can
    -- never write the other's — enforced in the service layer, which maps
    -- the caller onto side A or B.
    notes_a TEXT CHECK (notes_a IS NULL OR length(notes_a) <= 4000),
    notes_b TEXT CHECK (notes_b IS NULL OR length(notes_b) <= 4000),
    rating_a SMALLINT CHECK (rating_a IS NULL OR rating_a BETWEEN 1 AND 5),
    rating_b SMALLINT CHECK (rating_b IS NULL OR rating_b BETWEEN 1 AND 5),
    canceled BOOLEAN NOT NULL DEFAULT FALSE,
    canceled_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_peer_sessions_by_match
    ON peer_sessions (match_id, session_at DESC);
