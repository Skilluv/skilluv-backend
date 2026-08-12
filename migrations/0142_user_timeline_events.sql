-- SKI-39 (Post-MVP T1-04) — chronological profile timeline.
--
-- "2026-03 joined -> 2026-06 first verified deliverable -> 2026-09 Ranger
--  -> 2027-02 first attestation -> 2027-08 Artisan".
--
-- ## Why a real table and not a materialized view
--
-- The ticket floated a materialized view. A MV cannot be "populated by
-- INSERT from the P19 hooks" — it is recomputed wholesale, which means
-- either a refresh on every proof event (a full rescan per verified
-- deliverable) or a stale timeline. A table fed incrementally is O(1) per
-- event and always current.
--
-- ## Why database triggers and not Rust hooks
--
-- The P19 hooks cover the paths that go through `proof_hooks`, but ranks,
-- capabilities, attestations and orientations are also written by admin
-- endpoints, backfills and migrations. A trigger sees every writer, so the
-- timeline cannot silently miss an event because a new code path forgot to
-- call it. It also makes the backfill below a straight replay of history
-- rather than a second implementation of the same rules.
--
-- ## Idempotency
--
-- `(user_id, event_type, dedup_key)` is UNIQUE and every insert is
-- ON CONFLICT DO NOTHING, so triggers and the backfill converge on the
-- same rows no matter how many times either runs. `dedup_key` is the
-- natural identity of the event: the deliverable id, the rank slug, the
-- capability slug, or the literal 'first' for once-in-a-lifetime events.

CREATE TABLE IF NOT EXISTS user_timeline_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type VARCHAR(40) NOT NULL
        CHECK (event_type IN (
            'signup',
            'orientation_added',
            'deliverable_verified',
            'rank_promoted',
            'capability_granted',
            'attestation_received',
            'event_participation',
            'first_bounty_earned',
            'first_mentor_session'
        )),
    -- When the event actually happened, not when the row was written. The
    -- backfill inserts historical rows, so NOW() would be wrong.
    event_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    -- Natural identity of the event within its type. Never NULL: NULLs do
    -- not collide in a unique index, which would defeat the whole point.
    dedup_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT user_timeline_events_unique
        UNIQUE (user_id, event_type, dedup_key)
);

