//! The quality domain: defect reports, imported test runs, cross-domain
//! routing.
//!
//! Two things a quality contributor produces that no other domain has a place
//! for, and one query the whole domain is organised around.
//!
//!   * a **defect report** carries reproduction, environment and severity as
//!     columns, so what makes it usable does not depend on how carefully it
//!     was written out — and it becomes a proof only when the fix has shipped
//!     and the reporter has gone back to look;
//!   * a **test run** is imported from whatever tool produced it, into one
//!     shape, because a reviewer needs the same four things from all five
//!     sources;
//!   * **who reviews what** is decided by the trade behind the slice, which is
//!     why the routing helper here reads the orientation rather than the
//!     subtype.
//!
//! ## The severity a reporter gives is not the severity that counts
//!
//! `severity` is what the reporter thought. `severity_adjusted_to` is what the
//! reviewer thought, and it is kept alongside rather than overwriting: a
//! pattern of over-rating is something a mentor should be able to see, and
//! overwriting would erase exactly that. The craft score reads the reviewed
//! figure and nothing else.
//!
//! ## Confirming a fix is the reporter's act
//!
//! Not the reviewer's, and not a webhook's. A merged pull request is somebody
//! else's claim that the defect is gone; the confirmation is the person who
//! found it having gone back and checked. That is why
//! [`confirm_fix`] refuses anybody but the reporter.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The five families of quality review. Mirrors `orientations.reviewer_group`.
pub const REVIEWER_GROUPS: &[&str] = &[
    "automation",
    "intrusion",
    "usability",
    "playtest",
    "strategy",
];

/// What a quality report can be. Mirrors migration 0450's CHECK.
pub const SUBTYPES: &[&str] = &[
    "test_plan",
    "test_automation",
    "bug_report",
    "usability_study",
    "a11y_audit",
    "playtest_report",
    "coverage_analysis",
    "test_strategy",
];

pub const SEVERITIES: &[&str] = &["critical", "high", "medium", "low"];

pub const REPRODUCIBILITIES: &[&str] = &["always", "often", "sometimes", "rare", "once"];

/// Where a test run can be imported from. Mirrors migration 0451's CHECK.
pub const RUN_SOURCES: &[&str] = &[
    "github_actions",
    "codecov",
    "junit_xml",
    "playwright",
    "cypress",
    "postman",
];

/// The shortest reproduction the database will accept, in characters.
///
/// Mirrors the CHECK on `quality_bug_reports.repro_steps_md`, so the refusal
/// is a validation message rather than a constraint violation the caller
/// cannot read.
const MIN_REPRO_LEN: usize = 40;

