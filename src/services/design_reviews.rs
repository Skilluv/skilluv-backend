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
/// migration 0506; the shared code reasons stay available because some of
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
    /// The review grid of migration 0504, filled in.
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
    published_artifact_url: Option<String>,
    design_version_notes_md: Option<String>,
    fragments_reward: i32,
    design_subtype: Option<String>,
}

async fn load_design_slice(db: &PgPool, slice_id: Uuid) -> Result<DesignSliceState, AppError> {
    let state: Option<DesignSliceState> = sqlx::query_as(
        r#"
        SELECT s.title, s.status, s.slice_type, s.claimed_by_user_id,
               o.slug AS orientation_slug,
               s.published_artifact_url, s.design_version_notes_md,
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
           SET published_artifact_url = $2,
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

    notify_reviewers_of(db, &slice).await;
    run_auto_checks_in_background(db, slice_id, url);
    Ok(slice)
}

/// Ask the machine what it can say about this version, without making the
/// designer wait for it.
///
/// Spawned rather than awaited because the checks fetch somebody else's host,
/// and a submission that hangs for fifteen seconds on a slow CDN is a worse
/// experience than a panel that fills in a moment later. A failure is the
/// panel saying so, never a failed submission.
fn run_auto_checks_in_background(db: &PgPool, slice_id: Uuid, artifact_url: &str) {
    let db = db.clone();
    let url = artifact_url.to_string();
    tokio::spawn(async move {
        // The round this version will be reviewed as. The decision journal's
        // trigger numbers rounds on insert, and no decision exists yet — so
        // the next one is one past the highest recorded.
        let round: i16 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(round), 0::SMALLINT) + 1
               FROM slice_validation_decisions WHERE slice_id = $1",
        )
        .bind(slice_id)
        .fetch_one(&db)
        .await
        .unwrap_or(1);

        // A sixth version cannot be reviewed — the ceiling is five — and the
        // results table says so too. Nothing to record.
        if !(1..=5).contains(&round) {
            return;
        }

        if let Err(e) =
            crate::services::design_auto_checks::run_for_version(&db, slice_id, round, &url).await
        {
            tracing::warn!(%slice_id, round, error = %e, "automatic checks did not run");
        }
    });
}

/// Tell the people competent in this trade that a version is waiting.
///
/// Addressed to the trade's own reviewers plus the domain wildcard, rather
/// than to everybody holding any design capability: a type designer being
/// pinged about a motion brief is how a queue gets muted. No email by
/// default — a reviewer opens the queue on purpose.
///
/// A slice with no trade reaches nobody, which is correct: it is the
/// condition `review()` refuses on, and inventing a recipient would only
/// forward the problem.
async fn notify_reviewers_of(db: &PgPool, slice: &ProjectSlice) {
    let Some(orientation_id) = slice.orientation_id else {
        return;
    };
    let group: Option<String> =
        match sqlx::query_scalar("SELECT reviewer_group FROM orientations WHERE id = $1")
            .bind(orientation_id)
            .fetch_optional(db)
            .await
        {
            Ok(row) => row.flatten(),
            Err(e) => {
                tracing::warn!(error = %e, "could not resolve the reviewer group");
                return;
            }
        };

    let mut capabilities = vec!["design_reviewer:all".to_string()];
    if let Some(group) = group {
        capabilities.push(format!("design_reviewer:{group}"));
    }

    if let Err(e) = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(db),
        crate::services::notify::Recipient::AnyCapability(capabilities),
        "design.version_submitted",
    )
    .arg("slice", slice.title.clone())
    .payload(serde_json::json!({ "slice_id": slice.id }))
    .execute()
    .await
    {
        tracing::warn!(slice_id = %slice.id, error = %e, "reviewer notification not delivered");
    }
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
    let blocking_reason = input
        .blocking_reason
        .map(str::trim)
        .filter(|s| !s.is_empty());

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
    .bind(state.published_artifact_url.as_deref())
    .bind(state.design_version_notes_md.as_deref())
    .bind(&input.grid_scores)
    .execute(&mut *tx)
    .await?;

    if input.verdict == Verdict::Approve {
        let designer = state.claimed_by_user_id.ok_or_else(|| {
            AppError::Validation("a design challenge nobody claimed cannot be validated".into())
        })?;
        // The literal is cast: `round` is SMALLINT, and an uncast `1` makes
        // PostgreSQL widen the whole COALESCE to INT4, which then refuses to
        // decode into `i16` — a 500 on the round that approves the work.
        let rounds: i16 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(round), 1::SMALLINT)
               FROM slice_validation_decisions WHERE slice_id = $1",
        )
        .bind(slice_id)
        .fetch_one(&mut *tx)
        .await?;

        let deliverable_id =
            record_deliverable(&mut tx, &state, slice_id, designer, reviewer_id).await?;
        tx.commit().await?;

        // Outside the transaction: an attestation that fails to write is a
        // re-runnable problem, whereas a half-written validation is not. The
        // proof — the deliverable — is already committed.
        if let (Some(deliverable_id), Some(url)) =
            (deliverable_id, state.published_artifact_url.as_deref())
        {
            crate::services::design_attestations::deliverable_validated(
                db,
                designer,
                deliverable_id,
                &state.title,
                url,
                rounds,
                state.design_subtype.as_deref(),
            )
            .await?;
        }

        // The brief's author, if this slice came from one. Their reward for
        // setting work is the only signal that separates a good brief from a
        // plausible one, and it can only be known now.
        if let Err(e) =
            crate::services::design_briefs::reward_author_on_first_validation(db, slice_id).await
        {
            tracing::warn!(%slice_id, error = %e, "brief author not rewarded");
        }

        notify_designer(
            db,
            designer,
            "design.validated",
            slice_id,
            &state.title,
            Some(("rounds", rounds.to_string())),
        )
        .await;

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

    // A critique nobody reads is a round nobody takes. The designer cannot
    // act until they have read it, and the reviewer cannot finish until the
    // designer does — so both verdicts travel, and `iterate` is the one that
    // buzzes.
    if let Some(designer) = state.claimed_by_user_id {
        let kind = match input.verdict {
            Verdict::Iterate => "design.iteration_requested",
            Verdict::Reject => "design.rejected",
            // Handled above, with the round count.
            Verdict::Approve => return Ok(slice),
        };
        notify_designer(db, designer, kind, slice_id, &state.title, None).await;
    }

    Ok(slice)
}

