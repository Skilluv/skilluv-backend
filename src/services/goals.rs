//! SKI-38 (Post-MVP T1-03) — measurable personal goals.
//!
//! A goal stores only a target. Progress is derived on every read from the
//! same tables the proof engine writes, so a goal can never disagree with
//! the profile it describes — there is no counter to drift.
//!
//! ## Progress model
//!
//! Every kind reduces to "criteria met / criteria required", clamped to
//! `[0, 100]`:
//!
//! * `artifact_count` and `skill_level` have a single criterion, so the
//!   percentage is that one ratio.
//! * `capability` is inherently binary — you hold it or you don't — so it
//!   reports 0 or 100. Showing "62% of the way to mentor" would be a lie:
//!   nothing about the grant is incremental.
//! * `rank` is a composite (verified deliverables + attestations, plus the
//!   `mentor` capability for `doyen`). It averages the per-criterion
//!   ratios, each capped at 1 first. Capping before averaging matters: 80
//!   deliverables and 0 attestations is not 100% of the way to `doyen`,
//!   and without the cap the surplus deliverables would mask the missing
//!   attestations.
//!
//! ## ETA model
//!
//! `eta_days_at_current_pace` extrapolates from the last
//! [`PACE_WINDOW_DAYS`] days of *verified* output. It is deliberately
//! naive and deliberately honest about it:
//!
//! * A user with no output in the window has no pace, so the ETA is
//!   `None` rather than infinity or a fabricated number.
//! * When a goal has several unmet criteria, the ETA is the slowest one —
//!   the goal completes when the last criterion does, not the first.
//! * The `mentor` requirement for `doyen` has no pace at all (it is
//!   granted, not accumulated), so a `doyen` goal for a non-mentor
//!   reports `None`.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ranks;

/// Trailing window used to estimate a user's throughput.
///
/// 90 days smooths over a holiday or a heavy sprint without being so long
/// that a user who stopped three months ago still shows momentum.
pub const PACE_WINDOW_DAYS: i64 = 90;

pub const GOAL_KINDS: &[&str] = &["rank", "skill_level", "capability", "artifact_count"];

/// A goal row as stored.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Goal {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub target_value: String,
    pub target_skill_id: Option<Uuid>,
    pub deadline: Option<chrono::NaiveDate>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub achieved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A goal plus its freshly computed progress.
#[derive(Debug, Clone, Serialize)]
pub struct GoalProgress {
    pub goal: Goal,
    /// 0.0..=100.0, rounded to one decimal.
    pub progress_pct: f64,
    /// True when every criterion is met right now.
    pub achieved: bool,
    /// Per-criterion detail, so the front end can say *what* is missing
    /// instead of only how far away it is.
    pub criteria: Vec<Criterion>,
    /// `None` when there is no measurable pace, or when the remaining work
    /// is not the kind that accumulates (a capability grant).
    pub eta_days_at_current_pace: Option<i64>,
}

/// One measurable component of a goal.
#[derive(Debug, Clone, Serialize)]
pub struct Criterion {
    /// Stable identifier: `verified_deliverables`, `attestations`,
    /// `mentor_capability`, `proficiency_level`, `capability`.
    pub name: String,
    pub current: i64,
    pub required: i64,
}

impl Criterion {
    fn ratio(&self) -> f64 {
        if self.required <= 0 {
            // A zero-cost criterion is already satisfied (apprenti).
            return 1.0;
        }
        (self.current as f64 / self.required as f64).clamp(0.0, 1.0)
    }

    fn met(&self) -> bool {
        self.current >= self.required
    }

    fn remaining(&self) -> i64 {
        (self.required - self.current).max(0)
    }
}

/// Reject an unknown goal kind before it reaches the DB CHECK.
pub fn validate_kind(kind: &str) -> Result<(), AppError> {
    if GOAL_KINDS.contains(&kind) {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "kind must be one of: {}",
        GOAL_KINDS.join(", ")
    )))
}