// ═══════════════════════════════════════════════════════════════════
// Defect reports
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BugReport {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub reporter_user_id: Uuid,
    pub title: String,
    pub repro_steps_md: String,
    pub expected_md: String,
    pub observed_md: String,
    pub environment: serde_json::Value,
    pub severity: String,
    pub reproducibility: String,
    pub attachment_urls: Vec<String>,
    pub fix_url: Option<String>,
    pub fix_confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub severity_adjusted_to: Option<String>,
    pub rejected_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const BUG_SELECT: &str = r#"
    SELECT id, slice_id, reporter_user_id, title, repro_steps_md, expected_md,
           observed_md, environment, severity, reproducibility, attachment_urls,
           fix_url, fix_confirmed_at, reviewed_at, severity_adjusted_to,
           rejected_reason, created_at
      FROM quality_bug_reports
"#;

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct BugReportInput {
    pub slice_id: Uuid,
    pub title: String,
    pub repro_steps_md: String,
    pub expected_md: String,
    pub observed_md: String,
    /// `{"os": …, "browser": …, "build": …}`. Free-form on purpose: the
    /// useful keys are not the same for a web application, a game build and a
    /// command-line tool.
    #[schema(value_type = Object)]
    pub environment: serde_json::Value,
    pub severity: String,
    pub reproducibility: String,
    #[serde(default)]
    pub attachment_urls: Vec<String>,
}

/// File a defect report against a slice of quality work.
///
/// The slice must be the caller's own and must be a `bug_report` slice — the
/// database enforces the second, and this enforces the first. Without the
/// ownership check somebody could file reports under another person's slice
/// and hand them the attestation.
pub async fn file_bug_report(
    db: &PgPool,
    user_id: Uuid,
    input: BugReportInput,
) -> Result<BugReport, AppError> {
    validate_bug_input(&input)?;

    let owns_slice: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM project_slices ps
             WHERE ps.id = $1
               AND (ps.claimed_by_user_id = $2 OR ps.created_by_user_id = $2)
        )
        "#,
    )
    .bind(input.slice_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !owns_slice {
        return Err(AppError::Validation(
            "a defect report is filed against a slice you hold — claim it first".into(),
        ));
    }

    let report: BugReport = sqlx::query_as(
        r#"
        INSERT INTO quality_bug_reports
            (slice_id, reporter_user_id, title, repro_steps_md, expected_md,
             observed_md, environment, severity, reproducibility, attachment_urls)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id, slice_id, reporter_user_id, title, repro_steps_md,
                  expected_md, observed_md, environment, severity,
                  reproducibility, attachment_urls, fix_url, fix_confirmed_at,
                  reviewed_at, severity_adjusted_to, rejected_reason, created_at
        "#,
    )
    .bind(input.slice_id)
    .bind(user_id)
    .bind(input.title.trim())
    .bind(input.repro_steps_md.trim())
    .bind(input.expected_md.trim())
    .bind(input.observed_md.trim())
    .bind(&input.environment)
    .bind(&input.severity)
    .bind(&input.reproducibility)
    .bind(&input.attachment_urls)
    .fetch_one(db)
    .await?;

    Ok(report)
}

/// Everything the input has to satisfy before it reaches the database.
///
/// The constraints exist in the schema too, and deliberately: this is the
/// half that produces a message somebody can act on, and the schema is the
/// half that holds when something writes around this function.
fn validate_bug_input(input: &BugReportInput) -> Result<(), AppError> {
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("a defect report needs a title".into()));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;

    if input.repro_steps_md.trim().chars().count() < MIN_REPRO_LEN {
        return Err(AppError::Validation(format!(
            "reproduction steps have to be at least {MIN_REPRO_LEN} characters — \
             a report a stranger cannot follow is not a report"
        )));
    }
    if input.expected_md.trim().is_empty() || input.observed_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a defect report states what was expected and what was observed, separately".into(),
        ));
    }
    if !input.environment.is_object() || input.environment.as_object().is_some_and(|o| o.is_empty())
    {
        return Err(AppError::Validation(
            "the environment has to say where this happened — a defect nobody can \
             situate cannot be reproduced by somebody who does not already share it"
                .into(),
        ));
    }
    if !SEVERITIES.contains(&input.severity.as_str()) {
        return Err(AppError::Validation(format!(
            "severity must be one of: {}",
            SEVERITIES.join(", ")
        )));
    }
    if !REPRODUCIBILITIES.contains(&input.reproducibility.as_str()) {
        return Err(AppError::Validation(format!(
            "reproducibility must be one of: {}",
            REPRODUCIBILITIES.join(", ")
        )));
    }
    for url in &input.attachment_urls {
        crate::validators::validate_url(url, "attachment_urls", 500)?;
    }
    Ok(())
}

/// Record where the fix landed.
///
/// Separate from confirming it, and that separation is the point: linking a
/// merged pull request is somebody else's claim, and it can be recorded by
/// anybody who can see the report. Saying the defect is gone is a different
/// act, done by a different person, in [`confirm_fix`].
pub async fn link_fix(
    db: &PgPool,
    user_id: Uuid,
    report_id: Uuid,
    fix_url: &str,
) -> Result<BugReport, AppError> {
    crate::validators::validate_url(fix_url, "fix_url", 500)?;
    if !fix_url.starts_with("https://") {
        return Err(AppError::Validation(
            "the fix has to be a public https link — a fix nobody can open proves nothing".into(),
        ));
    }

    let updated: Option<BugReport> = sqlx::query_as(
        r#"
        UPDATE quality_bug_reports
           SET fix_url = $3
         WHERE id = $1
           AND reporter_user_id = $2
           AND rejected_reason IS NULL
        RETURNING id, slice_id, reporter_user_id, title, repro_steps_md,
                  expected_md, observed_md, environment, severity,
                  reproducibility, attachment_urls, fix_url, fix_confirmed_at,
                  reviewed_at, severity_adjusted_to, rejected_reason, created_at
        "#,
    )
    .bind(report_id)
    .bind(user_id)
    .bind(fix_url)
    .fetch_optional(db)
    .await?;

    updated
        .ok_or_else(|| AppError::NotFound("no defect report of yours is open under that id".into()))
}

