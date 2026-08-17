//! The critique loop of a design challenge.
//!
//! ## Why design does not reuse `slice_validation`
//!
//! That module implements the code workflow: a reviewer picks up a CI-green
//! slice, holds it exclusively, then approves or rejects. Design has neither
//! half of that. There is no CI signal telling anyone the work is ready to
//! look at, and the verdict is not binary — the ordinary outcome of a design
//! review is "go one more round".
//!
//! ```text
//! claimed / in_progress
//!     │  submit_version()
//!     ▼
//! pending_validation
//!     │  review()
//!     ├── iterate ──► in_iteration ──► submit_version() (next round)
//!     ├── approve ──► validated      (deliverable, fragments, attestation)
//!     └── reject ───► closed
//! ```
//!
//! ## No pickup step, on purpose
//!
//! A pickup field is a lock that has to be released, and a reviewer who opens
//! a critique and never finishes would freeze the designer's challenge until
//! an admin intervened. Exclusivity here comes from the status instead: the
//! decision is written with `WHERE status = 'pending_validation'`, so two
//! reviewers racing resolve to exactly one, and nothing is left held.
//!
//! ## Where the five-round ceiling comes from
//!
//! Migration 0184, which every domain shares. Past five rounds the problem is
//! the brief or the assignment, not the work, and a sixth identical critique
//! helps nobody. The trigger raises before this module gets a chance to.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::capabilities::require_reviewer_for_orientation;
use crate::models::ProjectSlice;

/// Blocking reasons a design review may give. Mirrors the CHECK added by
/// migration 0232; the shared code reasons stay available because some of
/// them apply everywhere (`docs_missing`, `scope_mismatch`, `out_of_depth`).
pub const DESIGN_BLOCKING_REASONS: &[&str] = &[
    "brief_unmet",
    "direction_mismatch",
    "craft_gap",
    "accessibility",
    "system_inconsistent",
    "rights_unclear",
    "derivative",
    "docs_missing",
    "scope_mismatch",
    "out_of_depth",
];

/// Statuses a designer may submit a new version from.
const SUBMITTABLE_STATUSES: &[&str] = &["claimed", "in_progress", "in_iteration"];

/// What a reviewer decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The challenge is a success: deliverable, fragments, attestation.
    Approve,
    /// Another version is expected. The challenge stays open.
    Iterate,
    /// Refused for good.
    Reject,
}

impl Verdict {
    pub fn as_decision(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Iterate => "iterate",
            Self::Reject => "reject",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approve" => Some(Self::Approve),
            "iterate" => Some(Self::Iterate),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }

    /// The status the slice moves to.
    fn resulting_status(&self) -> &'static str {
        match self {
            Self::Approve => "validated",
            Self::Iterate => "in_iteration",
            Self::Reject => "closed",
        }
    }
}

pub struct ReviewInput<'a> {
    pub verdict: Verdict,
    /// Required for `iterate` and `reject`. Telling somebody to come back
    /// without saying what to change wastes a round.
    pub blocking_reason: Option<&'a str>,
    pub feedback_md: Option<&'a str>,
    /// The review grid of migration 0230, filled in.
    pub grid_scores: Option<serde_json::Value>,
}

/// The design fields a transition needs, plus the trade's slug.
#[derive(Debug, Clone, sqlx::FromRow)]
struct DesignSliceState {
    title: String,
    status: String,
    slice_type: String,
    claimed_by_user_id: Option<Uuid>,
    orientation_slug: Option<String>,
    design_external_url: Option<String>,
    design_version_notes_md: Option<String>,
    fragments_reward: i32,
    design_subtype: Option<String>,
}