/// Tell a designer what happened to their version.
///
/// Failures are logged and swallowed: the critique is committed, and a
/// notification that could not be delivered must not turn a recorded review
/// into a 500 that invites the reviewer to write it again.
async fn notify_designer(
    db: &PgPool,
    designer_id: Uuid,
    kind: &str,
    slice_id: Uuid,
    title: &str,
    extra: Option<(&str, String)>,
) {
    let mut builder = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(db),
        crate::services::notify::Recipient::User(designer_id),
        kind,
    )
    .arg("slice", title)
    .payload(serde_json::json!({ "slice_id": slice_id }));
    if let Some((name, value)) = extra {
        builder = builder.arg(name, value);
    }
    if let Err(e) = builder.execute().await {
        tracing::warn!(kind, %slice_id, error = %e, "design notification not delivered");
    }
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
        .published_artifact_url
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

// ═══════════════════════════════════════════════════════════════════
// Comparing two rounds
// ═══════════════════════════════════════════════════════════════════

/// One version, as it was when somebody reviewed it.
///
/// The URL comes from the decision row rather than from the slice: the slice
/// carries only the current version, and reading the trail from it would show
/// the same file at every round — the exact thing this endpoint exists to
/// disprove.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct VersionAtRound {
    pub round: i16,
    /// Where that version lived. NULL on rounds recorded before the trail
    /// snapshotted it, and on those the comparison is honestly unavailable
    /// rather than quietly wrong.
    pub artifact_url: Option<String>,
    /// What the designer said changed, written when the version was handed in.
    pub author_notes_md: Option<String>,
    pub decision: String,
    pub blocking_reason: Option<String>,
    /// The critique that closed this round.
    pub reason: Option<String>,
    pub grid_scores: Option<serde_json::Value>,
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

/// Two versions and everything said between them.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct Comparison {
    pub slice_id: Uuid,
    pub design_subtype: Option<String>,
    /// Which comparison is meaningful for this kind of artefact. The diff
    /// itself is computed by whoever has the pixels; this says which one to
    /// compute.
    pub diff_strategy: Option<String>,
    pub from: VersionAtRound,
    pub to: VersionAtRound,
    /// The critiques that ran between the two, in order — the reason the
    /// second version looks the way it does.
    pub critiques_between: Vec<ReviewRound>,
}