/// Validate `target_value` against its kind, returning a clean 400 rather
/// than letting a nonsense target silently sit at 0% forever.
pub async fn validate_target(
    db: &PgPool,
    kind: &str,
    target_value: &str,
    target_skill_id: Option<Uuid>,
) -> Result<(), AppError> {
    validate_kind(kind)?;
    match kind {
        "rank" => {
            // `apprenti` is granted at signup — targeting it is a no-op goal.
            if ranks::rank_position(target_value).unwrap_or(0) == 0 {
                return Err(AppError::Validation(format!(
                    "target_value must be a rank above apprenti, one of: {}",
                    ranks::rank_order()[1..].join(", ")
                )));
            }
            if target_skill_id.is_some() {
                return Err(AppError::Validation(
                    "target_skill_id is only valid for kind=skill_level".into(),
                ));
            }
        }
        "skill_level" => {
            let level: i64 = target_value
                .parse()
                .map_err(|_| AppError::Validation("target_value must be an integer 1..5".into()))?;
            if !(1..=5).contains(&level) {
                return Err(AppError::Validation(
                    "target_value must be an integer 1..5".into(),
                ));
            }
            let skill_id = target_skill_id.ok_or_else(|| {
                AppError::Validation("target_skill_id is required for kind=skill_level".into())
            })?;
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM skill_nodes WHERE id = $1)")
                    .bind(skill_id)
                    .fetch_one(db)
                    .await?;
            if !exists {
                return Err(AppError::NotFound(format!("skill {skill_id} not found")));
            }
        }
        "capability" => {
            if target_skill_id.is_some() {
                return Err(AppError::Validation(
                    "target_skill_id is only valid for kind=skill_level".into(),
                ));
            }
            // Read from the catalogue, not from a hardcoded list — and no
            // longer by pattern-matching the text of a CHECK constraint.
            // Migration 0404 made the capabilities rows and dropped
            // `user_capabilities_capability_check`, so that lookup answered
            // false for every capability that exists: goals of this kind
            // could not be created at all, and the message said the
            // capability was unknown.
            let known: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM capability_catalog WHERE capability = $1)",
            )
            .bind(target_value)
            .fetch_one(db)
            .await
            .unwrap_or(false);
            if !known {
                return Err(AppError::Validation(format!(
                    "unknown capability '{target_value}'"
                )));
            }
        }
        "artifact_count" => {
            let count: i64 = target_value.parse().map_err(|_| {
                AppError::Validation("target_value must be a positive integer".into())
            })?;
            if count < 1 {
                return Err(AppError::Validation(
                    "target_value must be a positive integer".into(),
                ));
            }
            if target_skill_id.is_some() {
                return Err(AppError::Validation(
                    "target_skill_id is only valid for kind=skill_level".into(),
                ));
            }
        }
        _ => unreachable!("validate_kind ran first"),
    }
    Ok(())
}

/// The user-state snapshot every progress computation reads from.
///
/// Fetched once per request and shared across all of a user's goals —
/// listing ten goals must not mean forty round trips.
#[derive(Debug, Clone, Copy)]
struct UserSnapshot {
    verified_deliverables: i64,
    attestations: i64,
    is_mentor: bool,
    /// Verified deliverables inside the pace window.
    deliverables_in_window: i64,
    /// Attestations inside the pace window.
    attestations_in_window: i64,
}

