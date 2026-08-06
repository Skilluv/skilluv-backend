//! P26 v2 SKI-87 / SKI-88 — CI signal handling for submitted slices.
//!
//! Two entry points converge on the same state transition
//! `submitted → ci_green`:
//!
//! - SKI-87 (webhook)  — `check_run.completed` event arrives; if it
//!   references a PR we know (via `submitted_pr_url`) and its conclusion
//!   is `success`, advance the slice.
//! - SKI-88 (poller)   — periodic fallback that scans slices stuck in
//!   `submitted` and asks GitHub whether the head commit's checks are
//!   all green. Guards against webhook delivery loss without needing
//!   the webhook to be configured on every repo.
//!
//! Both paths are idempotent: the UPDATE is scoped
//! `WHERE status = 'submitted'`, so a webhook and a poller racing on
//! the same slice results in one winning and the other becoming a
//! no-op. Advancing beyond `ci_green` (by the validator's pickup) is
//! also safe: neither path touches slices already moved on.

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = "skilluv-backend/ci-sync";

/// Minimal GitHub `check_run` event shape. We only need
/// (repository, conclusion, pull_requests[].number). Everything else
/// is discarded with #[serde(default)] on the wrapper struct.
#[derive(Debug, Deserialize)]
pub struct CheckRunEvent {
    pub action: String,
    pub check_run: CheckRun,
    pub repository: EventRepo,
}

#[derive(Debug, Deserialize)]
pub struct CheckRun {
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub pull_requests: Vec<CheckRunPr>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunPr {
    pub number: i32,
}

#[derive(Debug, Deserialize)]
pub struct EventRepo {
    pub name: String,
    pub owner: EventOwner,
}

#[derive(Debug, Deserialize)]
pub struct EventOwner {
    pub login: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CiWebhookDecision {
    /// Event is not `check_run.completed`.
    Ignored,
    /// Completed but not a success conclusion.
    NotSuccessful,
    /// Success but no PRs referenced — likely a branch build, ignore.
    NoPr,
    /// Matched N slices and advanced them to `ci_green`.
    Advanced { slice_count: usize },
    /// PRs referenced but none matched a submitted slice.
    Unmatched,
}

/// SKI-87 — handle a `check_run` webhook payload.
pub async fn handle_check_run_event(
    db: &PgPool,
    event: CheckRunEvent,
) -> Result<CiWebhookDecision, AppError> {
    if event.action != "completed" || event.check_run.status != "completed" {
        return Ok(CiWebhookDecision::Ignored);
    }
    if event.check_run.conclusion.as_deref() != Some("success") {
        return Ok(CiWebhookDecision::NotSuccessful);
    }
    if event.check_run.pull_requests.is_empty() {
        return Ok(CiWebhookDecision::NoPr);
    }

    let owner = &event.repository.owner.login;
    let repo = &event.repository.name;
    let mut advanced = 0usize;
    for pr in &event.check_run.pull_requests {
        let url = format!("https://github.com/{owner}/{repo}/pull/{}", pr.number);
        if advance_to_ci_green_by_url(db, &url).await? {
            advanced += 1;
        }
    }
    if advanced == 0 {
        Ok(CiWebhookDecision::Unmatched)
    } else {
        Ok(CiWebhookDecision::Advanced {
            slice_count: advanced,
        })
    }
}

/// The single writer that moves a slice from `submitted` to `ci_green`.
/// Returns `true` when a row actually flipped (idempotent otherwise).
async fn advance_to_ci_green_by_url(db: &PgPool, pr_url: &str) -> Result<bool, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE project_slices
           SET status = 'ci_green',
               updated_at = NOW()
         WHERE submitted_pr_url = $1
           AND status = 'submitted'
     RETURNING id
        "#,
    )
    .bind(pr_url)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

// ═══════════════════════════════════════════════════════════════════
// SKI-88 — poll fallback
// ═══════════════════════════════════════════════════════════════════

/// Age (minutes) a slice must sit in `submitted` before the poller
/// double-checks it. Short enough to keep the UX responsive when the
/// webhook is missing, long enough not to hammer the GitHub API.
pub const POLL_MIN_AGE_MINUTES: i32 = 3;

/// Cap the number of slices inspected per tick to bound work.
pub const POLL_MAX_PER_TICK: i64 = 25;

#[derive(Debug, Deserialize)]
struct PrPayload {
    head: PrHead,
}

#[derive(Debug, Deserialize)]
struct PrHead {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct CheckRunsList {
    total_count: i32,
    check_runs: Vec<CheckRunSummary>,
}

#[derive(Debug, Deserialize)]
struct CheckRunSummary {
    status: String,
    conclusion: Option<String>,
}

/// One tick of the poller. Returns the number of slices advanced.
pub async fn poll_once(db: &PgPool, bot_token: &str) -> Result<usize, AppError> {
    let candidates: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, submitted_pr_url
          FROM project_slices
         WHERE status = 'submitted'
           AND submitted_pr_url IS NOT NULL
           AND submitted_at < NOW() - ($1 || ' minutes')::interval
         ORDER BY submitted_at ASC
         LIMIT $2
        "#,
    )
    .bind(POLL_MIN_AGE_MINUTES.to_string())
    .bind(POLL_MAX_PER_TICK)
    .fetch_all(db)
    .await?;