/// Confirm the defect is gone.
///
/// Only the reporter. A confirmation by anybody else is the claim being
/// re-stated by somebody with an interest in it being true, which is what the
/// column exists to distinguish from a merged pull request.
///
/// Idempotent: a second confirmation returns the row unchanged rather than
/// moving the timestamp, so a double-clicked button does not rewrite when the
/// check happened.
pub async fn confirm_fix(
    db: &PgPool,
    user_id: Uuid,
    report_id: Uuid,
) -> Result<BugReport, AppError> {
    let updated: Option<BugReport> = sqlx::query_as(
        r#"
        UPDATE quality_bug_reports
           SET fix_confirmed_at = COALESCE(fix_confirmed_at, NOW())
         WHERE id = $1
           AND reporter_user_id = $2
           AND fix_url IS NOT NULL
           AND rejected_reason IS NULL
        RETURNING id, slice_id, reporter_user_id, title, repro_steps_md,
                  expected_md, observed_md, environment, severity,
                  reproducibility, attachment_urls, fix_url, fix_confirmed_at,
                  reviewed_at, severity_adjusted_to, rejected_reason, created_at
        "#,
    )
    .bind(report_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    let report = updated.ok_or_else(|| {
        AppError::Validation(
            "confirming a fix needs a defect report of yours that names where the fix \
             landed, and that nobody has rejected"
                .into(),
        )
    })?;

    // The attestation this earns is issued by the proof orchestrator, which
    // re-reads the confirmation. Issuing here would put the write and the
    // proof in one transaction and make a feed failure roll back somebody's
    // confirmation.
    Ok(report)
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct ReviewDecision {
    /// `accept` or `reject`.
    pub decision: String,
    /// What the reviewer thought the severity was. Absent means they agreed
    /// with the reporter.
    #[serde(default)]
    pub severity_adjusted_to: Option<String>,
    /// Required on a rejection. A rejection with no reason is a refusal with
    /// no appeal, and the person who has to act on it cannot.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Record a reviewer's decision on a defect report.
///
/// The caller has already been checked against the trade behind the slice —
/// see [`reviewer_orientation_for_slice`]. This does not re-derive the
/// permission, it records the outcome.
pub async fn review_bug_report(
    db: &PgPool,
    reviewer_id: Uuid,
    report_id: Uuid,
    decision: ReviewDecision,
) -> Result<BugReport, AppError> {
    let rejecting = match decision.decision.as_str() {
        "accept" => false,
        "reject" => true,
        other => {
            return Err(AppError::Validation(format!(
                "decision must be accept or reject, not '{other}'"
            )));
        }
    };

    let reason = decision
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty());
    if rejecting && reason.is_none() {
        return Err(AppError::Validation(
            "a rejection says why — otherwise the person who has to act on it cannot".into(),
        ));
    }
    if let Some(sev) = decision.severity_adjusted_to.as_deref()
        && !SEVERITIES.contains(&sev)
    {
        return Err(AppError::Validation(format!(
            "severity must be one of: {}",
            SEVERITIES.join(", ")
        )));
    }

    let updated: Option<BugReport> = sqlx::query_as(
        r#"
        UPDATE quality_bug_reports
           SET reviewed_by = $2,
               reviewed_at = NOW(),
               severity_adjusted_to = $3,
               rejected_reason = $4
         WHERE id = $1
           AND reporter_user_id <> $2
        RETURNING id, slice_id, reporter_user_id, title, repro_steps_md,
                  expected_md, observed_md, environment, severity,
                  reproducibility, attachment_urls, fix_url, fix_confirmed_at,
                  reviewed_at, severity_adjusted_to, rejected_reason, created_at
        "#,
    )
    .bind(report_id)
    .bind(reviewer_id)
    .bind(decision.severity_adjusted_to.as_deref())
    .bind(if rejecting { reason } else { None })
    .fetch_optional(db)
    .await?;

    updated.ok_or_else(|| {
        AppError::NotFound(
            "no defect report under that id that somebody other than you filed".into(),
        )
    })
}

/// Every defect report this person filed.
pub async fn bug_reports_for(db: &PgPool, user_id: Uuid) -> Result<Vec<BugReport>, AppError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{BUG_SELECT} WHERE reporter_user_id = $1 ORDER BY created_at DESC LIMIT 200"
    )))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// What is waiting for a reviewer, worst first, oldest first.
