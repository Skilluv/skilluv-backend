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

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::capabilities::require_challenge_validator_for;
use crate::models::ProjectSlice;

type HmacSha256 = Hmac<Sha256>;

/// SKI-90 — SHA-256-HMAC over the approval tuple. `JWT_SECRET`-keyed so
/// the hash cannot be forged from DB read-only access.
///
/// Kept pure (no I/O) for easy unit-testing. `validated_at` is the
/// timestamp the caller decided on (usually `Utc::now()` inside
/// `approve`) — the same input must be persisted alongside the hash to
/// keep verification deterministic later.
pub fn compute_attestation_hash(
    jwt_secret: &str,
    slice_id: Uuid,
    submitted_pr_url: &str,
    validated_at: chrono::DateTime<chrono::Utc>,
    validator_id: Uuid,
) -> String {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(jwt_secret.as_bytes())
        .expect("hmac accepts any key");
    mac.update(b"skilluv-attestation-v1|");
    mac.update(slice_id.as_bytes());
    mac.update(b"|");
    mac.update(submitted_pr_url.as_bytes());
    mac.update(b"|");
    mac.update(validated_at.timestamp_micros().to_be_bytes().as_slice());
    mac.update(b"|");
    mac.update(validator_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

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

/// Approve the PR: advance to `validated`, stamp validator + timestamp,
/// compute the SKI-90 attestation hash, and fire the SKI-91 proof hooks
/// so ranks / badges / stats catch the new success on the same request.
/// The caller must be the current pickup holder.
pub async fn approve(
    db: &PgPool,
    jwt_secret: &str,
    slice_id: Uuid,
    validator_id: Uuid,
) -> Result<ProjectSlice, AppError> {
    // Compute now() ONCE so the hash matches the row's validated_at.
    let now = chrono::Utc::now();

    // Load the URL first — the hash needs it and we want to fail early
    // if the slice is not in a shape we can attest.
    let existing: Option<(Option<String>,)> =
        sqlx::query_as("SELECT submitted_pr_url FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_optional(db)
            .await?;
    let submitted_pr_url = existing
        .and_then(|(u,)| u)
        .ok_or_else(|| AppError::Validation("Slice has no submitted_pr_url on file.".into()))?;

    let attestation_hash =
        compute_attestation_hash(jwt_secret, slice_id, &submitted_pr_url, now, validator_id);

    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices
           SET status = 'validated',
               validated_at = $3,
               validated_by_user_id = $2,
               attestation_hash = $4,
               updated_at = NOW()
         WHERE id = $1
           AND status = 'pending_validation'
           AND picked_by_validator_id = $2
     RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(validator_id)
    .bind(now)
    .bind(&attestation_hash)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Validation(
            "Cannot approve: not the current pickup holder or slice moved out of pending_validation."
                .into(),
        )
    })?;

    // SKI-114 (M-08) — append-only decision journal. Enables exact
    // approve/reject counts and per-decision latency without relying on
    // the mutable pickup fields on the slice.
    let _ = sqlx::query(
        r#"
        INSERT INTO slice_validation_decisions (slice_id, validator_id, decision, picked_at)
        VALUES ($1, $2, 'approve', $3)
        "#,
    )
    .bind(slice_id)
    .bind(validator_id)
    .bind(slice.picked_at)
    .execute(db)
    .await;

    // SKI-91 — trigger the proof engine for the challenger so ranks,
    // capabilities, and badges recompute against the freshly validated
    // challenge. Best-effort: a hook failure must not roll back the
    // approval itself (the challenger's success is already persisted).
    if let Some(claimer) = slice.claimed_by_user_id {
        let db_clone = db.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::services::proof_hooks::recompute_all_for_user(&db_clone, claimer).await
            {
                tracing::warn!(
                    user_id = %claimer, error = %e,
                    "SKI-91 proof recompute after validation failed"
                );
            }
        });
    }

    Ok(slice)
}

/// Reject the PR with a mandatory reason. Rewinds to `claimed`, clears
/// the pickup holder, and stores the reason so the challenger sees it.
/// What kind of blocker a rejection names.
///
/// Required, because "rejected" tells a contributor nothing about what to do
/// next: red CI is a different action from a naming comment, which is
/// different again from a slice that was never the right size.
pub const BLOCKING_REASONS: &[&str] = &[
    "ci_failing",
    "tests_missing",
    "docs_missing",
    "review_comments",
    "scope_mismatch",
    "out_of_depth",
];

pub async fn reject(
    db: &PgPool,
    slice_id: Uuid,
    validator_id: Uuid,
    reason: &str,
    blocking_reason: &str,
) -> Result<ProjectSlice, AppError> {
    if !BLOCKING_REASONS.contains(&blocking_reason) {
        return Err(AppError::Validation(format!(
            "unknown blocking reason '{blocking_reason}' (expected one of: {})",
            BLOCKING_REASONS.join(", ")
        )));
    }
    let trimmed = reason.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 2000 {
        return Err(AppError::Validation(
            "reject reason must be non-empty and at most 2000 characters".into(),
        ));
    }
    // Snapshot picked_at BEFORE the UPDATE clears it, so the decision
    // journal can carry an exact pickup->decision latency.
    let picked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT picked_at FROM project_slices WHERE id = $1 AND picked_by_validator_id = $2",
    )
    .bind(slice_id)
    .bind(validator_id)
    .fetch_optional(db)
    .await?
    .flatten();

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

    // SKI-114 (M-08) — journal the reject with the reason.
    let _ = sqlx::query(
        r#"
        INSERT INTO slice_validation_decisions
            (slice_id, validator_id, decision, reason, blocking_reason, picked_at)
        VALUES ($1, $2, 'reject', $3, $5, $4)
        "#,
    )
    .bind(slice_id)
    .bind(validator_id)
    .bind(trimmed)
    .bind(picked_at)
    .execute(db)
    .await;

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

#[cfg(test)]
mod hash_tests {
    use super::compute_attestation_hash;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn fixed_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn hash_is_64_hex_chars() {
        let h = compute_attestation_hash(
            "s3cret",
            Uuid::nil(),
            "https://github.com/x/y/pull/1",
            fixed_time(),
            Uuid::nil(),
        );
        assert_eq!(h.len(), 64);
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }

    #[test]
    fn hash_changes_with_any_field() {
        let base = compute_attestation_hash(
            "s3cret",
            Uuid::nil(),
            "https://github.com/x/y/pull/1",
            fixed_time(),
            Uuid::nil(),
        );
        assert_ne!(
            base,
            compute_attestation_hash(
                "OTHER-secret",
                Uuid::nil(),
                "https://github.com/x/y/pull/1",
                fixed_time(),
                Uuid::nil(),
            )
        );
        assert_ne!(
            base,
            compute_attestation_hash(
                "s3cret",
                Uuid::nil(),
                "https://github.com/x/y/pull/2",
                fixed_time(),
                Uuid::nil(),
            )
        );
    }
}