async fn load_snapshot(db: &PgPool, user_id: Uuid) -> Result<UserSnapshot, AppError> {
    // One round trip: the four counters are independent scalar subqueries
    // over tables the proof engine already indexes by user_id.
    let row: (i64, i64, bool, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM deliverables
              WHERE user_id = $1 AND verification_status = 'verified'),
            (SELECT COUNT(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL),
            (SELECT EXISTS(
                 SELECT 1 FROM user_capabilities
                  WHERE user_id = $1 AND capability = 'mentor'
                    AND revoked_at IS NULL
                    AND (expires_at IS NULL OR expires_at > NOW())
             )),
            (SELECT COUNT(*) FROM deliverables
              WHERE user_id = $1 AND verification_status = 'verified'
                AND verified_at >= NOW() - MAKE_INTERVAL(days => $2::INT)),
            (SELECT COUNT(*) FROM attestations
              WHERE user_id = $1 AND revoked_at IS NULL
                AND issued_at >= NOW() - MAKE_INTERVAL(days => $2::INT))
        "#,
    )
    .bind(user_id)
    .bind(PACE_WINDOW_DAYS as i32)
    .fetch_one(db)
    .await?;

    Ok(UserSnapshot {
        verified_deliverables: row.0,
        attestations: row.1,
        is_mentor: row.2,
        deliverables_in_window: row.3,
        attestations_in_window: row.4,
    })
}

/// Days to accumulate `remaining` more items, given `in_window` produced
/// over [`PACE_WINDOW_DAYS`]. `None` when the pace is zero.
fn eta_for(remaining: i64, in_window: i64) -> Option<i64> {
    if remaining <= 0 {
        return Some(0);
    }
    if in_window <= 0 {
        return None;
    }
    let per_day = in_window as f64 / PACE_WINDOW_DAYS as f64;
    Some((remaining as f64 / per_day).ceil() as i64)
}

/// Compute progress for a single goal.
pub async fn compute_progress(
    db: &PgPool,
    user_id: Uuid,
    goal_id: Uuid,
) -> Result<GoalProgress, AppError> {
    let goal: Goal = sqlx::query_as("SELECT * FROM user_goals WHERE id = $1 AND user_id = $2")
        .bind(goal_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal {goal_id} not found")))?;

    let snapshot = load_snapshot(db, user_id).await?;
    progress_for(db, &goal, &snapshot).await
}

/// Compute progress for every live goal of a user, sharing one snapshot.
pub async fn list_with_progress(
    db: &PgPool,
    user_id: Uuid,
    include_archived: bool,
) -> Result<Vec<GoalProgress>, AppError> {
    let goals: Vec<Goal> = sqlx::query_as(
        r#"
        SELECT * FROM user_goals
         WHERE user_id = $1
           AND ($2::BOOLEAN OR archived_at IS NULL)
         ORDER BY archived_at NULLS FIRST, created_at DESC
        "#,
    )
    .bind(user_id)
    .bind(include_archived)
    .fetch_all(db)
    .await?;

    if goals.is_empty() {
        return Ok(Vec::new());
    }

    let snapshot = load_snapshot(db, user_id).await?;
    let mut out = Vec::with_capacity(goals.len());
    for goal in goals {
        out.push(progress_for(db, &goal, &snapshot).await?);
    }
    Ok(out)
}

async fn progress_for(
    db: &PgPool,
    goal: &Goal,
    snapshot: &UserSnapshot,
) -> Result<GoalProgress, AppError> {
    let (criteria, eta) = match goal.kind.as_str() {
        "rank" => rank_criteria(goal, snapshot),
        "artifact_count" => {
            let required: i64 = goal.target_value.parse().unwrap_or(i64::MAX);
            let c = Criterion {
                name: "verified_deliverables".into(),
                current: snapshot.verified_deliverables,
                required,
            };
            let eta = eta_for(c.remaining(), snapshot.deliverables_in_window);
            (vec![c], eta)
        }
        "skill_level" => {
            let required: i64 = goal.target_value.parse().unwrap_or(i64::MAX);
            let current: i64 = sqlx::query_scalar(
                "SELECT proficiency_level::BIGINT FROM user_skills
                  WHERE user_id = $1 AND skill_id = $2",
            )
            .bind(goal.user_id)
            .bind(goal.target_skill_id)
            .fetch_optional(db)
            .await?
            // No row means the skill has never been proven: level 0, not
            // level 1. The DB default of 1 applies to skills you HAVE.
            .unwrap_or(0);
            let c = Criterion {
                name: "proficiency_level".into(),
                current,
                required,
            };
            // Proficiency is driven by verified deliverables on that skill,
            // so overall verified output is the closest honest pace proxy.
            let eta = eta_for(c.remaining(), snapshot.deliverables_in_window);
            (vec![c], eta)
        }
        "capability" => {
            let held: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM user_capabilities
                     WHERE user_id = $1 AND capability = $2
                       AND revoked_at IS NULL
                       AND (expires_at IS NULL OR expires_at > NOW())
                )
                "#,
            )
            .bind(goal.user_id)
            .bind(&goal.target_value)
            .fetch_one(db)
            .await?;
            let c = Criterion {
                name: "capability".into(),
                current: i64::from(held),
                required: 1,
            };
            // A capability is granted, not accumulated — no pace exists.
            let eta = if held { Some(0) } else { None };
            (vec![c], eta)
        }
        // A kind the CHECK constraint allows but this code doesn't know
        // yet: report no progress rather than panicking on live data.
        _ => (Vec::new(), None),
    };

    let achieved = !criteria.is_empty() && criteria.iter().all(Criterion::met);
    let progress_pct = if criteria.is_empty() {
        0.0
    } else {
        let mean = criteria.iter().map(Criterion::ratio).sum::<f64>() / criteria.len() as f64;
        (mean * 1000.0).round() / 10.0
    };

    Ok(GoalProgress {
        goal: goal.clone(),
        progress_pct,
        achieved,
        criteria,
        eta_days_at_current_pace: eta,
    })
}