async fn load_design_slice(db: &PgPool, slice_id: Uuid) -> Result<DesignSliceState, AppError> {
    let state: Option<DesignSliceState> = sqlx::query_as(
        r#"
        SELECT s.title, s.status, s.slice_type, s.claimed_by_user_id,
               o.slug AS orientation_slug,
               s.design_external_url, s.design_version_notes_md,
               s.fragments_reward, s.design_subtype
          FROM project_slices s
          LEFT JOIN orientations o ON o.id = s.orientation_id
         WHERE s.id = $1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?;

    let state = state.ok_or_else(|| AppError::NotFound("Slice not found".into()))?;
    if state.slice_type != "design_artifact" {
        return Err(AppError::Validation(
            "this slice is not a design artefact; use the code validation endpoints".into(),
        ));
    }
    Ok(state)
}

// ═══════════════════════════════════════════════════════════════════
// Designer side
// ═══════════════════════════════════════════════════════════════════

/// Hand in a version and ask for a critique.
///
/// The version lives on the slice while it is current; a reviewer copies it
/// into the decision row, so the journal ends up holding every version that
/// was actually read.
pub async fn submit_version(
    db: &PgPool,
    slice_id: Uuid,
    designer_id: Uuid,
    artifact_url: &str,
    notes_md: Option<&str>,
) -> Result<ProjectSlice, AppError> {
    let url = artifact_url.trim();
    if url.chars().count() < 4 || url.chars().count() > 2048 {
        return Err(AppError::Validation(
            "the version URL must be between 4 and 2048 characters".into(),
        ));
    }
    if !(url.starts_with("https://") || url.starts_with("s3://")) {
        return Err(AppError::Validation(
            "the version URL must be an https link or a stored object".into(),
        ));
    }

    let state = load_design_slice(db, slice_id).await?;
    if state.claimed_by_user_id != Some(designer_id) {
        return Err(AppError::Forbidden);
    }
    if !SUBMITTABLE_STATUSES.contains(&state.status.as_str()) {
        return Err(AppError::Conflict(format!(
            "a version cannot be handed in from status {}",
            state.status
        )));
    }

    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices
           SET design_external_url = $2,
               design_version_notes_md = $3,
               status = 'pending_validation',
               updated_at = NOW()
         WHERE id = $1
           AND status = ANY($4)
     RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(url)
    .bind(notes_md.map(str::trim).filter(|s| !s.is_empty()))
    .bind(SUBMITTABLE_STATUSES)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::Conflict("the slice moved underneath".into()))?;

    Ok(slice)
}

// ═══════════════════════════════════════════════════════════════════
// Reviewer side
// ═══════════════════════════════════════════════════════════════════

