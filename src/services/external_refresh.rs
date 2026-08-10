//! P26 v2 SKI-111 — external repo refresh poller.
//!
//! Covers 4 gaps that only exist when the ingested slice lives on a
//! GitHub repo we don't control (no webhook installable, no `pull_request`
//! event, no `issues.closed` event):
//!
//!   G13 — upstream issue edited (title/body/labels) → our slice is stale
//!   G14 — upstream issue closed → our slice keeps status='open'/'claimed'
//!   G3  — upstream PR merged → validated slice never advances to `merged`
//!   G5  — upstream PR closed without merge → challenger's PR is dead but
//!         the slice stays `submitted`/`ci_green`/`pending_validation`
//!
//! The poller is a single-writer for each transition so a webhook and
//! a poll cannot double-apply: every UPDATE is scoped to the "from"
//! status the transition expects.
//!
//! ─── Rate-limit policy ─────────────────────────────────────────────
//!
//! Per tick: max `POLL_MAX_PER_TICK` slices, sorted by `updated_at ASC`
//! so the oldest get refreshed first. Every candidate hits 1-2 GitHub API
//! calls (issue GET, and PR GET if `submitted_pr_url` is set). With the
//! default `SKILLUV_BOT_GITHUB_TOKEN` (5000 req/h), this leaves plenty
//! of headroom for the other pollers (SKI-88 CI poll, P11 issue poll).
//!
//! ─── Metrics (SKI-112) ─────────────────────────────────────────────
//!
//! Four Prometheus counters cover the observable transitions:
//!   skilluv_external_refresh_body_updated_total
//!   skilluv_external_refresh_issue_closed_total
//!   skilluv_external_refresh_merge_awarded_total
//!   skilluv_external_refresh_pr_rejected_total

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = "skilluv-backend/external-refresh";

/// How long a slice must sit before we bother refreshing it. Prevents
/// competing with the initial ingestor on freshly-created rows.
pub const REFRESH_MIN_AGE_MINUTES: i64 = 5;

/// Cap slices inspected per tick. Bounds GitHub API traffic.
pub const POLL_MAX_PER_TICK: i64 = 50;

// ─── Decision layer (pure — testable without a DB) ────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshAction {
    /// Nothing changed upstream.
    NoOp,
    /// Body/labels/title changed upstream — re-apply the enricher.
    UpdateFields,
    /// Upstream issue is closed; slice was never claimed → mark `closed`.
    CloseSlice,
    /// PR merged upstream and slice sits in `validated` → advance to `merged`.
    AwardMerge,
    /// PR closed without merge; slice sits in submitted/ci_green/pending →
    /// rewind to `claimed` with a canned reason.
    RejectPr,
}

/// Snapshot of the slice fields we need to decide. Kept tiny so a single
/// SELECT per tick is enough.
#[derive(Debug, Clone)]
pub struct SliceSnapshot {
    pub id: Uuid,
    pub status: String,
    pub submitted_pr_url: Option<String>,
    pub upstream_title: String,
    pub upstream_body_len: usize,
    pub upstream_labels_signature: String,
    pub slice_updated_at: chrono::DateTime<chrono::Utc>,
}

/// Data pulled from upstream GitHub, normalised for the decision.
#[derive(Debug, Clone)]
pub struct UpstreamState {
    pub issue_state: String, // "open" | "closed"
    pub issue_title: String,
    pub issue_body_len: usize,
    pub issue_labels_signature: String,
    pub issue_updated_at: chrono::DateTime<chrono::Utc>,
    /// None if the slice has no `submitted_pr_url` (never went past `claimed`).
    pub pr_state: Option<PrState>,
}

#[derive(Debug, Clone)]
pub struct PrState {
    pub state: String, // "open" | "closed"
    pub merged: bool,
}

