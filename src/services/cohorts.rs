//! SKI-40 (Post-MVP T2-01) — time-boxed study cohorts.
//!
//! See migration 0143 for why a cohort is neither a team nor a guild, and
//! why the group chat is its own table rather than a widened DM.
//!
//! This module owns the rules that a CHECK constraint cannot express:
//!
//!   * capacity — a join must not exceed `max_members`, and the check has
//!     to be race-free (two simultaneous joins on the last seat);
//!   * lifecycle — an archived or finished cohort accepts no new joins,
//!     messages or milestone edits;
//!   * organizer continuity — the last organizer cannot walk out and leave
//!     a cohort nobody can administer.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const ROLE_MEMBER: &str = "member";
pub const ROLE_ORGANIZER: &str = "organizer";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Cohort {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub max_members: i32,
    pub orientation_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub is_public: bool,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CohortMember {
    pub cohort_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CohortMilestone {
    pub id: Uuid,
    pub cohort_id: Uuid,
    pub title: String,
    pub description: String,
    pub target_date: chrono::NaiveDate,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CohortMessage {
    pub id: Uuid,
    pub cohort_id: Uuid,
    pub sender_id: Option<Uuid>,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The WebSocket room a cohort's chat broadcasts on.
///
/// Namespaced so it cannot collide with any other room key.
pub fn room_key(cohort_id: Uuid) -> String {
    format!("cohort:{cohort_id}")
}

/// Load a cohort, or 404.
pub async fn get(db: &PgPool, cohort_id: Uuid) -> Result<Cohort, AppError> {
    sqlx::query_as("SELECT * FROM cohorts WHERE id = $1")
        .bind(cohort_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cohort {cohort_id} not found")))
}

/// A member's role, or `None` if they are not in the cohort.
pub async fn role_of(
    db: &PgPool,
    cohort_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM cohort_members WHERE cohort_id = $1 AND user_id = $2")
            .bind(cohort_id)
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    Ok(role)
}

/// Assert the caller may read this cohort.
///
/// Public cohorts are readable by anyone; private ones only by members.
/// Answers `NotFound` rather than `Forbidden` for a private cohort so the
/// endpoint cannot be used to confirm that a given id exists.
pub async fn assert_readable(
    db: &PgPool,
    cohort_id: Uuid,
    viewer: Option<Uuid>,
) -> Result<Cohort, AppError> {
    let cohort = get(db, cohort_id).await?;
    if cohort.is_public {
        return Ok(cohort);
    }
    let is_member = match viewer {
        Some(v) => role_of(db, cohort_id, v).await?.is_some(),
        None => false,
    };
    if is_member {
        Ok(cohort)
    } else {
        Err(AppError::NotFound(format!("cohort {cohort_id} not found")))
    }
}

/// Assert the caller is an organizer of this cohort.
pub async fn assert_organizer(
    db: &PgPool,
    cohort_id: Uuid,
    user_id: Uuid,
) -> Result<Cohort, AppError> {
    let cohort = get(db, cohort_id).await?;
    match role_of(db, cohort_id, user_id).await?.as_deref() {
        Some(ROLE_ORGANIZER) => Ok(cohort),
        _ => Err(AppError::Forbidden),
    }
}

/// Reject writes to a cohort that is archived or already over.
///
/// A finished cohort stays fully readable — the archive is the point —
/// but accepting new messages into a cycle that ended would blur the
/// boundary that makes a cohort different from a guild.
fn assert_writable(cohort: &Cohort) -> Result<(), AppError> {
    if cohort.archived_at.is_some() {
        return Err(AppError::Conflict("cohort is archived".into()));
    }
    if cohort.ends_at <= chrono::Utc::now() {
        return Err(AppError::Conflict("cohort has ended".into()));
    }
    Ok(())
}

pub struct CreateCohortParams<'a> {
    pub slug: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub max_members: i32,
    pub orientation_id: Option<Uuid>,
    pub is_public: bool,
}

/// Create a cohort and seat its creator as the first organizer.
///
/// Both writes share a transaction: a cohort with no organizer would be
/// unadministrable from the moment it existed.
pub async fn create(
    db: &PgPool,
    creator: Uuid,
    params: CreateCohortParams<'_>,
) -> Result<Cohort, AppError> {
    if let Some(oid) = params.orientation_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM orientations WHERE id = $1 AND is_archived = FALSE)",
        )
        .bind(oid)
        .fetch_one(db)
        .await?;
        if !exists {
            return Err(AppError::NotFound(format!("orientation {oid} not found")));
        }
    }

    let mut tx = db.begin().await?;

    let cohort: Cohort = match sqlx::query_as(
        r#"
        INSERT INTO cohorts
            (slug, name, description, starts_at, ends_at, max_members,
             orientation_id, created_by, is_public)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
    .bind(params.slug)
    .bind(params.name)
    .bind(params.description)
    .bind(params.starts_at)
    .bind(params.ends_at)
    .bind(params.max_members)
    .bind(params.orientation_id)
    .bind(creator)
    .bind(params.is_public)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(c) => c,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(AppError::Conflict(format!(
                "cohort slug '{}' is already taken",
                params.slug
            )));
        }
        Err(e) => return Err(e.into()),
    };

    sqlx::query(
        "INSERT INTO cohort_members (cohort_id, user_id, role)
         VALUES ($1, $2, 'organizer')",
    )
    .bind(cohort.id)
    .bind(creator)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(cohort)
}