/// Record a critique and move the slice.
///
/// Requires `design_reviewer:{group}` for the slice's trade, or the domain
/// wildcard. A reviewer may not decide a challenge they claimed themselves.
pub async fn review(
    db: &PgPool,
    slice_id: Uuid,
    reviewer_id: Uuid,
    input: ReviewInput<'_>,
) -> Result<ProjectSlice, AppError> {
    let state = load_design_slice(db, slice_id).await?;

    if state.status != "pending_validation" {
        return Err(AppError::Conflict(format!(
            "no version is waiting for a critique (status {})",
            state.status
        )));
    }
    if state.claimed_by_user_id == Some(reviewer_id) {
        return Err(AppError::Forbidden);
    }

    let orientation = state.orientation_slug.clone().ok_or_else(|| {
        AppError::Validation("this design slice names no trade, so nothing can route it".into())
    })?;
    require_reviewer_for_orientation(db, reviewer_id, &orientation).await?;

    let feedback = input.feedback_md.map(str::trim).filter(|s| !s.is_empty());
    let blocking_reason = input.blocking_reason.map(str::trim).filter(|s| !s.is_empty());

    match input.verdict {
        Verdict::Approve => {
            if blocking_reason.is_some() {
                return Err(AppError::Validation(
                    "an approval carries no blocking reason".into(),
                ));
            }
        }
        Verdict::Iterate | Verdict::Reject => {
            let Some(reason) = blocking_reason else {
                return Err(AppError::Validation(format!(
                    "a {} needs a blocking reason (one of: {})",
                    input.verdict.as_decision(),
                    DESIGN_BLOCKING_REASONS.join(", ")
                )));
            };
            if !DESIGN_BLOCKING_REASONS.contains(&reason) {
                return Err(AppError::Validation(format!(
                    "unknown blocking reason '{reason}' (expected one of: {})",
                    DESIGN_BLOCKING_REASONS.join(", ")
                )));
            }
            // A refusal that says nothing is the one thing the design charter
            // rules out. Forty characters is not a quality bar, it is a floor
            // under "no".
            match feedback {
                Some(f) if f.chars().count() >= 40 => {}
                _ => {
                    return Err(AppError::Validation(
                        "a critique needs at least 40 characters of written feedback: \
                         a verdict the designer cannot act on wastes the round"
                            .into(),
                    ));
                }
            }
        }
    }

    if let Some(f) = feedback
        && f.chars().count() > 20_000
    {
        return Err(AppError::Validation(
            "feedback must be at most 20000 characters".into(),
        ));
    }

    let mut tx = db.begin().await?;

    // Move the slice first, under the status it was read in. Two reviewers
    // racing means the second gets zero rows and a clear conflict, and no
    // decision is written for a slice that had already moved.
    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices
           SET status = $2,
               validated_at = CASE WHEN $2 = 'validated' THEN NOW() ELSE validated_at END,
               validated_by_user_id = CASE WHEN $2 = 'validated' THEN $3 ELSE validated_by_user_id END,
               closed_at = CASE WHEN $2 = 'closed' THEN NOW() ELSE closed_at END,
               validation_reject_reason = CASE WHEN $2 = 'closed' THEN $4 ELSE validation_reject_reason END,
               updated_at = NOW()
         WHERE id = $1 AND status = 'pending_validation'
     RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(input.verdict.resulting_status())
    .bind(reviewer_id)
    .bind(feedback)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        AppError::Conflict("this version was already reviewed by somebody else".into())
    })?;

    // The round number is set by the trigger from migration 0184, and the
    // sixth insert raises rather than being recorded.
    sqlx::query(
        r#"
        INSERT INTO slice_validation_decisions
            (slice_id, validator_id, decision, reason, blocking_reason,
             reviewed_artifact_url, reviewed_artifact_notes_md, grid_scores)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(slice_id)
    .bind(reviewer_id)
    .bind(input.verdict.as_decision())
    .bind(feedback)
    .bind(blocking_reason)
    .bind(state.design_external_url.as_deref())
    .bind(state.design_version_notes_md.as_deref())
    .bind(&input.grid_scores)
    .execute(&mut *tx)
    .await?;

    if input.verdict == Verdict::Approve {
        let designer = state.claimed_by_user_id.ok_or_else(|| {
            AppError::Validation("a design challenge nobody claimed cannot be validated".into())
        })?;
        let rounds: i16 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(round), 1) FROM slice_validation_decisions WHERE slice_id = $1",
        )
        .bind(slice_id)
        .fetch_one(&mut *tx)
        .await?;

        let deliverable_id = record_deliverable(&mut tx, &state, slice_id, designer, reviewer_id).await?;
        tx.commit().await?;

        // Outside the transaction: an attestation that fails to write is a
        // re-runnable problem, whereas a half-written validation is not. The
        // proof — the deliverable — is already committed.
        if let (Some(deliverable_id), Some(url)) = (deliverable_id, state.design_external_url.as_deref())
        {
            crate::services::design_attestations::deliverable_validated(
                db,
                designer,
                deliverable_id,
                &state.title,
                url,
                rounds,
            )
            .await?;
        }

        let db_clone = db.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::services::proof_hooks::recompute_all_for_user(&db_clone, designer).await
            {
                tracing::warn!(
                    user_id = %designer, error = %e,
                    "proof recompute after design validation failed"
                );
            }
        });

        return Ok(slice);
    }

    tx.commit().await?;
    Ok(slice)
}

/// Turn a validated design challenge into a row of the proof table.
///
/// This is what makes design count: `ranks` counts verified `deliverables`,
/// the badge engine reads them, the public portfolio renders them. A
/// validated slice with no deliverable row would move nothing.
///
/// Returns `None` when a deliverable already exists, which is what makes a
/// replayed validation safe rather than a second payout.
async fn record_deliverable(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    state: &DesignSliceState,
    slice_id: Uuid,
    designer_id: Uuid,
    reviewer_id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deliverables WHERE slice_id = $1 AND user_id = $2)",
    )
    .bind(slice_id)
    .bind(designer_id)
    .fetch_one(&mut **tx)
    .await?;
    if already {
        return Ok(None);
    }

    let url = state
        .design_external_url
        .clone()
        .ok_or_else(|| AppError::Validation("no version to record as a deliverable".into()))?;

    let deliverable_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO deliverables (
            slice_id, user_id, artifact_type, artifact_url, artifact_metadata,
            verifiable_by, verification_status, verified_at, verified_by_user_id,
            fragments_awarded, public
        )
        VALUES ($1, $2, 'design_artifact', $3, $4,
                'human_review', 'verified', NOW(), $5, $6, TRUE)
        RETURNING id
        "#,
    )
    .bind(slice_id)
    .bind(designer_id)
    .bind(&url)
    .bind(serde_json::json!({ "design_subtype": state.design_subtype }))
    .bind(reviewer_id)
    .bind(state.fragments_reward)
    .fetch_one(&mut **tx)
    .await?;

    if state.fragments_reward > 0 {
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(state.fragments_reward)
        .bind(designer_id)
        .execute(&mut **tx)
        .await?;
    }

    // The slice's tagged skills move onto the designer's graph, exactly as a
    // merged pull request does for a developer.
    crate::services::deliverables::DeliverablesService::propagate_skills(
        tx,
        slice_id,
        designer_id,
        deliverable_id,
    )
    .await?;

    Ok(Some(deliverable_id))
}