/// Decide the ONE action to apply for this slice. Precedence matters:
///   1. Merge/reject signals win over body edits (a merged PR is the
///      terminal outcome — no point re-applying enricher on a dead slice).
///   2. `close_slice` only fires when the slice is still claimable and
///      the upstream issue closed; if the challenger already submitted,
///      we let the PR lifecycle drive.
///   3. `update_fields` is the least urgent — pure freshness.
pub fn decide_action(slice: &SliceSnapshot, upstream: &UpstreamState) -> RefreshAction {
    if let Some(pr) = &upstream.pr_state {
        if pr.merged && slice.status == "validated" {
            return RefreshAction::AwardMerge;
        }
        if pr.state == "closed"
            && !pr.merged
            && matches!(
                slice.status.as_str(),
                "submitted" | "ci_green" | "pending_validation"
            )
        {
            return RefreshAction::RejectPr;
        }
    }

    if upstream.issue_state == "closed"
        && matches!(slice.status.as_str(), "open" | "draft" | "claimed")
    {
        return RefreshAction::CloseSlice;
    }

    let body_changed = upstream.issue_body_len != slice.upstream_body_len;
    let title_changed = upstream.issue_title != slice.upstream_title;
    let labels_changed = upstream.issue_labels_signature != slice.upstream_labels_signature;
    let is_newer = upstream.issue_updated_at > slice.slice_updated_at;
    if is_newer && (body_changed || title_changed || labels_changed) {
        return RefreshAction::UpdateFields;
    }

    RefreshAction::NoOp
}

/// Stable signature of a labels list (order-insensitive). Used both when
/// building `SliceSnapshot` from DB rows and `UpstreamState` from GitHub —
/// same routine on both sides so `labels_changed` can't false-positive
/// on ordering alone.
pub fn labels_signature(labels: &[String]) -> String {
    let mut sorted: Vec<String> = labels.iter().map(|s| s.to_lowercase()).collect();
    sorted.sort();
    sorted.join(",")
}

// ─── I/O layer ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GhIssue {
    state: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    labels: Vec<GhLabel>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct GhLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GhPr {
    state: String,
    #[serde(default)]
    merged: bool,
}

/// Row we pull from the DB per tick.
#[derive(Debug, sqlx::FromRow)]
struct Candidate {
    id: Uuid,
    project_id: Uuid,
    status: String,
    external_ref: String, // issue number as text
    title: String,
    description: String,
    submitted_pr_url: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
    external_metadata: sqlx::types::Json<serde_json::Value>,
    // Project-level fallback for the enricher on refresh.
    github_repo_owner: String,
    github_repo_name: String,
    default_domain: String,
}

/// Load the next batch. Only rows whose slice_type='github_issue' and
/// status is non-terminal (so we don't spam GitHub on merged/closed/expired
/// rows that will never change).
async fn pick_candidates(db: &PgPool) -> Result<Vec<Candidate>, AppError> {
    let rows = sqlx::query_as::<_, Candidate>(
        r#"
        SELECT s.id,
               s.project_id,
               s.status,
               s.external_ref,
               s.title,
               s.description,
               s.submitted_pr_url,
               s.updated_at,
               s.external_metadata,
               p.github_repo_owner AS "github_repo_owner!",
               p.github_repo_name  AS "github_repo_name!",
               COALESCE(p.skill_domains[1], 'code') AS default_domain
          FROM project_slices s
          JOIN projects p ON p.id = s.project_id
         WHERE s.slice_type = 'github_issue'
           AND s.status NOT IN ('merged','closed','expired')
           AND s.external_ref IS NOT NULL
           AND p.github_repo_owner IS NOT NULL
           AND p.github_repo_name IS NOT NULL
           AND p.archived_at IS NULL
           AND s.updated_at < NOW() - ($1 || ' minutes')::interval
         ORDER BY s.updated_at ASC
         LIMIT $2
        "#,
    )
    .bind(REFRESH_MIN_AGE_MINUTES.to_string())
    .bind(POLL_MAX_PER_TICK)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn fetch_upstream(
    token: &str,
    owner: &str,
    name: &str,
    issue_number: &str,
    pr_url: Option<&str>,
) -> Result<UpstreamState, AppError> {
    let client = reqwest::Client::new();

    let issue_url = format!("{GITHUB_API}/repos/{owner}/{name}/issues/{issue_number}");
    let issue: GhIssue = client
        .get(&issue_url)
        .bearer_auth(token)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("refresh issue fetch: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("refresh issue status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("refresh issue decode: {e}")))?;

    let pr_state = if let Some(url) = pr_url {
        let (o, n, num) = match parse_pr_url(url) {
            Some(t) => t,
            None => return Err(AppError::Internal(format!("bad pr_url: {url}"))),
        };
        let pr_endpoint = format!("{GITHUB_API}/repos/{o}/{n}/pulls/{num}");
        let pr: GhPr = client
            .get(&pr_endpoint)
            .bearer_auth(token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("refresh pr fetch: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("refresh pr status: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("refresh pr decode: {e}")))?;
        Some(PrState {
            state: pr.state,
            merged: pr.merged,
        })
    } else {
        None
    };

    Ok(UpstreamState {
        issue_state: issue.state,
        issue_title: issue.title,
        issue_body_len: issue.body.as_deref().unwrap_or("").len(),
        issue_labels_signature: labels_signature(
            &issue
                .labels
                .iter()
                .map(|l| l.name.clone())
                .collect::<Vec<_>>(),
        ),
        issue_updated_at: issue.updated_at,
        pr_state,
    })
}

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

