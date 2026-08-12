//! SKI-46 (Post-MVP T3-03) — reputation staking.
//!
//! See migration 0148 for the schema and for why the rank penalty is a
//! layer over the derived rank rather than a write into it.
//!
//! ## Guard rails
//!
//! The ticket flags abuse potential, so the limits are explicit and all in
//! one place:
//!
//!   * only Doyen may vouch — the top rank, which takes 50 verified
//!     deliverables, 5 attestations and the mentor capability to reach, so
//!     a sock-puppet voucher is not a realistic attack;
//!   * [`MAX_LIVE_VOUCHINGS`] caps how many people one senior can back at
//!     once, because a Doyen vouching for two hundred juniors is not
//!     staking anything meaningful on any of them;
//!   * a vouching always expires ([`MAX_WINDOW_DAYS`]), so nobody carries
//!     forgotten risk;
//!   * breaking one is a moderator action, never automatic.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ranks;

/// Minimum rank required to vouch.
pub const MIN_VOUCHER_RANK: &str = ranks::RANK_DOYEN;

/// How many people one voucher may back at once.
pub const MAX_LIVE_VOUCHINGS: i64 = 10;

/// Longest window a vouching may cover.
pub const MAX_WINDOW_DAYS: i64 = 365;
/// Shortest window worth recording.
pub const MIN_WINDOW_DAYS: i64 = 30;

/// How long the voucher's rank penalty lasts after a broken vouching.
pub const PENALTY_DAYS: i64 = 90;

// Compile-time coherence checks. Assertions rather than tests: these are
// constants, so a violation is a build error and never reaches production.
const _: () = assert!(MIN_WINDOW_DAYS > 0);
const _: () = assert!(MAX_WINDOW_DAYS >= MIN_WINDOW_DAYS);
// The penalty must bite, but it must also end.
const _: () = assert!(PENALTY_DAYS > 0 && PENALTY_DAYS <= MAX_WINDOW_DAYS);