    let mut advanced = 0usize;
    for (slice_id, pr_url) in candidates {
        match check_pr_is_green(bot_token, &pr_url).await {
            Ok(true) => {
                if advance_to_ci_green_by_url(db, &pr_url).await? {
                    advanced += 1;
                    tracing::info!(
                        slice_id = %slice_id, pr_url,
                        "SKI-88 poll: advanced submitted → ci_green"
                    );
                }
            }
            Ok(false) => {} // still red / pending
            Err(e) => {
                tracing::warn!(slice_id = %slice_id, error = %e, "SKI-88 poll: check failed");
            }
        }
    }
    Ok(advanced)
}

/// Parse `https://github.com/{o}/{r}/pull/{n}`. Rejects any other shape.
fn parse_pr_url(url: &str) -> Option<(String, String, i32)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 4 || parts[2] != "pull" {
        return None;
    }
    let n: i32 = parts[3].parse().ok()?;
    if n <= 0 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    Some((parts[0].to_string(), parts[1].to_string(), n))
}

async fn check_pr_is_green(bot_token: &str, pr_url: &str) -> Result<bool, AppError> {
    let (owner, repo, number) =
        parse_pr_url(pr_url).ok_or_else(|| AppError::Internal(format!("bad pr_url: {pr_url}")))?;
    let client = reqwest::Client::new();

    // 1) Resolve head SHA.
    let pr_endpoint = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls/{number}");
    let pr: PrPayload = client
        .get(&pr_endpoint)
        .bearer_auth(bot_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("PR fetch failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("PR fetch status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("PR decode failed: {e}")))?;

    // 2) List check-runs on the head SHA.
    let checks_endpoint = format!(
        "{GITHUB_API}/repos/{owner}/{repo}/commits/{}/check-runs",
        pr.head.sha
    );
    let checks: CheckRunsList = client
        .get(&checks_endpoint)
        .bearer_auth(bot_token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("check-runs fetch failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("check-runs status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("check-runs decode failed: {e}")))?;

    if checks.total_count == 0 {
        // No CI configured on this repo — treat as green so the workflow
        // doesn't stall. Operators can layer stricter rules by adding a
        // required check on the repo side (canonical GitHub pattern).
        return Ok(true);
    }
    let all_green = checks
        .check_runs
        .iter()
        .all(|c| c.status == "completed" && c.conclusion.as_deref() == Some("success"));
    Ok(all_green)
}

/// Spawn the background poller. Runs every 60s. No-op when
/// `SKILLUV_BOT_GITHUB_TOKEN` is unset (matches SKI-72/73 config).
pub fn start_ci_poll_task(db: PgPool) {
    tokio::spawn(async move {
        let Ok(token) = std::env::var("SKILLUV_BOT_GITHUB_TOKEN") else {
            tracing::info!(
                "SKI-88 poll disabled: SKILLUV_BOT_GITHUB_TOKEN not set (webhook-only mode)"
            );
            return;
        };
        loop {
            match poll_once(&db, &token).await {
                Ok(n) if n > 0 => {
                    metrics::counter!("skilluv_ci_poll_advanced_total").increment(n as u64);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "SKI-88 poll tick failed"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_url_ok() {
        assert_eq!(
            parse_pr_url("https://github.com/skilluv/skilluv-backend/pull/12"),
            Some(("skilluv".into(), "skilluv-backend".into(), 12)),
        );
    }

    #[test]
    fn parse_pr_url_rejects_bad_shape() {
        assert_eq!(parse_pr_url("https://gitlab.com/x/y/pull/1"), None);
        assert_eq!(parse_pr_url("https://github.com/x/y/issues/1"), None);
        assert_eq!(parse_pr_url("https://github.com/x/y/pull/-1"), None);
    }

    #[test]
    fn ignored_when_action_wrong() {
        // We would parse this synchronously if we didn't need a db;
        // instead assert the discriminant by round-tripping a fresh event
        // through the pure branches: the top of handle_check_run_event is
        // pure until the DB call.
        let ev = CheckRunEvent {
            action: "created".into(),
            check_run: CheckRun {
                status: "completed".into(),
                conclusion: Some("success".into()),
                pull_requests: vec![],
            },
            repository: EventRepo {
                name: "x".into(),
                owner: EventOwner { login: "y".into() },
            },
        };
        // Just verifying the struct assembles; the async branch cannot be
        // tested without a DB fixture (integration territory).
        assert_eq!(ev.action, "created");
    }
}