/// Join a cohort, respecting capacity.
///
/// The capacity check runs inside a transaction that first takes a row
/// lock on the cohort (`FOR UPDATE`). Without it, two members racing for
/// the last seat would both read `count < max` and both insert. Postgres
/// cannot express "at most N rows referencing this parent" as a
/// constraint, so the lock is the enforcement.
pub async fn join(db: &PgPool, cohort_id: Uuid, user_id: Uuid) -> Result<CohortMember, AppError> {
    let mut tx = db.begin().await?;

    let cohort: Cohort = sqlx::query_as("SELECT * FROM cohorts WHERE id = $1 FOR UPDATE")
        .bind(cohort_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cohort {cohort_id} not found")))?;

    assert_writable(&cohort)?;
    if !cohort.is_public {
        // Private cohorts are invite-only; an organizer adds members via
        // `add_member`. Reported as 404 to match `assert_readable`.
        return Err(AppError::NotFound(format!("cohort {cohort_id} not found")));
    }

    let already: Option<String> =
        sqlx::query_scalar("SELECT role FROM cohort_members WHERE cohort_id = $1 AND user_id = $2")
            .bind(cohort_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    if already.is_some() {
        return Err(AppError::Conflict("already a member of this cohort".into()));
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cohort_members WHERE cohort_id = $1")
        .bind(cohort_id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= cohort.max_members as i64 {
        return Err(AppError::Conflict("cohort is full".into()));
    }

    let member: CohortMember = sqlx::query_as(
        "INSERT INTO cohort_members (cohort_id, user_id, role)
         VALUES ($1, $2, 'member')
         RETURNING *",
    )
    .bind(cohort_id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(member)
}

/// Remove a member.
///
/// Refuses to remove the last organizer: a cohort with members but no
/// organizer could never be archived, edited, or given new milestones.
pub async fn leave(db: &PgPool, cohort_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let mut tx = db.begin().await?;

    // Lock the parent so a concurrent leave cannot also believe it is not
    // the last organizer.
    sqlx::query("SELECT id FROM cohorts WHERE id = $1 FOR UPDATE")
        .bind(cohort_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cohort {cohort_id} not found")))?;

    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM cohort_members WHERE cohort_id = $1 AND user_id = $2")
            .bind(cohort_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(role) = role else {
        return Err(AppError::NotFound("not a member of this cohort".into()));
    };

    if role == ROLE_ORGANIZER {
        let organizers: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cohort_members
              WHERE cohort_id = $1 AND role = 'organizer'",
        )
        .bind(cohort_id)
        .fetch_one(&mut *tx)
        .await?;
        if organizers <= 1 {
            return Err(AppError::Conflict(
                "promote another organizer before leaving — a cohort cannot be left \
                 without one"
                    .into(),
            ));
        }
    }

    sqlx::query("DELETE FROM cohort_members WHERE cohort_id = $1 AND user_id = $2")
        .bind(cohort_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Add a member directly (organizer action, and the only way into a
/// private cohort).
pub async fn add_member(
    db: &PgPool,
    cohort_id: Uuid,
    target: Uuid,
    role: &str,
) -> Result<CohortMember, AppError> {
    if role != ROLE_MEMBER && role != ROLE_ORGANIZER {
        return Err(AppError::Validation(format!(
            "role must be '{ROLE_MEMBER}' or '{ROLE_ORGANIZER}'"
        )));
    }

    let mut tx = db.begin().await?;
    let cohort: Cohort = sqlx::query_as("SELECT * FROM cohorts WHERE id = $1 FOR UPDATE")
        .bind(cohort_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("cohort {cohort_id} not found")))?;
    assert_writable(&cohort)?;

    let user_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
    if !user_exists {
        return Err(AppError::NotFound(format!("user {target} not found")));
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cohort_members
          WHERE cohort_id = $1 AND user_id <> $2",
    )
    .bind(cohort_id)
    .bind(target)
    .fetch_one(&mut *tx)
    .await?;
    if count >= cohort.max_members as i64 {
        return Err(AppError::Conflict("cohort is full".into()));
    }

    // Upsert so promoting an existing member to organizer uses this path
    // too, instead of needing a separate endpoint.
    let member: CohortMember = sqlx::query_as(
        "INSERT INTO cohort_members (cohort_id, user_id, role)
         VALUES ($1, $2, $3)
         ON CONFLICT (cohort_id, user_id) DO UPDATE SET role = EXCLUDED.role
         RETURNING *",
    )
    .bind(cohort_id)
    .bind(target)
    .bind(role)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(member)
}

/// Post a message to the cohort chat. Members only.
pub async fn post_message(
    db: &PgPool,
    cohort_id: Uuid,
    sender: Uuid,
    body: &str,
) -> Result<CohortMessage, AppError> {
    let trimmed = body.trim();
    let len = trimmed.chars().count();
    if !(1..=4000).contains(&len) {
        return Err(AppError::Validation(
            "body must be 1..4000 characters after trim".into(),
        ));
    }

    let cohort = get(db, cohort_id).await?;
    assert_writable(&cohort)?;
    if role_of(db, cohort_id, sender).await?.is_none() {
        return Err(AppError::Forbidden);
    }

    let message: CohortMessage = sqlx::query_as(
        "INSERT INTO cohort_messages (cohort_id, sender_id, body)
         VALUES ($1, $2, $3)
         RETURNING *",
    )
    .bind(cohort_id)
    .bind(sender)
    .bind(trimmed)
    .fetch_one(db)
    .await?;

    Ok(message)
}

/// Read the cohort chat. Members only, even for a public cohort: the
/// cohort's existence is public, its conversation is not.
pub async fn list_messages(
    db: &PgPool,
    cohort_id: Uuid,
    viewer: Uuid,
    limit: i64,
    before: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<CohortMessage>, AppError> {
    if role_of(db, cohort_id, viewer).await?.is_none() {
        return Err(AppError::Forbidden);
    }
    let messages: Vec<CohortMessage> = sqlx::query_as(
        r#"
        SELECT * FROM cohort_messages
         WHERE cohort_id = $1
           AND ($2::TIMESTAMPTZ IS NULL OR created_at < $2)
         ORDER BY created_at DESC
         LIMIT $3
        "#,
    )
    .bind(cohort_id)
    .bind(before)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(messages)
}

#[cfg(test)]
mod unit {
    use super::*;

    fn cohort_with(archived: Option<chrono::DateTime<chrono::Utc>>, ends_in_days: i64) -> Cohort {
        let now = chrono::Utc::now();
        Cohort {
            id: Uuid::nil(),
            slug: "test".into(),
            name: "Test".into(),
            description: String::new(),
            starts_at: now - chrono::Duration::days(1),
            ends_at: now + chrono::Duration::days(ends_in_days),
            max_members: 10,
            orientation_id: None,
            created_by: None,
            is_public: true,
            archived_at: archived,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn writable_only_while_live() {
        assert!(assert_writable(&cohort_with(None, 10)).is_ok());
        assert!(
            assert_writable(&cohort_with(None, -1)).is_err(),
            "a cohort past its end date is frozen"
        );
        assert!(
            assert_writable(&cohort_with(Some(chrono::Utc::now()), 10)).is_err(),
            "an archived cohort is frozen even before its end date"
        );
    }

    #[test]
    fn room_keys_are_namespaced_and_unique() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert!(room_key(a).starts_with("cohort:"));
        assert_ne!(room_key(a), room_key(b));
    }
}