/// Criteria for a `rank` goal, plus the ETA of the slowest unmet one.
fn rank_criteria(goal: &Goal, snapshot: &UserSnapshot) -> (Vec<Criterion>, Option<i64>) {
    let Some(t) = ranks::thresholds_for(&goal.target_value) else {
        return (Vec::new(), None);
    };

    let deliverables = Criterion {
        name: "verified_deliverables".into(),
        current: snapshot.verified_deliverables,
        required: t.deliverables,
    };
    let attestations = Criterion {
        name: "attestations".into(),
        current: snapshot.attestations,
        required: t.attestations,
    };

    let mut criteria = vec![deliverables, attestations];
    if t.requires_mentor {
        criteria.push(Criterion {
            name: "mentor_capability".into(),
            current: i64::from(snapshot.is_mentor),
            required: 1,
        });
    }

    // The goal completes when the LAST criterion does. An unmet criterion
    // with no pace (mentor capability, or zero recent output) makes the
    // whole ETA unknowable rather than optimistic.
    let mut eta = Some(0i64);
    for c in &criteria {
        if c.met() {
            continue;
        }
        let this = match c.name.as_str() {
            "verified_deliverables" => eta_for(c.remaining(), snapshot.deliverables_in_window),
            "attestations" => eta_for(c.remaining(), snapshot.attestations_in_window),
            // Granted, not accumulated.
            _ => None,
        };
        match (eta, this) {
            (_, None) => {
                eta = None;
                break;
            }
            (Some(acc), Some(v)) => eta = Some(acc.max(v)),
            (None, _) => break,
        }
    }

    (criteria, eta)
}

/// Stamp `achieved_at` on goals that have just reached 100%.
///
/// Called by the archival job and after a proof recompute. Returns the ids
/// newly stamped. Idempotent: a goal already carrying `achieved_at` is
/// skipped, so the timestamp records the FIRST time the goal was reached.
pub async fn mark_achieved_goals(db: &PgPool, user_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    let progresses = list_with_progress(db, user_id, false).await?;
    let mut stamped = Vec::new();
    for p in progresses {
        if !p.achieved || p.goal.achieved_at.is_some() {
            continue;
        }
        sqlx::query(
            "UPDATE user_goals SET achieved_at = NOW() WHERE id = $1 AND achieved_at IS NULL",
        )
        .bind(p.goal.id)
        .execute(db)
        .await?;
        stamped.push(p.goal.id);
    }
    Ok(stamped)
}