pub async fn unreviewed_bug_reports(db: &PgPool) -> Result<Vec<BugReport>, AppError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{BUG_SELECT}
          WHERE reviewed_at IS NULL
          ORDER BY CASE severity
                     WHEN 'critical' THEN 0 WHEN 'high' THEN 1
                     WHEN 'medium' THEN 2 ELSE 3 END,
                   created_at
          LIMIT 100"
    )))
    .fetch_all(db)
    .await?)
}

/// Which trade a report has to be reviewed by.
///
/// Reads the orientation on the slice, not the subtype. The subtype says what
/// the artefact is; the orientation says which family of reviewer can judge
/// it, and they are not the same question — a defect report against a game
/// build and one against an API are both `bug_report`, and the two people who
/// can read them are different.
///
/// `None` when the slice carries no orientation. The caller refuses in that
/// case rather than picking one: routing an artefact to a reviewer chosen on
/// its behalf is how work ends up in a queue nobody reads.
pub async fn reviewer_orientation_for_slice(
    db: &PgPool,
    slice_id: Uuid,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT o.slug
          FROM project_slices ps
          JOIN orientations o ON o.id = ps.orientation_id
         WHERE ps.id = $1 AND o.primary_domain = 'quality'
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?)
}

/// The trade behind a defect report, for the permission check.
pub async fn reviewer_orientation_for_report(
    db: &PgPool,
    report_id: Uuid,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT o.slug
          FROM quality_bug_reports b
          JOIN project_slices ps ON ps.id = b.slice_id
          JOIN orientations o ON o.id = ps.orientation_id
         WHERE b.id = $1 AND o.primary_domain = 'quality'
        "#,
    )
    .bind(report_id)
    .fetch_optional(db)
    .await?)
}

// ═══════════════════════════════════════════════════════════════════
// Imported test runs
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TestRun {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub source: String,
    pub report_url: String,
    pub commit_sha: Option<String>,
    pub repository_url: Option<String>,
    pub tests_total: i32,
    pub tests_failed: i32,
    pub tests_skipped: i32,
    pub duration_seconds: Option<i32>,
    pub coverage_percent: Option<bigdecimal::BigDecimal>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub imported_at: chrono::DateTime<chrono::Utc>,
}

const RUN_SELECT: &str = r#"
    SELECT id, slice_id, source, report_url, commit_sha, repository_url,
           tests_total, tests_failed, tests_skipped, duration_seconds,
           coverage_percent, verified_at, imported_at
      FROM quality_test_runs
"#;

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct TestRunInput {
    pub slice_id: Uuid,
    pub source: String,
    pub report_url: String,
    #[serde(default)]
    pub commit_sha: Option<String>,
    #[serde(default)]
    pub repository_url: Option<String>,
    pub tests_total: i32,
    #[serde(default)]
    pub tests_failed: i32,
    #[serde(default)]
    pub tests_skipped: i32,
    #[serde(default)]
    pub duration_seconds: Option<i32>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub coverage_percent: Option<bigdecimal::BigDecimal>,
    /// Whatever else the source gave. Read by nothing; kept so a parser that
    /// improves later can re-derive a column without asking people to import
    /// again.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub raw_summary: Option<serde_json::Value>,
}

