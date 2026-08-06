//! P26 v2 Phase D — validator workflow for `project_slices`.
//!
//! The validation step advances a challenge from CI-green PR to Skilluv
//! success. A validator (holder of `challenge_validator:{domain}`, see
//! migration 0120) picks a slice up (SKI-83), reviews the PR, then either
//! approves (SKI-84, → status `validated`) or rejects (SKI-85, → status
//! `claimed` with a reason on file).
//!
//! Design decisions:
//! - Pickup is exclusive: DB-level `picked_by_validator_id` is set once
//!   per slice; a second concurrent pickup fails cleanly on the WHERE
//!   status = 'ci_green' predicate (no unique constraint needed).
//! - Validators cannot self-approve their own challenge (they were the
//!   claimer). Enforced at the service layer.
//! - Approve is idempotent from the validator's perspective: calling it
//!   twice with the same holder is a no-op; changing holder mid-flight
//!   fails.
//! - Reject records the reason but does not persist a history table —
//!   the last reason is enough for the frontend to render feedback; a
//!   fuller audit table would be over-engineering for Phase 1 dogfooding.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::capabilities::require_challenge_validator_for;
use crate::models::ProjectSlice;

/// Pick up a submitted slice for validation. Requires the caller to hold
/// `challenge_validator:{slice.primary_domain}` and forbids self-review.
pub async fn pickup(
    db: &PgPool,
    slice_id: Uuid,
    validator_id: Uuid,
) -> Result<ProjectSlice, AppError> {
    // 1. Load minimal fields to check eligibility BEFORE any UPDATE, so
    //    the error message is specific ("wrong domain") rather than a
    //    silent "slice unchanged".
    let row: Option<(String, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT primary_domain, claimed_by_user_id, status FROM project_slices WHERE id = $1",
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?;
    let Some((domain, claimer, status)) = row else {
        return Err(AppError::NotFound("Slice not found".into()));
    };
    if status != "ci_green" {
        return Err(AppError::Validation(format!(
            "Slice cannot be picked up (status={status}, needs ci_green)"
        )));
    }
    if claimer == Some(validator_id) {
        return Err(AppError::Forbidden);
    }
    require_challenge_validator_for(db, validator_id, &domain).await?;

    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices
           SET status = 'pending_validation',
               picked_by_validator_id = $2,
               picked_at = NOW(),
               updated_at = NOW()
         WHERE id = $1
           AND status = 'ci_green'
           AND picked_by_validator_id IS NULL
     RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(validator_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Validation(
            "Slice was picked up by another validator or state changed underneath.".into(),
        )
    })?;
    Ok(slice)
}

/// Approve the PR: advance to `validated`, stamp validator + timestamp.
/// The caller must be the current pickup holder.
pub async fn approve(
    db: &PgPool,
    slice_id: Uuid,
    validator_id: Uuid,
) -> Result<ProjectSlice, AppError> {
    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices
           SET status = 'validated',
               validated_at = NOW(),
               validated_by_user_id = $2,
               updated_at = NOW()
         WHERE id = $1
           AND status = 'pending_validation'
           AND picked_by_validator_id = $2
     RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(validator_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Validation(
            "Cannot approve: not the current pickup holder or slice moved out of pending_validation."
                .into(),
        )
    })?;
    Ok(slice)
}

/// Reject the PR with a mandatory reason. Rewinds to `claimed`, clears
/// the pickup holder, and stores the reason so the challenger sees it.
pub async fn reject(
    db: &PgPool,
    slice_id: Uuid,
    validator_id: Uuid,
    reason: &str,
) -> Result<ProjectSlice, AppError> {
    let trimmed = reason.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 2000 {
        return Err(AppError::Validation(
            "reject reason must be non-empty and at most 2000 characters".into(),
        ));
    }
    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices
           SET status = 'claimed',
               picked_by_validator_id = NULL,
               picked_at = NULL,
               validation_reject_reason = $3,
               updated_at = NOW()
         WHERE id = $1
           AND status = 'pending_validation'
           AND picked_by_validator_id = $2
     RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(validator_id)
    .bind(trimmed)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Validation(
            "Cannot reject: not the current pickup holder or slice moved out of pending_validation."
                .into(),
        )
    })?;
    Ok(slice)
}

/// List slices currently held by this validator (`pending_validation`).
pub async fn my_queue(db: &PgPool, validator_id: Uuid) -> Result<Vec<ProjectSlice>, AppError> {
    let rows = sqlx::query_as::<_, ProjectSlice>(
        r#"
        SELECT * FROM project_slices
        WHERE picked_by_validator_id = $1
          AND status = 'pending_validation'
        ORDER BY picked_at ASC
        "#,
    )
    .bind(validator_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