async fn apply_action(
    db: &PgPool,
    slice: &Candidate,
    upstream: &UpstreamState,
    action: RefreshAction,
) -> Result<(), AppError> {
    match action {
        RefreshAction::NoOp => Ok(()),
        RefreshAction::CloseSlice => {
            let updated = sqlx::query(
                r#"
                UPDATE project_slices
                   SET status = 'closed', closed_at = NOW(), updated_at = NOW()
                 WHERE id = $1 AND status IN ('open','draft','claimed')
                "#,
            )
            .bind(slice.id)
            .execute(db)
            .await?
            .rows_affected();
            if updated > 0 {
                metrics::counter!("skilluv_external_refresh_issue_closed_total").increment(1);
                tracing::info!(slice_id = %slice.id, "SKI-111 refresh: issue closed upstream");
            }
            Ok(())
        }
        RefreshAction::AwardMerge => {
            let updated = sqlx::query(
                r#"
                UPDATE project_slices
                   SET status = 'merged', updated_at = NOW()
                 WHERE id = $1 AND status = 'validated'
                "#,
            )
            .bind(slice.id)
            .execute(db)
            .await?
            .rows_affected();
            if updated > 0 {
                metrics::counter!("skilluv_external_refresh_merge_awarded_total").increment(1);
                tracing::info!(slice_id = %slice.id, "SKI-111 refresh: merge bonus awarded");
            }
            Ok(())
        }
        RefreshAction::RejectPr => {
            let updated = sqlx::query(
                r#"
                UPDATE project_slices
                   SET status = 'claimed',
                       picked_by_validator_id = NULL,
                       picked_at = NULL,
                       validation_reject_reason = 'PR closed upstream without merge',
                       updated_at = NOW()
                 WHERE id = $1
                   AND status IN ('submitted','ci_green','pending_validation')
                "#,
            )
            .bind(slice.id)
            .execute(db)
            .await?
            .rows_affected();
            if updated > 0 {
                metrics::counter!("skilluv_external_refresh_pr_rejected_total").increment(1);
                tracing::info!(slice_id = %slice.id, "SKI-111 refresh: PR closed no-merge");
            }
            Ok(())
        }
        RefreshAction::UpdateFields => {
            // Re-fetch full body from GitHub (we only pulled length in
            // `fetch_upstream` to keep the decision path cheap). Simpler
            // to re-run the same GET here than to plumb the body through
            // 3 layers just to save one call every few minutes.
            let client = reqwest::Client::new();
            let url = format!(
                "{GITHUB_API}/repos/{}/{}/issues/{}",
                slice.github_repo_owner, slice.github_repo_name, slice.external_ref
            );
            let issue: GhIssue = client
                .get(&url)
                .bearer_auth(std::env::var("SKILLUV_BOT_GITHUB_TOKEN").unwrap_or_default())
                .header("User-Agent", USER_AGENT)
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("refresh full fetch: {e}")))?
                .error_for_status()
                .map_err(|e| AppError::Internal(format!("refresh full status: {e}")))?
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("refresh full decode: {e}")))?;

            let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();
            let enriched = crate::services::slice_enrichment::enrich_from_issue(
                &labels,
                issue.body.as_deref(),
                &slice.default_domain,
            );

            let title_trunc = truncate(&issue.title, 300);
            let desc_trunc = truncate(issue.body.as_deref().unwrap_or("(no description)"), 4000);

            // Preserve the ingestion audit trail: only mutate the
            // signature portion of external_metadata.
            let mut md = slice.external_metadata.0.clone();
            if let Some(obj) = md.as_object_mut() {
                obj.insert("labels".to_string(), serde_json::json!(labels));
                let now = upstream.issue_updated_at.to_rfc3339();
                obj.insert("refreshed_at".to_string(), serde_json::json!(now));
            }

            let updated = sqlx::query(
                r#"
                UPDATE project_slices
                   SET title = $2,
                       description = $3,
                       acceptance_criteria = $4,
                       primary_domain = $5,
                       difficulty = $6,
                       external_metadata = $7,
                       updated_at = NOW()
                 WHERE id = $1
                   AND status NOT IN ('merged','closed','expired')
                "#,
            )
            .bind(slice.id)
            .bind(&title_trunc)
            .bind(&desc_trunc)
            .bind(&enriched.acceptance_criteria)
            .bind(&enriched.primary_domain)
            .bind(enriched.difficulty)
            .bind(&md)
            .execute(db)
            .await?
            .rows_affected();
            if updated > 0 {
                metrics::counter!("skilluv_external_refresh_body_updated_total").increment(1);
                tracing::debug!(slice_id = %slice.id, "SKI-111 refresh: fields updated");
            }
            Ok(())
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let cut = s
        .char_indices()
        .take_while(|(i, _)| *i < max.saturating_sub(1))
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max);
    format!("{}…", &s[..cut])
}

