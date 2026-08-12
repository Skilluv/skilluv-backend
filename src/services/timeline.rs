//! SKI-39 (Post-MVP T1-04) — profile timeline.
//!
//! Writing is handled entirely by the database triggers installed in
//! migration 0142 — see that file for why triggers rather than Rust hooks.
//! This module owns the two things SQL triggers cannot do on their own:
//!
//!   * [`list_for_user`] — the paginated read path, with the visibility
//!     rule applied.
//!   * [`backfill`] — a replayable rebuild. Migration 0142 backfills once
//!     at deploy time, but a migration cannot be re-run; this can, which
//!     matters if triggers are ever dropped during maintenance or a bulk
//!     import bypasses them (`COPY` fires row triggers, but a restore with
//!     `--disable-triggers` does not).
//!
//! Every insert path is `ON CONFLICT DO NOTHING` against the
//! `(user_id, event_type, dedup_key)` unique constraint, so backfilling is
//! idempotent and safe to run against a live database.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Every event type the timeline can hold. Mirrors the CHECK in 0142.
pub const EVENT_TYPES: &[&str] = &[
    "signup",
    "orientation_added",
    "deliverable_verified",
    "rank_promoted",
    "capability_granted",
    "attestation_received",
    "event_participation",
    "first_bounty_earned",
    "first_mentor_session",
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TimelineEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub event_type: String,
    pub event_at: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
    pub dedup_key: String,
}

/// Reject an unknown event type before it reaches SQL.
pub fn validate_event_type(event_type: &str) -> Result<(), AppError> {
    if EVENT_TYPES.contains(&event_type) {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "event_type must be one of: {}",
        EVENT_TYPES.join(", ")
    )))
}

/// One page of a user's timeline, oldest-last.
///
/// Ordered `event_at DESC, id DESC`: the id tiebreak keeps pagination
/// stable when several events share a timestamp, which happens routinely
/// because a rank promotion and the deliverable that triggered it land in
/// the same transaction.
pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
    event_type: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<TimelineEvent>, AppError> {
    if let Some(t) = event_type {
        validate_event_type(t)?;
    }
    let rows: Vec<TimelineEvent> = sqlx::query_as(
        r#"
        SELECT id, user_id, event_type, event_at, metadata, dedup_key
          FROM user_timeline_events
         WHERE user_id = $1
           AND ($2::TEXT IS NULL OR event_type = $2)
         ORDER BY event_at DESC, id DESC
         LIMIT $3 OFFSET $4
        "#,
    )
    .bind(user_id)
    .bind(event_type)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Total event count, for pagination metadata.
pub async fn count_for_user(
    db: &PgPool,
    user_id: Uuid,
    event_type: Option<&str>,
) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_timeline_events
          WHERE user_id = $1 AND ($2::TEXT IS NULL OR event_type = $2)",
    )
    .bind(user_id)
    .bind(event_type)
    .fetch_one(db)
    .await?;
    Ok(count)
}

/// Rows inserted per event type during a [`backfill`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct BackfillReport {
    pub signup: u64,
    pub orientation_added: u64,
    pub deliverable_verified: u64,
    pub rank_promoted: u64,
    pub capability_granted: u64,
    pub attestation_received: u64,
    pub event_participation: u64,
    pub first_bounty_earned: u64,
    pub first_mentor_session: u64,
}

impl BackfillReport {
    pub fn total(&self) -> u64 {
        self.signup
            + self.orientation_added
            + self.deliverable_verified
            + self.rank_promoted
            + self.capability_granted
            + self.attestation_received
            + self.event_participation
            + self.first_bounty_earned
            + self.first_mentor_session
    }
}