/// Report of one archival sweep.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ArchivalReport {
    /// Goals stamped `achieved_at` during this sweep.
    pub newly_achieved: usize,
    /// Achieved goals moved out of the active list.
    pub archived_achieved: usize,
    /// Unachieved goals whose deadline has passed.
    pub archived_expired: usize,
}

/// Weekly sweep: stamp newly achieved goals, then archive the settled ones.
///
/// A goal is settled when it was achieved (nothing left to track) or when
/// its deadline lapsed without being achieved (tracking it further would
/// just be a standing reproach). Both stay readable via
/// `?include_archived=true`; only the default listing gets quieter.
pub async fn run_archival_sweep(db: &PgPool) -> Result<ArchivalReport, AppError> {
    let mut report = ArchivalReport::default();

    // Only users with at least one live goal — no point recomputing the
    // whole table every week.
    let user_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT DISTINCT user_id FROM user_goals WHERE archived_at IS NULL")
            .fetch_all(db)
            .await?;

    for uid in user_ids {
        match mark_achieved_goals(db, uid).await {
            Ok(ids) => report.newly_achieved += ids.len(),
            // Best-effort per user, mirroring proof_hooks::sweep_active_users:
            // one bad row must not stop the sweep.
            Err(e) => tracing::warn!(user_id = %uid, error = %e, "goal achievement stamp skipped"),
        }
    }

    report.archived_achieved = sqlx::query(
        "UPDATE user_goals SET archived_at = NOW()
          WHERE archived_at IS NULL AND achieved_at IS NOT NULL",
    )
    .execute(db)
    .await?
    .rows_affected() as usize;

    report.archived_expired = sqlx::query(
        "UPDATE user_goals SET archived_at = NOW()
          WHERE archived_at IS NULL
            AND achieved_at IS NULL
            AND deadline IS NOT NULL
            AND deadline < CURRENT_DATE",
    )
    .execute(db)
    .await?
    .rows_affected() as usize;

    Ok(report)
}

/// Default cadence for [`start_goal_archival_task`]: weekly.
const DEFAULT_ARCHIVAL_INTERVAL_SECS: u64 = 60 * 60 * 24 * 7;

/// Background task running [`run_archival_sweep`].
///
/// Env-gated exactly like `proof_hooks::start_proof_sweep_task` — off by
/// default so a dev machine doesn't mutate goal state in the background:
///   - `SKILLUV_GOAL_ARCHIVAL_ENABLED=1` to enable
///   - `SKILLUV_GOAL_ARCHIVAL_INTERVAL_SECS` (default 604800)
pub fn start_goal_archival_task(db: PgPool) {
    if std::env::var("SKILLUV_GOAL_ARCHIVAL_ENABLED").as_deref() != Ok("1") {
        tracing::info!("SKI-38: goal archival disabled (set SKILLUV_GOAL_ARCHIVAL_ENABLED=1)");
        return;
    }
    let interval_secs = std::env::var("SKILLUV_GOAL_ARCHIVAL_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ARCHIVAL_INTERVAL_SECS);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            match run_archival_sweep(&db).await {
                Ok(r) => {
                    tracing::info!(
                        newly_achieved = r.newly_achieved,
                        archived_achieved = r.archived_achieved,
                        archived_expired = r.archived_expired,
                        "SKI-38: goal archival sweep completed"
                    );
                    metrics::counter!("skilluv_goals_achieved_total")
                        .increment(r.newly_achieved as u64);
                    metrics::counter!("skilluv_goals_archived_total")
                        .increment((r.archived_achieved + r.archived_expired) as u64);
                }
                Err(e) => tracing::warn!(error = %e, "SKI-38: goal archival sweep failed"),
            }
        }
    });
}