-- The one read path: a user's timeline, newest first, paginated.
CREATE INDEX IF NOT EXISTS idx_user_timeline_by_user
    ON user_timeline_events (user_id, event_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Recording helper
-- ═══════════════════════════════════════════════════════════════════

-- Single insertion point for every trigger below. Keeping the ON CONFLICT
-- in one place means a new event type cannot accidentally ship without
-- idempotency.
CREATE OR REPLACE FUNCTION timeline_record(
    p_user_id UUID,
    p_event_type VARCHAR,
    p_event_at TIMESTAMPTZ,
    p_dedup_key TEXT,
    p_metadata JSONB
) RETURNS VOID AS $$
BEGIN
    IF p_user_id IS NULL OR p_event_at IS NULL THEN
        RETURN;
    END IF;
    INSERT INTO user_timeline_events
        (user_id, event_type, event_at, dedup_key, metadata)
    VALUES
        (p_user_id, p_event_type, p_event_at, p_dedup_key,
         COALESCE(p_metadata, '{}'::JSONB))
    ON CONFLICT (user_id, event_type, dedup_key) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

-- ═══════════════════════════════════════════════════════════════════
-- Triggers
-- ═══════════════════════════════════════════════════════════════════

-- signup
CREATE OR REPLACE FUNCTION trg_timeline_signup() RETURNS TRIGGER AS $$
BEGIN
    PERFORM timeline_record(NEW.id, 'signup', NEW.created_at, 'signup', '{}'::JSONB);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_signup ON users;
CREATE TRIGGER timeline_signup
    AFTER INSERT ON users
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_signup();

-- orientation_added
CREATE OR REPLACE FUNCTION trg_timeline_orientation() RETURNS TRIGGER AS $$
DECLARE
    v_slug TEXT;
BEGIN
    SELECT slug INTO v_slug FROM orientations WHERE id = NEW.orientation_id;
    PERFORM timeline_record(
        NEW.user_id, 'orientation_added', NEW.started_at,
        NEW.orientation_id::TEXT,
        jsonb_build_object('orientation_slug', v_slug, 'mode', NEW.mode)
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_orientation ON user_orientations;
CREATE TRIGGER timeline_orientation
    AFTER INSERT ON user_orientations
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_orientation();

-- deliverable_verified and first_bounty_earned. Both fire on the same row
-- transition — INTO 'verified' — so they share one trigger rather than
-- doubling the write path. Firing on the transition only means later edits
-- to an already-verified deliverable do not re-stamp the timeline.
--
-- On first_bounty_earned: standalone bounties were folded into
-- `project_slices` by migration 0074, so "earned a bounty" is now "had a
-- deliverable verified that paid credits". dedup_key is the literal
-- 'first', so the UNIQUE constraint keeps only the earliest paid
-- deliverable whatever order rows arrive in.
CREATE OR REPLACE FUNCTION trg_timeline_deliverable() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.verification_status = 'verified'
       AND (TG_OP = 'INSERT' OR OLD.verification_status IS DISTINCT FROM 'verified')
    THEN
        PERFORM timeline_record(
            NEW.user_id, 'deliverable_verified',
            COALESCE(NEW.verified_at, NOW()),
            NEW.id::TEXT,
            jsonb_build_object('artifact_type', NEW.artifact_type)
        );

        IF COALESCE(NEW.credits_awarded, 0) > 0 THEN
            PERFORM timeline_record(
                NEW.user_id, 'first_bounty_earned',
                COALESCE(NEW.verified_at, NOW()),
                'first',
                jsonb_build_object(
                    'deliverable_id', NEW.id,
                    'credits_awarded', NEW.credits_awarded
                )
            );
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_deliverable ON deliverables;
CREATE TRIGGER timeline_deliverable
    AFTER INSERT OR UPDATE OF verification_status ON deliverables
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_deliverable();

-- rank_promoted. Sourced from user_rank_history rather than user_ranks:
-- history is append-only and already records from/to, whereas user_ranks
-- is an upsert target whose OLD row is gone after the fact.
CREATE OR REPLACE FUNCTION trg_timeline_rank() RETURNS TRIGGER AS $$
BEGIN
    PERFORM timeline_record(
        NEW.user_id, 'rank_promoted',
        COALESCE(NEW.achieved_at, NOW()),
        NEW.to_rank,
        jsonb_build_object('from_rank', NEW.from_rank, 'to_rank', NEW.to_rank)
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_rank ON user_rank_history;
CREATE TRIGGER timeline_rank
    AFTER INSERT ON user_rank_history
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_rank();

-- capability_granted
CREATE OR REPLACE FUNCTION trg_timeline_capability() RETURNS TRIGGER AS $$
BEGIN
    PERFORM timeline_record(
        NEW.user_id, 'capability_granted', NEW.granted_at,
        NEW.capability,
        jsonb_build_object('capability', NEW.capability, 'reason', NEW.granted_reason)
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_capability ON user_capabilities;
CREATE TRIGGER timeline_capability
    AFTER INSERT ON user_capabilities
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_capability();

-- attestation_received
CREATE OR REPLACE FUNCTION trg_timeline_attestation() RETURNS TRIGGER AS $$
BEGIN
    PERFORM timeline_record(
        NEW.user_id, 'attestation_received', NEW.issued_at,
        NEW.id::TEXT,
        jsonb_build_object('title', NEW.title, 'attestation_type', NEW.attestation_type)
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_attestation ON attestations;
CREATE TRIGGER timeline_attestation
    AFTER INSERT ON attestations
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_attestation();

-- event_participation
CREATE OR REPLACE FUNCTION trg_timeline_event_participation() RETURNS TRIGGER AS $$
DECLARE
    v_title TEXT;
BEGIN
    SELECT name INTO v_title FROM events WHERE id = NEW.event_id;
    PERFORM timeline_record(
        NEW.user_id, 'event_participation', NEW.joined_at,
        NEW.event_id::TEXT,
        jsonb_build_object('event_title', v_title)
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_event_participation ON user_event_participation;
CREATE TRIGGER timeline_event_participation
    AFTER INSERT ON user_event_participation
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_event_participation();

-- first_mentor_session. Recorded for BOTH participants: "I was mentored"
-- and "I started mentoring" are each a milestone on their own timeline.
CREATE OR REPLACE FUNCTION trg_timeline_mentor_session() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'completed'
       AND (TG_OP = 'INSERT' OR OLD.status IS DISTINCT FROM 'completed')
    THEN
        PERFORM timeline_record(
            NEW.mentee_user_id, 'first_mentor_session', NEW.scheduled_at, 'first',
            jsonb_build_object('role', 'mentee', 'session_id', NEW.id)
        );
        PERFORM timeline_record(
            NEW.mentor_user_id, 'first_mentor_session', NEW.scheduled_at, 'first',
            jsonb_build_object('role', 'mentor', 'session_id', NEW.id)
        );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS timeline_mentor_session ON mentorship_sessions;
CREATE TRIGGER timeline_mentor_session
    AFTER INSERT OR UPDATE OF status ON mentorship_sessions
    FOR EACH ROW EXECUTE FUNCTION trg_timeline_mentor_session();

-- ═══════════════════════════════════════════════════════════════════
-- Backfill of everything that happened before this migration
-- ═══════════════════════════════════════════════════════════════════
--
-- Same ON CONFLICT DO NOTHING path as the triggers, so running this twice
-- (or running the Rust backfill binary afterwards) is a no-op.

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT id, 'signup', created_at, 'signup', '{}'::JSONB
  FROM users
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT uo.user_id, 'orientation_added', uo.started_at, uo.orientation_id::TEXT,
       jsonb_build_object('orientation_slug', o.slug, 'mode', uo.mode)
  FROM user_orientations uo
  JOIN orientations o ON o.id = uo.orientation_id
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT user_id, 'deliverable_verified', COALESCE(verified_at, submitted_at), id::TEXT,
       jsonb_build_object('artifact_type', artifact_type)
  FROM deliverables
 WHERE verification_status = 'verified'
   AND COALESCE(verified_at, submitted_at) IS NOT NULL
ON CONFLICT DO NOTHING;

-- DISTINCT ON keeps the earliest promotion per (user, rank): a user who
-- was demoted by an admin override and re-promoted has one history row per
-- promotion but only one timeline entry per rank reached.
INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT DISTINCT ON (user_id, to_rank)
       user_id, 'rank_promoted', achieved_at, to_rank,
       jsonb_build_object('from_rank', from_rank, 'to_rank', to_rank)
  FROM user_rank_history
 ORDER BY user_id, to_rank, achieved_at ASC
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT DISTINCT ON (user_id, capability)
       user_id, 'capability_granted', granted_at, capability,
       jsonb_build_object('capability', capability, 'reason', granted_reason)
  FROM user_capabilities
 ORDER BY user_id, capability, granted_at ASC
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT user_id, 'attestation_received', issued_at, id::TEXT,
       jsonb_build_object('title', title, 'attestation_type', attestation_type)
  FROM attestations
 WHERE revoked_at IS NULL
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT uep.user_id, 'event_participation', uep.joined_at, uep.event_id::TEXT,
       jsonb_build_object('event_title', e.name)
  FROM user_event_participation uep
  JOIN events e ON e.id = uep.event_id
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT DISTINCT ON (user_id)
       user_id, 'first_bounty_earned', COALESCE(verified_at, submitted_at), 'first',
       jsonb_build_object('deliverable_id', id, 'credits_awarded', credits_awarded)
  FROM deliverables
 WHERE verification_status = 'verified'
   AND credits_awarded > 0
   AND COALESCE(verified_at, submitted_at) IS NOT NULL
 ORDER BY user_id, COALESCE(verified_at, submitted_at) ASC
ON CONFLICT DO NOTHING;

INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
SELECT DISTINCT ON (user_id) user_id, 'first_mentor_session', scheduled_at, 'first', metadata
  FROM (
      SELECT mentee_user_id AS user_id, scheduled_at,
             jsonb_build_object('role', 'mentee', 'session_id', id) AS metadata
        FROM mentorship_sessions WHERE status = 'completed'
      UNION ALL
      SELECT mentor_user_id, scheduled_at,
             jsonb_build_object('role', 'mentor', 'session_id', id)
        FROM mentorship_sessions WHERE status = 'completed'
  ) s
 ORDER BY user_id, scheduled_at ASC
ON CONFLICT DO NOTHING;