/// One reviewed version.
pub async fn version_at(
    db: &PgPool,
    slice_id: Uuid,
    round: i16,
) -> Result<VersionAtRound, AppError> {
    sqlx::query_as::<_, VersionAtRound>(
        r#"
        SELECT round,
               reviewed_artifact_url        AS artifact_url,
               reviewed_artifact_notes_md   AS author_notes_md,
               decision, blocking_reason, reason, grid_scores, decided_at
          FROM slice_validation_decisions
         WHERE slice_id = $1 AND round = $2
        "#,
    )
    .bind(slice_id)
    .bind(round)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("this slice has no round {round}")))
}

/// Round `from` against round `to`, with the critiques in between.
///
/// Refuses a comparison of a round with itself, and refuses `from` after
/// `to`: both are answerable, and both mean the caller built the request
/// wrong. Answering them would return something that looks like a result.
pub async fn compare(
    db: &PgPool,
    slice_id: Uuid,
    from: i16,
    to: i16,
) -> Result<Comparison, AppError> {
    if from >= to {
        return Err(AppError::Validation(
            "compare an earlier round to a later one: from must be before to".into(),
        ));
    }

    let earlier = version_at(db, slice_id, from).await?;
    let later = version_at(db, slice_id, to).await?;

    let subtype: Option<String> =
        sqlx::query_scalar("SELECT design_subtype FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_optional(db)
            .await?
            .flatten();

    let diff_strategy = subtype
        .as_deref()
        .and_then(crate::models::DesignSubtype::parse)
        .map(|s| s.diff_strategy().as_str().to_string());

    // The rounds strictly between the two: what was asked for, which is the
    // reason the later version differs. `from`'s own critique is included
    // because it is the one that produced `to`.
    let critiques_between = sqlx::query_as::<_, ReviewRound>(
        r#"
        SELECT round, decision, blocking_reason, reason,
               reviewed_artifact_url, reviewed_artifact_notes_md,
               grid_scores, decided_at
          FROM slice_validation_decisions
         WHERE slice_id = $1 AND round >= $2 AND round < $3
         ORDER BY round ASC
        "#,
    )
    .bind(slice_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;

    Ok(Comparison {
        slice_id,
        design_subtype: subtype,
        diff_strategy,
        from: earlier,
        to: later,
        critiques_between,
    })
}

/// A validated piece of work that took at least three rounds.
///
/// The most convincing thing on a design profile is not the final image, it
/// is the distance between the first version and the last one. A first
/// attempt that was approved immediately says less — so this reads only the
/// work that was argued about and still got there.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct IterationStory {
    pub slice_id: Uuid,
    pub title: String,
    pub design_subtype: Option<String>,
    pub orientation_slug: Option<String>,
    pub rounds: i64,
    pub first_artifact_url: Option<String>,
    pub final_artifact_url: Option<String>,
    pub validated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// How many rounds a piece of work has to have survived to tell a story.
///
/// Three. Two is one critique and a fix, which happens to everybody; three is
/// where a direction was questioned and the person came back.
pub const STORY_MIN_ROUNDS: i64 = 3;

pub async fn iteration_stories(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<IterationStory>, AppError> {
    let stories = sqlx::query_as::<_, IterationStory>(
        r#"
        SELECT s.id            AS slice_id,
               s.title,
               s.design_subtype,
               o.slug          AS orientation_slug,
               count(d.round)  AS rounds,
               (array_agg(d.reviewed_artifact_url ORDER BY d.round ASC)
                    FILTER (WHERE d.reviewed_artifact_url IS NOT NULL))[1]
                               AS first_artifact_url,
               (array_agg(d.reviewed_artifact_url ORDER BY d.round DESC)
                    FILTER (WHERE d.reviewed_artifact_url IS NOT NULL))[1]
                               AS final_artifact_url,
               max(d.decided_at) FILTER (WHERE d.decision = 'approve')
                               AS validated_at
          FROM project_slices s
          JOIN slice_validation_decisions d ON d.slice_id = s.id
          LEFT JOIN orientations o ON o.id = s.orientation_id
         WHERE s.claimed_by_user_id = $1
           AND s.slice_type = 'design_artifact'
           AND s.status = 'validated'
         GROUP BY s.id, s.title, s.design_subtype, o.slug
        HAVING count(d.round) >= $2
         ORDER BY max(d.decided_at) DESC
         LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(STORY_MIN_ROUNDS)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(stories)
}