/// One tick. Returns the number of slices that had a non-NoOp action applied.
pub async fn refresh_once(db: &PgPool, bot_token: &str) -> Result<usize, AppError> {
    let candidates = pick_candidates(db).await?;
    let mut applied = 0usize;
    for c in candidates {
        let snapshot = SliceSnapshot {
            id: c.id,
            status: c.status.clone(),
            submitted_pr_url: c.submitted_pr_url.clone(),
            upstream_title: c.title.clone(),
            upstream_body_len: c.description.len(),
            upstream_labels_signature: labels_signature(
                &c.external_metadata
                    .0
                    .get("labels")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            ),
            slice_updated_at: c.updated_at,
        };

        let upstream = match fetch_upstream(
            bot_token,
            &c.github_repo_owner,
            &c.github_repo_name,
            &c.external_ref,
            c.submitted_pr_url.as_deref(),
        )
        .await
        {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(slice_id = %c.id, error = %e, "SKI-111 refresh fetch failed");
                continue;
            }
        };

        let action = decide_action(&snapshot, &upstream);
        if action != RefreshAction::NoOp
            && let Err(e) = apply_action(db, &c, &upstream, action.clone()).await
        {
            tracing::warn!(slice_id = %c.id, action = ?action, error = %e, "SKI-111 apply failed");
            continue;
        }
        if action != RefreshAction::NoOp {
            applied += 1;
        }
        // Slot Ignored project_id use — keep field to prove intent for
        // future per-project rate limiting; suppress dead_code warning.
        let _ = c.project_id;
    }
    Ok(applied)
}