// ═══════════════════════════════════════════════════════════════════
// Reads
// ═══════════════════════════════════════════════════════════════════

/// One round of the critique trail, as the public profile renders it.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ReviewRound {
    pub round: i16,
    pub decision: String,
    pub blocking_reason: Option<String>,
    pub reason: Option<String>,
    pub reviewed_artifact_url: Option<String>,
    pub reviewed_artifact_notes_md: Option<String>,
    pub grid_scores: Option<serde_json::Value>,
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

/// The whole trail, oldest first — the order the story reads in.
pub async fn history(db: &PgPool, slice_id: Uuid) -> Result<Vec<ReviewRound>, AppError> {
    let rounds = sqlx::query_as::<_, ReviewRound>(
        r#"
        SELECT round, decision, blocking_reason, reason,
               reviewed_artifact_url, reviewed_artifact_notes_md,
               grid_scores, decided_at
          FROM slice_validation_decisions
         WHERE slice_id = $1
         ORDER BY round ASC
        "#,
    )
    .bind(slice_id)
    .fetch_all(db)
    .await?;
    Ok(rounds)
}

/// Design slices waiting for a critique, in the trades this reviewer is
/// competent in. Oldest first, so nobody waits twice.
pub async fn reviewer_queue(
    db: &PgPool,
    reviewer_id: Uuid,
    limit: i64,
) -> Result<Vec<ProjectSlice>, AppError> {
    let caps = crate::middleware::capabilities::list_active_capabilities(db, reviewer_id).await?;
    let wildcard = caps
        .iter()
        .any(|c| c == "design_reviewer:all" || c == "admin");
    let groups: Vec<String> = caps
        .iter()
        .filter_map(|c| c.strip_prefix("design_reviewer:").map(str::to_string))
        .filter(|g| g != "all")
        .collect();

    if !wildcard && groups.is_empty() {
        return Ok(Vec::new());
    }

    let slices = sqlx::query_as::<_, ProjectSlice>(
        r#"
        SELECT s.* FROM project_slices s
          LEFT JOIN orientations o ON o.id = s.orientation_id
         WHERE s.slice_type = 'design_artifact'
           AND s.status = 'pending_validation'
           AND s.claimed_by_user_id IS DISTINCT FROM $1
           AND ($2::BOOLEAN OR o.reviewer_group = ANY($3))
         ORDER BY s.updated_at ASC
         LIMIT $4
        "#,
    )
    .bind(reviewer_id)
    .bind(wildcard)
    .bind(&groups)
    .bind(limit.clamp(1, 200))
    .fetch_all(db)
    .await?;
    Ok(slices)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn verdicts_round_trip() {
        for v in [Verdict::Approve, Verdict::Iterate, Verdict::Reject] {
            assert_eq!(Verdict::parse(v.as_decision()), Some(v));
        }
        assert_eq!(Verdict::parse("maybe"), None);
    }

    #[test]
    fn each_verdict_lands_the_slice_somewhere_different() {
        assert_eq!(Verdict::Approve.resulting_status(), "validated");
        assert_eq!(Verdict::Iterate.resulting_status(), "in_iteration");
        assert_eq!(Verdict::Reject.resulting_status(), "closed");
    }

    #[test]
    fn iterating_does_not_close_the_challenge() {
        // The distinction the whole module exists for: asking for another
        // version leaves the slice open, refusing does not.
        assert_ne!(
            Verdict::Iterate.resulting_status(),
            Verdict::Reject.resulting_status()
        );
        assert_eq!(Verdict::Iterate.resulting_status(), "in_iteration");
    }

    #[test]
    fn design_reasons_do_not_collide_with_the_code_ones() {
        // `ci_failing` and `tests_missing` mean nothing on a brand identity,
        // and offering them would invite a reviewer to pick the nearest wrong
        // label.
        for code_only in ["ci_failing", "tests_missing", "review_comments"] {
            assert!(
                !DESIGN_BLOCKING_REASONS.contains(&code_only),
                "{code_only} should not be offered on a design review"
            );
        }
    }
}