/// Rebuild the timeline from the source tables.
///
/// `only_user` scopes the rebuild to a single profile (used by the admin
/// endpoint); `None` rebuilds everyone (used by the CLI).
///
/// Counts reflect rows actually inserted — a second run over unchanged
/// data reports all zeros, which is the cheapest possible confirmation
/// that the timeline is already complete.
pub async fn backfill(db: &PgPool, only_user: Option<Uuid>) -> Result<BackfillReport, AppError> {
    // Each statement takes the same `$1` user filter: NULL means "all
    // users", which keeps one query text for both call sites.
    let signup = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT id, 'signup', created_at, 'signup', '{}'::JSONB
          FROM users
         WHERE ($1::UUID IS NULL OR id = $1)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    let orientation_added = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT uo.user_id, 'orientation_added', uo.started_at, uo.orientation_id::TEXT,
               jsonb_build_object('orientation_slug', o.slug, 'mode', uo.mode)
          FROM user_orientations uo
          JOIN orientations o ON o.id = uo.orientation_id
         WHERE ($1::UUID IS NULL OR uo.user_id = $1)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    let deliverable_verified = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT user_id, 'deliverable_verified', COALESCE(verified_at, submitted_at), id::TEXT,
               jsonb_build_object('artifact_type', artifact_type)
          FROM deliverables
         WHERE verification_status = 'verified'
           AND COALESCE(verified_at, submitted_at) IS NOT NULL
           AND ($1::UUID IS NULL OR user_id = $1)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    // Earliest promotion per (user, rank): an admin demotion followed by a
    // re-promotion must not produce two "reached Ranger" entries.
    let rank_promoted = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT DISTINCT ON (user_id, to_rank)
               user_id, 'rank_promoted', achieved_at, to_rank,
               jsonb_build_object('from_rank', from_rank, 'to_rank', to_rank)
          FROM user_rank_history
         WHERE ($1::UUID IS NULL OR user_id = $1)
         ORDER BY user_id, to_rank, achieved_at ASC
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    let capability_granted = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT DISTINCT ON (user_id, capability)
               user_id, 'capability_granted', granted_at, capability,
               jsonb_build_object('capability', capability, 'reason', granted_reason)
          FROM user_capabilities
         WHERE ($1::UUID IS NULL OR user_id = $1)
         ORDER BY user_id, capability, granted_at ASC
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    let attestation_received = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT user_id, 'attestation_received', issued_at, id::TEXT,
               jsonb_build_object('title', title, 'attestation_type', attestation_type)
          FROM attestations
         WHERE revoked_at IS NULL
           AND ($1::UUID IS NULL OR user_id = $1)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    let event_participation = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT uep.user_id, 'event_participation', uep.joined_at, uep.event_id::TEXT,
               jsonb_build_object('event_title', e.name)
          FROM user_event_participation uep
          JOIN events e ON e.id = uep.event_id
         WHERE ($1::UUID IS NULL OR uep.user_id = $1)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    let first_bounty_earned = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT DISTINCT ON (user_id)
               user_id, 'first_bounty_earned', COALESCE(verified_at, submitted_at), 'first',
               jsonb_build_object('deliverable_id', id, 'credits_awarded', credits_awarded)
          FROM deliverables
         WHERE verification_status = 'verified'
           AND credits_awarded > 0
           AND COALESCE(verified_at, submitted_at) IS NOT NULL
           AND ($1::UUID IS NULL OR user_id = $1)
         ORDER BY user_id, COALESCE(verified_at, submitted_at) ASC
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    // Both sides of a session get their own milestone.
    let first_mentor_session = sqlx::query(
        r#"
        INSERT INTO user_timeline_events (user_id, event_type, event_at, dedup_key, metadata)
        SELECT DISTINCT ON (user_id)
               user_id, 'first_mentor_session', scheduled_at, 'first', metadata
          FROM (
              SELECT mentee_user_id AS user_id, scheduled_at,
                     jsonb_build_object('role', 'mentee', 'session_id', id) AS metadata
                FROM mentorship_sessions WHERE status = 'completed'
              UNION ALL
              SELECT mentor_user_id, scheduled_at,
                     jsonb_build_object('role', 'mentor', 'session_id', id)
                FROM mentorship_sessions WHERE status = 'completed'
          ) s
         WHERE ($1::UUID IS NULL OR user_id = $1)
         ORDER BY user_id, scheduled_at ASC
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(only_user)
    .execute(db)
    .await?
    .rows_affected();

    Ok(BackfillReport {
        signup,
        orientation_added,
        deliverable_verified,
        rank_promoted,
        capability_granted,
        attestation_received,
        event_participation,
        first_bounty_earned,
        first_mentor_session,
    })
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn event_type_allowlist_is_enforced() {
        for t in EVENT_TYPES {
            assert!(validate_event_type(t).is_ok(), "{t} should be accepted");
        }
        assert!(validate_event_type("promoted").is_err());
        assert!(validate_event_type("").is_err());
    }

    #[test]
    fn backfill_report_total_sums_every_field() {
        let r = BackfillReport {
            signup: 1,
            orientation_added: 2,
            deliverable_verified: 3,
            rank_promoted: 4,
            capability_granted: 5,
            attestation_received: 6,
            event_participation: 7,
            first_bounty_earned: 8,
            first_mentor_session: 9,
        };
        assert_eq!(r.total(), 45);
        assert_eq!(BackfillReport::default().total(), 0);
    }
}