/// Spawn the poller. Runs every 10 minutes. No-op silently when
/// `SKILLUV_BOT_GITHUB_TOKEN` is unset (matches SKI-88 convention).
pub fn start_external_refresh_task(db: PgPool) {
    tokio::spawn(async move {
        let Ok(token) = std::env::var("SKILLUV_BOT_GITHUB_TOKEN") else {
            tracing::info!("SKI-111 external refresh disabled: SKILLUV_BOT_GITHUB_TOKEN not set");
            return;
        };
        loop {
            match refresh_once(&db, &token).await {
                Ok(n) if n > 0 => {
                    tracing::info!(applied = n, "SKI-111 external refresh tick");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "SKI-111 tick failed"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn base_snapshot() -> SliceSnapshot {
        SliceSnapshot {
            id: Uuid::nil(),
            status: "open".into(),
            submitted_pr_url: None,
            upstream_title: "title".into(),
            upstream_body_len: 100,
            upstream_labels_signature: labels_signature(&["bug".into()]),
            slice_updated_at: now(),
        }
    }

    fn base_upstream() -> UpstreamState {
        UpstreamState {
            issue_state: "open".into(),
            issue_title: "title".into(),
            issue_body_len: 100,
            issue_labels_signature: labels_signature(&["bug".into()]),
            issue_updated_at: now(),
            pr_state: None,
        }
    }

    #[test]
    fn no_change_returns_noop() {
        assert_eq!(
            decide_action(&base_snapshot(), &base_upstream()),
            RefreshAction::NoOp,
        );
    }

    #[test]
    fn upstream_body_change_triggers_update() {
        let mut up = base_upstream();
        up.issue_body_len = 200;
        up.issue_updated_at = now() + chrono::Duration::minutes(5);
        assert_eq!(
            decide_action(&base_snapshot(), &up),
            RefreshAction::UpdateFields,
        );
    }

    #[test]
    fn labels_reorder_alone_is_not_a_change() {
        let mut snap = base_snapshot();
        snap.upstream_labels_signature = labels_signature(&["bug".into(), "help".into()]);
        let mut up = base_upstream();
        up.issue_labels_signature = labels_signature(&["help".into(), "bug".into()]);
        up.issue_updated_at = now() + chrono::Duration::minutes(5);
        // Sorted labels ⇒ same signature ⇒ NoOp (only ordering changed).
        assert_eq!(decide_action(&snap, &up), RefreshAction::NoOp);
    }

    #[test]
    fn upstream_issue_closed_while_open_marks_slice_closed() {
        let mut up = base_upstream();
        up.issue_state = "closed".into();
        assert_eq!(
            decide_action(&base_snapshot(), &up),
            RefreshAction::CloseSlice,
        );
    }

    #[test]
    fn upstream_issue_closed_after_claim_does_not_close_slice() {
        // Once claimed and the challenger has a PR, the PR lifecycle
        // drives — don't close the slice just because the issue closed
        // (maintainer often closes the issue via "closes #N" in the PR).
        let mut snap = base_snapshot();
        snap.status = "submitted".into();
        snap.submitted_pr_url = Some("https://github.com/o/r/pull/1".into());
        let mut up = base_upstream();
        up.issue_state = "closed".into();
        up.pr_state = Some(PrState {
            state: "open".into(),
            merged: false,
        });
        assert_eq!(decide_action(&snap, &up), RefreshAction::NoOp);
    }

    #[test]
    fn pr_merged_awards_when_validated() {
        let mut snap = base_snapshot();
        snap.status = "validated".into();
        snap.submitted_pr_url = Some("x".into());
        let mut up = base_upstream();
        up.pr_state = Some(PrState {
            state: "closed".into(),
            merged: true,
        });
        assert_eq!(decide_action(&snap, &up), RefreshAction::AwardMerge);
    }

    #[test]
    fn pr_merged_does_not_award_when_not_validated() {
        // A merge without Skilluv validation ≠ Skilluv success.
        let mut snap = base_snapshot();
        snap.status = "submitted".into();
        snap.submitted_pr_url = Some("x".into());
        let mut up = base_upstream();
        up.pr_state = Some(PrState {
            state: "closed".into(),
            merged: true,
        });
        assert_eq!(decide_action(&snap, &up), RefreshAction::NoOp);
    }

    #[test]
    fn pr_closed_without_merge_rewinds_to_claimed() {
        for from in ["submitted", "ci_green", "pending_validation"] {
            let mut snap = base_snapshot();
            snap.status = from.into();
            snap.submitted_pr_url = Some("x".into());
            let mut up = base_upstream();
            up.pr_state = Some(PrState {
                state: "closed".into(),
                merged: false,
            });
            assert_eq!(
                decide_action(&snap, &up),
                RefreshAction::RejectPr,
                "from={from}"
            );
        }
    }

    #[test]
    fn merge_takes_precedence_over_body_change() {
        // If the PR merged AND the body was edited in the same window,
        // we honor the terminal state — don't waste an UPDATE on fields.
        let mut snap = base_snapshot();
        snap.status = "validated".into();
        snap.submitted_pr_url = Some("x".into());
        let mut up = base_upstream();
        up.issue_body_len = 999;
        up.issue_updated_at = now() + chrono::Duration::minutes(5);
        up.pr_state = Some(PrState {
            state: "closed".into(),
            merged: true,
        });
        assert_eq!(decide_action(&snap, &up), RefreshAction::AwardMerge);
    }

    #[test]
    fn labels_signature_stable_regardless_of_case_and_order() {
        assert_eq!(
            labels_signature(&["Bug".into(), "help".into()]),
            labels_signature(&["help".into(), "BUG".into()]),
        );
    }
}