pub const AT_STAKE_RANK: &str = "rank_temporary";
pub const AT_STAKE_REPUTATION: &str = "reputation_only";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Vouching {
    pub id: Uuid,
    pub voucher_id: Uuid,
    pub vouched_id: Uuid,
    pub active_until: chrono::DateTime<chrono::Utc>,
    pub at_stake_kind: String,
    pub statement: String,
    pub broken_at: Option<chrono::DateTime<chrono::Utc>>,
    pub break_reason: Option<String>,
    pub broken_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Vouching {
    /// Live means: not broken, and not expired.
    pub fn is_live(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.broken_at.is_none() && self.active_until > now
    }
}

/// Create a vouching.
pub async fn create(
    db: &PgPool,
    voucher_id: Uuid,
    vouched_id: Uuid,
    window_days: i64,
    at_stake_kind: &str,
    statement: &str,
) -> Result<Vouching, AppError> {
    if voucher_id == vouched_id {
        return Err(AppError::Validation("cannot vouch for yourself".into()));
    }
    if at_stake_kind != AT_STAKE_RANK && at_stake_kind != AT_STAKE_REPUTATION {
        return Err(AppError::Validation(format!(
            "at_stake_kind must be '{AT_STAKE_RANK}' or '{AT_STAKE_REPUTATION}'"
        )));
    }
    if !(MIN_WINDOW_DAYS..=MAX_WINDOW_DAYS).contains(&window_days) {
        return Err(AppError::Validation(format!(
            "window_days must be between {MIN_WINDOW_DAYS} and {MAX_WINDOW_DAYS}"
        )));
    }
    if statement.chars().count() > 1000 {
        return Err(AppError::Validation(
            "statement must be at most 1000 characters".into(),
        ));
    }

    // Effective rank, so a voucher already serving a penalty cannot keep
    // vouching — which would let one bad call cascade into more.
    let rank = ranks::effective_rank(db, voucher_id).await?;
    if rank != MIN_VOUCHER_RANK {
        return Err(AppError::Forbidden);
    }

    let target_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND is_banned = FALSE)",
    )
    .bind(vouched_id)
    .fetch_one(db)
    .await?;
    if !target_ok {
        return Err(AppError::NotFound(format!("user {vouched_id} not found")));
    }

    let live: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vouchings
          WHERE voucher_id = $1 AND broken_at IS NULL AND active_until > NOW()",
    )
    .bind(voucher_id)
    .fetch_one(db)
    .await?;
    if live >= MAX_LIVE_VOUCHINGS {
        return Err(AppError::Validation(format!(
            "at most {MAX_LIVE_VOUCHINGS} live vouchings — a vouching you cannot \
             stand behind is worth nothing"
        )));
    }

    let active_until = chrono::Utc::now() + chrono::Duration::days(window_days);
    let inserted: Result<Vouching, sqlx::Error> = sqlx::query_as(
        r#"
        INSERT INTO vouchings
            (voucher_id, vouched_id, active_until, at_stake_kind, statement)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(voucher_id)
    .bind(vouched_id)
    .bind(active_until)
    .bind(at_stake_kind)
    .bind(statement.trim())
    .fetch_one(db)
    .await;

    match inserted {
        Ok(v) => Ok(v),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(AppError::Conflict(
            "you already have a live vouching for this user".into(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Live vouchings backing a user — what a recruiter sees on the profile.
pub async fn list_for_vouched(db: &PgPool, vouched_id: Uuid) -> Result<Vec<Vouching>, AppError> {
    let rows: Vec<Vouching> = sqlx::query_as(
        "SELECT * FROM vouchings
          WHERE vouched_id = $1 AND broken_at IS NULL AND active_until > NOW()
          ORDER BY created_at DESC",
    )
    .bind(vouched_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Everything a voucher is currently backing, plus their history.
pub async fn list_for_voucher(db: &PgPool, voucher_id: Uuid) -> Result<Vec<Vouching>, AppError> {
    let rows: Vec<Vouching> =
        sqlx::query_as("SELECT * FROM vouchings WHERE voucher_id = $1 ORDER BY created_at DESC")
            .bind(voucher_id)
            .fetch_all(db)
            .await?;
    Ok(rows)
}

/// Count of live vouchings, used by talent search to boost a profile.
pub async fn live_count(db: &PgPool, vouched_id: Uuid) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vouchings
          WHERE vouched_id = $1 AND broken_at IS NULL AND active_until > NOW()",
    )
    .bind(vouched_id)
    .fetch_one(db)
    .await?;
    Ok(count)
}

/// Withdraw a vouching you made, before anything goes wrong.
///
/// Recorded as a break with no penalty: the voucher is choosing to stop
/// backing someone, which is the honest thing to do the moment they stop
/// believing it. Penalising that would push people to stay silent instead.
pub async fn withdraw(db: &PgPool, vouching_id: Uuid, voucher_id: Uuid) -> Result<(), AppError> {
    let affected = sqlx::query(
        "UPDATE vouchings
            SET broken_at = NOW(),
                break_reason = 'withdrawn by voucher',
                broken_by = $2
          WHERE id = $1 AND voucher_id = $2 AND broken_at IS NULL",
    )
    .bind(vouching_id)
    .bind(voucher_id)
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "live vouching {vouching_id} not found"
        )));
    }
    Ok(())
}

/// Report of a broken vouching.
#[derive(Debug, Clone, Serialize)]
pub struct BreakReport {
    pub vouching: Vouching,
    /// True when the voucher took a rank penalty.
    pub penalty_applied: bool,
    pub voucher_rank_before: String,
    pub voucher_rank_effective: String,
    pub penalty_until: Option<chrono::DateTime<chrono::Utc>>,
}

/// Break a vouching because the vouched user was caught in fraud.
///
/// Moderator action, never automatic: whether a fraud finding is severe
/// enough to burn someone else's rank is a judgement call, and the whole
/// mechanism depends on that call being made by a person.
///
/// Applies the penalty only for `at_stake_kind = 'rank_temporary'`. The
/// penalty is a window on `user_ranks`, not a rewrite of the rank — see
/// migration 0148.
pub async fn break_vouching(
    db: &PgPool,
    vouching_id: Uuid,
    moderator_id: Uuid,
    reason: &str,
) -> Result<BreakReport, AppError> {
    let reason = reason.trim();
    if reason.chars().count() < 8 {
        return Err(AppError::Validation(
            "break_reason must be at least 8 characters — this costs someone a rank".into(),
        ));
    }

    let mut tx = db.begin().await?;

    let vouching: Vouching = sqlx::query_as("SELECT * FROM vouchings WHERE id = $1 FOR UPDATE")
        .bind(vouching_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("vouching {vouching_id} not found")))?;

    if vouching.broken_at.is_some() {
        return Err(AppError::Conflict("vouching already broken".into()));
    }
    if vouching.active_until <= chrono::Utc::now() {
        return Err(AppError::Conflict(
            "vouching has expired — nothing is at stake anymore".into(),
        ));
    }

    let vouching: Vouching = sqlx::query_as(
        "UPDATE vouchings
            SET broken_at = NOW(), break_reason = $2, broken_by = $3
          WHERE id = $1
          RETURNING *",
    )
    .bind(vouching_id)
    .bind(reason)
    .bind(moderator_id)
    .fetch_one(&mut *tx)
    .await?;

    let rank_before: String = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(vouching.voucher_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_else(|| ranks::RANK_APPRENTI.to_string());

    let penalty_applied = vouching.at_stake_kind == AT_STAKE_RANK;
    let mut penalty_until = None;

    if penalty_applied {
        let until = chrono::Utc::now() + chrono::Duration::days(PENALTY_DAYS);
        penalty_until = Some(until);

        // Upsert: a voucher always has a user_ranks row in practice (they
        // are Doyen), but a missing row must not silently skip the penalty.
        sqlx::query(
            r#"
            INSERT INTO user_ranks (user_id, rank, penalty_until, penalty_source_vouching_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                penalty_until = EXCLUDED.penalty_until,
                penalty_source_vouching_id = EXCLUDED.penalty_source_vouching_id
            "#,
        )
        .bind(vouching.voucher_id)
        .bind(&rank_before)
        .bind(until)
        .bind(vouching_id)
        .execute(&mut *tx)
        .await?;

        // Governance journal, same table admin overrides write to, so
        // "why is this person's rank different from their proofs" has one
        // answer.
        sqlx::query(
            r#"
            INSERT INTO rank_overrides
                (user_id, admin_id, old_rank, new_rank, reason, source_vouching_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(vouching.voucher_id)
        .bind(moderator_id)
        .bind(&rank_before)
        .bind(demoted_label(&rank_before))
        .bind(format!("broken vouching: {reason}"))
        .bind(vouching_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let effective = ranks::effective_rank(db, vouching.voucher_id).await?;

    Ok(BreakReport {
        vouching,
        penalty_applied,
        voucher_rank_before: rank_before,
        voucher_rank_effective: effective,
        penalty_until,
    })
}

/// The rank label recorded in the governance journal as the post-penalty
/// value. Mirrors the one-step demotion `ranks::effective_rank` applies.
fn demoted_label(rank: &str) -> String {
    match ranks::rank_position(rank) {
        Some(0) | None => ranks::RANK_APPRENTI.to_string(),
        Some(i) => ranks::rank_order()[i - 1].to_string(),
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn vouching_with(broken: Option<chrono::DateTime<chrono::Utc>>, until_days: i64) -> Vouching {
        Vouching {
            id: Uuid::nil(),
            voucher_id: Uuid::nil(),
            vouched_id: Uuid::nil(),
            active_until: chrono::Utc::now() + chrono::Duration::days(until_days),
            at_stake_kind: AT_STAKE_RANK.into(),
            statement: String::new(),
            broken_at: broken,
            break_reason: None,
            broken_by: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn liveness_requires_unbroken_and_unexpired() {
        let now = chrono::Utc::now();
        assert!(vouching_with(None, 30).is_live(now));
        assert!(
            !vouching_with(None, -1).is_live(now),
            "an expired vouching stakes nothing"
        );
        assert!(!vouching_with(Some(now), 30).is_live(now));
    }

    #[test]
    fn demotion_label_matches_the_ladder() {
        assert_eq!(demoted_label(ranks::RANK_DOYEN), ranks::RANK_MAITRE);
        assert_eq!(demoted_label(ranks::RANK_RANGER), ranks::RANK_APPRENTI);
        assert_eq!(demoted_label(ranks::RANK_APPRENTI), ranks::RANK_APPRENTI);
        assert_eq!(demoted_label("legende"), ranks::RANK_APPRENTI);
    }

    #[test]
    fn only_the_top_rank_may_vouch() {
        assert_eq!(
            ranks::rank_position(MIN_VOUCHER_RANK),
            Some(ranks::rank_order().len() - 1),
            "vouching is restricted to the top of the ladder"
        );
    }
}