/// Record a test run against a slice.
///
/// Importing the same run twice updates the row rather than doubling the
/// figures: somebody re-importing after a parser fix is fixing their own data,
/// not reporting a second run.
pub async fn import_test_run(
    db: &PgPool,
    user_id: Uuid,
    input: TestRunInput,
) -> Result<TestRun, AppError> {
    if !RUN_SOURCES.contains(&input.source.as_str()) {
        return Err(AppError::Validation(format!(
            "source must be one of: {}",
            RUN_SOURCES.join(", ")
        )));
    }
    crate::validators::validate_url(&input.report_url, "report_url", 500)?;
    if !input.report_url.starts_with("https://") {
        return Err(AppError::Validation(
            "the report has to be a public https link — a figure with no source is \
             the claim this replaces, not a smaller version of it"
                .into(),
        ));
    }
    if let Some(url) = input.repository_url.as_deref() {
        crate::validators::validate_url(url, "repository_url", 500)?;
    }
    if input.tests_total < 0 || input.tests_failed < 0 || input.tests_skipped < 0 {
        return Err(AppError::Validation(
            "a test run cannot have a negative count".into(),
        ));
    }
    if input.tests_failed + input.tests_skipped > input.tests_total {
        return Err(AppError::Validation(
            "a run cannot report more failures and skips than tests".into(),
        ));
    }

    let run: TestRun = sqlx::query_as(
        r#"
        INSERT INTO quality_test_runs
            (slice_id, imported_by, source, report_url, commit_sha,
             repository_url, tests_total, tests_failed, tests_skipped,
             duration_seconds, coverage_percent, raw_summary)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (slice_id, source, report_url) DO UPDATE
            SET tests_total = EXCLUDED.tests_total,
                tests_failed = EXCLUDED.tests_failed,
                tests_skipped = EXCLUDED.tests_skipped,
                duration_seconds = EXCLUDED.duration_seconds,
                coverage_percent = EXCLUDED.coverage_percent,
                commit_sha = EXCLUDED.commit_sha,
                repository_url = EXCLUDED.repository_url,
                raw_summary = EXCLUDED.raw_summary,
                -- A re-import is new data, so whatever a reviewer checked no
                -- longer describes what is in the row.
                verified_by = NULL,
                verified_at = NULL,
                imported_at = NOW()
        RETURNING id, slice_id, source, report_url, commit_sha, repository_url,
                  tests_total, tests_failed, tests_skipped, duration_seconds,
                  coverage_percent, verified_at, imported_at
        "#,
    )
    .bind(input.slice_id)
    .bind(user_id)
    .bind(&input.source)
    .bind(&input.report_url)
    .bind(input.commit_sha.as_deref())
    .bind(input.repository_url.as_deref())
    .bind(input.tests_total)
    .bind(input.tests_failed)
    .bind(input.tests_skipped)
    .bind(input.duration_seconds)
    .bind(input.coverage_percent.as_ref())
    .bind(input.raw_summary.as_ref())
    .fetch_one(db)
    .await?;

    Ok(run)
}

/// Mark a run as checked.
///
/// A reviewer opened the link, saw the run belongs to the work, and says the
/// figures are what the report says. Until then the row is somebody's import
/// and the score ignores it.
pub async fn verify_test_run(
    db: &PgPool,
    reviewer_id: Uuid,
    run_id: Uuid,
) -> Result<TestRun, AppError> {
    let updated: Option<TestRun> = sqlx::query_as(
        r#"
        UPDATE quality_test_runs
           SET verified_by = $2, verified_at = NOW()
         WHERE id = $1 AND imported_by <> $2
        RETURNING id, slice_id, source, report_url, commit_sha, repository_url,
                  tests_total, tests_failed, tests_skipped, duration_seconds,
                  coverage_percent, verified_at, imported_at
        "#,
    )
    .bind(run_id)
    .bind(reviewer_id)
    .fetch_optional(db)
    .await?;

    updated.ok_or_else(|| {
        AppError::NotFound("no test run under that id that somebody else imported".into())
    })
}