#[cfg(test)]
mod unit {
    use super::*;

    fn crit(current: i64, required: i64) -> Criterion {
        Criterion {
            name: "x".into(),
            current,
            required,
        }
    }

    #[test]
    fn ratio_is_clamped_and_zero_cost_is_satisfied() {
        assert_eq!(crit(0, 4).ratio(), 0.0);
        assert_eq!(crit(2, 4).ratio(), 0.5);
        assert_eq!(crit(4, 4).ratio(), 1.0);
        // Surplus never exceeds 1 — otherwise it would mask a sibling
        // criterion sitting at 0.
        assert_eq!(crit(80, 4).ratio(), 1.0);
        assert_eq!(crit(0, 0).ratio(), 1.0);
    }

    #[test]
    fn eta_needs_a_pace() {
        // Nothing left to do: zero days, regardless of pace.
        assert_eq!(eta_for(0, 0), Some(0));
        // No recent output: unknowable, not infinite.
        assert_eq!(eta_for(5, 0), None);
        // 90 items in 90 days = 1/day.
        assert_eq!(eta_for(5, 90), Some(5));
        // 45 in 90 days = 0.5/day, so 5 items take 10 days.
        assert_eq!(eta_for(5, 45), Some(10));
        // Always rounds up — a partial day still needs the day.
        assert_eq!(eta_for(1, 45), Some(2));
    }

    #[test]
    fn rank_goal_caps_surplus_per_criterion() {
        let goal = Goal {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            kind: "rank".into(),
            target_value: "artisan".into(),
            target_skill_id: None,
            deadline: None,
            created_at: chrono::Utc::now(),
            achieved_at: None,
            archived_at: None,
        };
        // artisan = 11 deliverables + 1 attestation. Plenty of deliverables,
        // no attestation: must NOT read as complete.
        let snapshot = UserSnapshot {
            verified_deliverables: 80,
            attestations: 0,
            is_mentor: false,
            deliverables_in_window: 90,
            attestations_in_window: 0,
        };
        let (criteria, eta) = rank_criteria(&goal, &snapshot);
        assert_eq!(criteria.len(), 2);
        assert!(!criteria.iter().all(Criterion::met));
        let mean = criteria.iter().map(Criterion::ratio).sum::<f64>() / 2.0;
        assert_eq!(mean, 0.5, "capped deliverables + zero attestations");
        // Attestation pace is zero, so the overall ETA is unknown.
        assert_eq!(eta, None);
    }

    #[test]
    fn doyen_without_mentor_has_no_eta() {
        let goal = Goal {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            kind: "rank".into(),
            target_value: "doyen".into(),
            target_skill_id: None,
            deadline: None,
            created_at: chrono::Utc::now(),
            achieved_at: None,
            archived_at: None,
        };
        let snapshot = UserSnapshot {
            verified_deliverables: 50,
            attestations: 5,
            is_mentor: false,
            deliverables_in_window: 90,
            attestations_in_window: 90,
        };
        let (criteria, eta) = rank_criteria(&goal, &snapshot);
        assert_eq!(criteria.len(), 3, "doyen adds the mentor criterion");
        assert_eq!(eta, None, "a capability grant has no pace");
        assert!(!criteria.iter().all(Criterion::met));
    }

    #[test]
    fn rank_goal_met_reports_zero_eta() {
        let goal = Goal {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            kind: "rank".into(),
            target_value: "ranger".into(),
            target_skill_id: None,
            deadline: None,
            created_at: chrono::Utc::now(),
            achieved_at: None,
            archived_at: None,
        };
        let snapshot = UserSnapshot {
            verified_deliverables: 4,
            attestations: 0,
            is_mentor: false,
            deliverables_in_window: 4,
            attestations_in_window: 0,
        };
        let (criteria, eta) = rank_criteria(&goal, &snapshot);
        assert!(criteria.iter().all(Criterion::met));
        assert_eq!(eta, Some(0));
    }
}