/// Every run imported against a slice.
pub async fn test_runs_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<TestRun>, AppError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{RUN_SELECT} WHERE slice_id = $1 ORDER BY imported_at DESC LIMIT 100"
    )))
    .bind(slice_id)
    .fetch_all(db)
    .await?)
}

// ═══════════════════════════════════════════════════════════════════
// Cross-domain routing
// ═══════════════════════════════════════════════════════════════════

/// Quality artefacts aimed at one domain, or at all of them.
///
/// The listing behind `GET /api/quality/reports?target_domain=game`. What the
/// backlog called cross-domain sub-tagging (W-05) is this query and the
/// breakdown on the profile; the column that makes both possible is migration
/// 0450's.
pub async fn reports_by_target_domain(
    db: &PgPool,
    target_domain: Option<&str>,
    limit: i64,
) -> Result<Vec<serde_json::Value>, AppError> {
    if let Some(domain) = target_domain {
        crate::validators::check_skill_domain(domain, "target_domain")?;
    }

    Ok(sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'slice_id', ps.id,
                   'title', ps.title,
                   'qa_subtype', ps.qa_subtype,
                   'target_domain', ps.target_domain,
                   'qa_tooling', ps.qa_tooling,
                   'orientation', o.slug,
                   'author', u.username,
                   'verified_at', d.verified_at)
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
          JOIN users u ON u.id = d.user_id
          LEFT JOIN orientations o ON o.id = ps.orientation_id
         WHERE ps.slice_type = 'qa_report'
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND ($1::VARCHAR IS NULL OR ps.target_domain = $1)
         ORDER BY d.verified_at DESC
         LIMIT $2
        "#,
    )
    .bind(target_domain)
    .bind(limit.clamp(1, 100))
    .fetch_all(db)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> BugReportInput {
        BugReportInput {
            slice_id: Uuid::nil(),
            title: "The invoice list loads empty for accounts with no orders".into(),
            repro_steps_md: "1. Sign in as a new account\n2. Open /invoices\n3. Observe".into(),
            expected_md: "An empty state explaining there are no invoices yet".into(),
            observed_md: "A spinner that never resolves".into(),
            environment: serde_json::json!({"os": "Windows 11", "browser": "Firefox 128"}),
            severity: "high".into(),
            reproducibility: "always".into(),
            attachment_urls: vec![],
        }
    }

    #[test]
    fn a_reproduction_too_short_to_follow_is_refused() {
        let mut input = valid_input();
        input.repro_steps_md = "it does not work".into();
        let err = validate_bug_input(&input).expect_err("must refuse");
        assert!(format!("{err:?}").contains("reproduction"));
    }

    #[test]
    fn an_environment_nobody_can_situate_is_refused() {
        // A defect whose environment is unknown cannot be reproduced by
        // somebody who does not already share it.
        let mut input = valid_input();
        input.environment = serde_json::json!({});
        assert!(validate_bug_input(&input).is_err());
        input.environment = serde_json::json!("Windows");
        assert!(validate_bug_input(&input).is_err());
    }

    #[test]
    fn a_severity_nothing_defines_is_refused() {
        let mut input = valid_input();
        input.severity = "catastrophic".into();
        assert!(validate_bug_input(&input).is_err());
    }

    #[test]
    fn reproducibility_is_not_severity() {
        // Two closed vocabularies, and swapping them is the mistake the pair
        // exists to make visible.
        let mut input = valid_input();
        input.reproducibility = "critical".into();
        assert!(validate_bug_input(&input).is_err());
    }

    #[test]
    fn a_complete_report_passes() {
        validate_bug_input(&valid_input()).expect("this one is fine");
    }

    #[test]
    fn every_subtype_the_schema_allows_is_listed_here() {
        // `SUBTYPES` is what the reference endpoint publishes. A subtype in
        // the schema and not here is one no client offers, so nobody files it.
        for subtype in SUBTYPES {
            assert!(
                crate::services::quality_attestations::basis_for_subtype(subtype).is_some(),
                "{subtype} is offered and earns nothing"
            );
        }
    }
}
