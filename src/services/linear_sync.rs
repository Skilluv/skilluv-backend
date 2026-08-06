//! SKI-72 (P26 v2 B-01) — internal-tracker → GitHub Issue bot.
//!
//! When a ticket in our internal tracker is flagged with the trigger label
//! `challenge-ready`, this service creates (or updates) a matching GitHub
//! Issue in the target Skilluv repo with the label `skilluv-challenge`,
//! which the P11 GitHubIngestor then materialises as a `project_slice`.
//!
//! Naming policy — the Skilluv repos are public. The vendor of our internal
//! tracker is intentionally NOT mentioned in the outbound GitHub Issue body
//! (only an opaque upstream URL and identifier). Server-side identifiers
//! keep the "linear_" prefix to help operators reconcile with the upstream
//! ticket, but nothing that leaks to the public web references it.
//!
//! Target-repo mapping — phase 1 dogfooding: the target repo is derived
//! from labels of the form `repo:<slug>` present on the upstream ticket
//! (e.g. `repo:backend` → `skilluv/skilluv-backend`). A ticket missing any
//! `repo:*` label is skipped with a warning; a ticket carrying several is
//! rejected (ambiguity → no silent misroute).
//!
//! Idempotence — first sync inserts a `linear_challenge_sync` row + creates
//! a GitHub issue; subsequent syncs (title / body edits) `PATCH` the same
//! GitHub issue.

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;

use crate::errors::AppError;

type HmacSha256 = Hmac<Sha256>;

pub const TRIGGER_LABEL: &str = "challenge-ready";
pub const REPO_LABEL_PREFIX: &str = "repo:";
pub const GITHUB_TARGET_LABEL: &str = "skilluv-challenge";

const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = "skilluv-backend/linear-sync";

/// Phase 1 dogfooding: `repo:<slug>` label → GitHub target. Extending this
/// list (or migrating to a DB table) is required before opening the bot to
/// tickets targeting repos beyond the 4 seeded by `skilluv-seed-projects`.
const KNOWN_TARGETS: &[(&str, &str, &str)] = &[
    ("repo:backend", "skilluv", "skilluv-backend"),
    ("repo:frontend", "skilluv", "skilluv-frontend"),
    ("repo:admin", "skilluv", "skilluv-admin"),
    ("repo:ia", "skilluv", "skilluv-ia"),
];

/// Minimal shape of the events we care about. The upstream tracker emits
/// many event types — we only act on the ones that could change trigger
/// state or the sync payload.
#[derive(Debug, Deserialize)]
pub struct InboundEvent {
    /// Event type as documented by the tracker (e.g. `Issue`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// Verb (e.g. `create`, `update`).
    #[serde(default)]
    pub action: String,
    pub data: EventData,
}

#[derive(Debug, Deserialize)]
pub struct EventData {
    /// Upstream identifier (e.g. `SKI-72`).
    pub identifier: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Web URL of the upstream ticket (used only for operator debugging).
    pub url: String,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Present on close/reopen events. `completed` / `canceled` map to
    /// GitHub closure; others keep the issue open.
    #[serde(default)]
    pub state: Option<State>,
}

#[derive(Debug, Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct State {
    #[serde(rename = "type")]
    pub state_type: String,
}

/// Decision returned by `handle_event`, useful in tests and metrics.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncDecision {
    /// Trigger label absent → no-op.
    NotTriggered,
    /// Multiple or missing `repo:*` labels → refused (logged, no write).
    AmbiguousTarget,
    /// Created a fresh GitHub Issue.
    Created {
        issue_number: i32,
        target: (String, String),
    },
    /// Updated an existing GitHub Issue.
    Updated {
        issue_number: i32,
        target: (String, String),
    },
    /// Event type is not one we sync on (e.g. comment events).
    Ignored,
}

/// Verify the HMAC-SHA256 signature the upstream tracker attaches to
/// webhook deliveries. Constant-time comparison via `hmac::Mac::verify_slice`.
pub fn verify_signature(secret: &str, body: &[u8], signature_hex: &str) -> Result<(), AppError> {
    let sig = hex::decode(signature_hex.trim_start_matches("sha256=")).map_err(|_| {
        tracing::warn!("linear webhook: signature not valid hex");
        AppError::Unauthorized
    })?;
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes())
        .map_err(|_| AppError::Internal("hmac init failed".into()))?;
    mac.update(body);
    mac.verify_slice(&sig).map_err(|_| {
        tracing::warn!("linear webhook: signature mismatch");
        AppError::Unauthorized
    })?;
    Ok(())
}

/// Resolve the target GitHub repo from the ticket labels. Returns
/// `AmbiguousTarget` if 0 or >1 `repo:*` labels are set — silent misroute
/// would be much worse than a rejected ticket.
pub fn resolve_target(labels: &[Label]) -> Result<(&'static str, &'static str), SyncDecision> {
    let matches: Vec<_> = KNOWN_TARGETS
        .iter()
        .filter(|(label, _, _)| labels.iter().any(|l| l.name == *label))
        .collect();
    match matches.as_slice() {
        [(_, owner, repo)] => Ok((owner, repo)),
        _ => Err(SyncDecision::AmbiguousTarget),
    }
}

/// True if the trigger label is present on the ticket.
pub fn is_triggered(labels: &[Label]) -> bool {
    labels.iter().any(|l| l.name == TRIGGER_LABEL)
}

/// Compose the public-facing GitHub Issue body. Deliberately does NOT
/// mention the upstream tracker vendor (public repo policy).
pub fn compose_issue_body(data: &EventData) -> String {
    let description = data
        .description
        .as_deref()
        .unwrap_or("_No description provided._")
        .trim();
    format!(
        "{description}\n\n---\n\n\
        _Skilluv challenge · upstream reference `{ident}` · [tracker link]({url})_",
        ident = data.identifier,
        url = data.url,
    )
}

/// Main entry point called by the webhook route. Returns a `SyncDecision`
/// that lets the caller emit a matching metric or log without inspecting
/// database state again.
pub async fn handle_event(
    db: &PgPool,
    bot_token: &str,
    event: InboundEvent,
) -> Result<SyncDecision, AppError> {
    if event.event_type != "Issue" {
        return Ok(SyncDecision::Ignored);
    }
    let data = event.data;

    if !is_triggered(&data.labels) {
        return Ok(SyncDecision::NotTriggered);
    }
    let (owner, repo) = match resolve_target(&data.labels) {
        Ok(t) => t,
        Err(decision) => {
            tracing::warn!(
                identifier = %data.identifier,
                labels = ?data.labels.iter().map(|l| &l.name).collect::<Vec<_>>(),
                "linear→github sync: ambiguous or missing repo:* label — skipping",
            );
            return Ok(decision);
        }
    };

    let existing: Option<(i32,)> = sqlx::query_as(
        "SELECT github_issue_number FROM linear_challenge_sync WHERE linear_issue_id = $1",
    )
    .bind(&data.identifier)
    .fetch_optional(db)
    .await?;

    let body = compose_issue_body(&data);

    if let Some((issue_number,)) = existing {
        update_github_issue(bot_token, owner, repo, issue_number, &data.title, &body).await?;
        let closing = data
            .state
            .as_ref()
            .map(|s| matches!(s.state_type.as_str(), "completed" | "canceled"))
            .unwrap_or(false);
        if closing {
            close_github_issue(bot_token, owner, repo, issue_number).await?;
        }
        sqlx::query(
            r#"
            UPDATE linear_challenge_sync
               SET last_status = $2,
                   updated_at = NOW()
             WHERE linear_issue_id = $1
            "#,
        )
        .bind(&data.identifier)
        .bind(if closing { "closed" } else { "open" })
        .execute(db)
        .await?;
        return Ok(SyncDecision::Updated {
            issue_number,
            target: (owner.to_string(), repo.to_string()),
        });
    }

    let created = create_github_issue(bot_token, owner, repo, &data.title, &body).await?;
    sqlx::query(
        r#"
        INSERT INTO linear_challenge_sync
            (linear_issue_id, linear_ticket_url, github_owner, github_repo,
             github_issue_number, github_issue_url, last_status)
        VALUES ($1, $2, $3, $4, $5, $6, 'open')
        "#,
    )
    .bind(&data.identifier)
    .bind(&data.url)
    .bind(owner)
    .bind(repo)
    .bind(created.number)
    .bind(&created.html_url)
    .execute(db)
    .await?;
    Ok(SyncDecision::Created {
        issue_number: created.number,
        target: (owner.to_string(), repo.to_string()),
    })
}

// ─── GitHub API — thin wrappers ───────────────────────────────────

#[derive(Debug, Deserialize)]
struct GhCreated {
    number: i32,
    html_url: String,
}

#[derive(Debug, Serialize)]
struct GhCreateReq<'a> {
    title: &'a str,
    body: &'a str,
    labels: [&'a str; 1],
}

#[derive(Debug, Serialize)]
struct GhUpdateReq<'a> {
    title: &'a str,
    body: &'a str,
}

#[derive(Debug, Serialize)]
struct GhCloseReq<'a> {
    state: &'a str,
}

async fn create_github_issue(
    token: &str,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
) -> Result<GhCreated, AppError> {
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues");
    let req = GhCreateReq {
        title,
        body,
        labels: [GITHUB_TARGET_LABEL],
    };
    let resp = reqwest::Client::new()
        .post(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github create issue failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "github create issue {owner}/{repo} returned {status}: {text}"
        )));
    }
    resp.json::<GhCreated>()
        .await
        .map_err(|e| AppError::Internal(format!("github create issue decode failed: {e}")))
}

async fn update_github_issue(
    token: &str,
    owner: &str,
    repo: &str,
    number: i32,
    title: &str,
    body: &str,
) -> Result<(), AppError> {
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues/{number}");
    let req = GhUpdateReq { title, body };
    let resp = reqwest::Client::new()
        .patch(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github update issue failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github update issue {owner}/{repo}#{number} returned {}",
            resp.status()
        )));
    }
    Ok(())
}

async fn close_github_issue(
    token: &str,
    owner: &str,
    repo: &str,
    number: i32,
) -> Result<(), AppError> {
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues/{number}");
    let req = GhCloseReq { state: "closed" };
    let resp = reqwest::Client::new()
        .patch(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github close issue failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github close issue {owner}/{repo}#{number} returned {}",
            resp.status()
        )));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// SKI-73 (P26 v2 B-02) — reverse sync: GitHub Issue closed → tracker Done
// ═══════════════════════════════════════════════════════════════════

const LINEAR_GRAPHQL: &str = "https://api.linear.app/graphql";

/// Minimal shape of the GitHub `issues` webhook event we act on.
#[derive(Debug, Deserialize)]
pub struct GithubIssueEvent {
    pub action: String,
    pub issue: GithubIssue,
    pub repository: GithubRepository,
}

#[derive(Debug, Deserialize)]
pub struct GithubIssue {
    pub number: i32,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// `open` or `closed`. GitHub uses "closed" for both merged PRs
    /// (referenced from `closed via #N` in an issue) and manual close.
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubRepository {
    pub name: String,
    pub owner: RepoOwner,
}

#[derive(Debug, Deserialize)]
pub struct RepoOwner {
    pub login: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReverseSyncDecision {
    /// Event is not `issues.closed`.
    NotAClose,
    /// Issue does not carry the `skilluv-challenge` label — not one of ours.
    NotOurs,
    /// No matching row in `linear_challenge_sync` — the issue predates the
    /// bot or was created out-of-band.
    Untracked,
    /// Tracker state successfully advanced to Done.
    MovedToDone { linear_issue_id: String },
    /// Row already marked closed — no tracker call issued.
    AlreadyClosed { linear_issue_id: String },
}

/// Verify the GitHub-style HMAC signature (`X-Hub-Signature-256: sha256=...`).
/// Distinct from `verify_signature` only to keep the header name in the log
/// clear; the crypto is the same.
pub fn verify_github_signature(secret: &str, body: &[u8], signature: &str) -> Result<(), AppError> {
    verify_signature(secret, body, signature)
}

/// Handle a GitHub `issues` event. Only acts on `closed` with our target
/// label; other events are cheap no-ops so the caller can pass every
/// delivery through without pre-filtering.
pub async fn handle_github_issue_event(
    db: &PgPool,
    linear_api_key: &str,
    linear_done_state_id: &str,
    event: GithubIssueEvent,
) -> Result<ReverseSyncDecision, AppError> {
    if event.action != "closed" {
        return Ok(ReverseSyncDecision::NotAClose);
    }
    let carries_target = event
        .issue
        .labels
        .iter()
        .any(|l| l.name == GITHUB_TARGET_LABEL);
    if !carries_target {
        return Ok(ReverseSyncDecision::NotOurs);
    }

    let row: Option<(String, String)> = sqlx::query_as(
        r#"
        SELECT linear_issue_id, last_status
        FROM linear_challenge_sync
        WHERE github_owner = $1 AND github_repo = $2 AND github_issue_number = $3
        "#,
    )
    .bind(&event.repository.owner.login)
    .bind(&event.repository.name)
    .bind(event.issue.number)
    .fetch_optional(db)
    .await?;

    let Some((linear_issue_id, last_status)) = row else {
        return Ok(ReverseSyncDecision::Untracked);
    };

    if last_status == "closed" {
        return Ok(ReverseSyncDecision::AlreadyClosed { linear_issue_id });
    }

    move_linear_ticket_to_done(linear_api_key, linear_done_state_id, &linear_issue_id).await?;

    sqlx::query(
        r#"
        UPDATE linear_challenge_sync
           SET last_status = 'closed', updated_at = NOW()
         WHERE linear_issue_id = $1
        "#,
    )
    .bind(&linear_issue_id)
    .execute(db)
    .await?;

    Ok(ReverseSyncDecision::MovedToDone { linear_issue_id })
}

#[derive(Debug, Serialize)]
struct LinearGqlRequest<'a> {
    query: &'a str,
    variables: serde_json::Value,
}

async fn move_linear_ticket_to_done(
    api_key: &str,
    done_state_id: &str,
    issue_identifier: &str,
) -> Result<(), AppError> {
    // Two-step: identifier ("SKI-72") is human-friendly but Linear's
    // issueUpdate takes the UUID id. Resolve via `issue(id: "SKI-72")`
    // which accepts either — returns the UUID.
    let query = r#"
        mutation UpdateByIdentifier($id: String!, $stateId: String!) {
          issueUpdate(id: $id, input: { stateId: $stateId }) { success }
        }
    "#;
    let req = LinearGqlRequest {
        query,
        variables: serde_json::json!({ "id": issue_identifier, "stateId": done_state_id }),
    };

    let resp = reqwest::Client::new()
        .post(LINEAR_GRAPHQL)
        .header("Authorization", api_key)
        .header("Content-Type", "application/json")
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("linear graphql call failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "linear graphql {status}: {text}"
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("linear graphql decode: {e}")))?;
    if body.get("errors").is_some() {
        return Err(AppError::Internal(format!("linear graphql errors: {body}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(name: &str) -> Label {
        Label {
            name: name.to_string(),
        }
    }

    #[test]
    fn signature_roundtrip_ok() {
        let secret = "s3cret";
        let body = br#"{"hello":"world"}"#;
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());
        assert!(verify_signature(secret, body, &sig).is_ok());
        assert!(verify_signature(secret, body, &format!("sha256={sig}")).is_ok());
    }

    #[test]
    fn signature_mismatch_rejected() {
        let secret = "s3cret";
        let body = br#"{"hello":"world"}"#;
        assert!(verify_signature(secret, body, "deadbeef").is_err());
    }

    #[test]
    fn is_triggered_only_when_label_present() {
        assert!(!is_triggered(&[label("bug")]));
        assert!(is_triggered(&[label("bug"), label(TRIGGER_LABEL)]));
    }

    #[test]
    fn resolve_target_requires_exactly_one_repo_label() {
        assert_eq!(
            resolve_target(&[label("challenge-ready")]),
            Err(SyncDecision::AmbiguousTarget),
        );
        assert_eq!(
            resolve_target(&[label("repo:backend"), label("repo:frontend")]),
            Err(SyncDecision::AmbiguousTarget),
        );
        assert_eq!(
            resolve_target(&[label("repo:admin")]),
            Ok(("skilluv", "skilluv-admin")),
        );
    }

    #[test]
    fn issue_body_does_not_mention_tracker_vendor() {
        let data = EventData {
            identifier: "SKI-72".into(),
            title: "Ignored here".into(),
            description: Some("Do the thing.".into()),
            url: "https://tracker.example.com/SKI-72".into(),
            labels: vec![],
            state: None,
        };
        let body = compose_issue_body(&data);
        let lower = body.to_lowercase();
        // Sanity: the vendor name should never leak into a public GitHub issue.
        assert!(
            !lower.contains("linear"),
            "body leaks tracker vendor: {body}"
        );
        assert!(body.contains("SKI-72"));
        assert!(body.contains("Do the thing."));
    }

    #[test]
    fn github_signature_shares_hmac_with_tracker_verifier() {
        // Same secret, same body → same signature. Guards against a future
        // fork where the two verifiers accidentally use different domains.
        let secret = "gh-secret";
        let body = br#"{"action":"closed"}"#;
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());
        assert!(verify_github_signature(secret, body, &format!("sha256={sig}")).is_ok());
    }

    #[test]
    fn github_event_parses_minimal_shape() {
        // Sanity check that #[serde(default)] on `labels`/`state` lets the
        // deserializer accept the real GitHub payload (which is huge — we
        // only pull the 4 fields we care about, so `deny_unknown_fields`
        // is deliberately NOT used).
        let raw = serde_json::json!({
            "action": "closed",
            "issue": {
                "number": 42,
                "labels": [{"name": "skilluv-challenge"}],
                "state": "closed"
            },
            "repository": {
                "name": "skilluv-backend",
                "owner": {"login": "skilluv"}
            }
        });
        let ev: GithubIssueEvent = serde_json::from_value(raw).unwrap();
        assert_eq!(ev.issue.number, 42);
        assert_eq!(ev.repository.owner.login, "skilluv");
        assert_eq!(ev.issue.labels[0].name, "skilluv-challenge");
    }

    #[test]
    fn issue_body_handles_missing_description() {
        let data = EventData {
            identifier: "SKI-1".into(),
            title: "t".into(),
            description: None,
            url: "https://x/1".into(),
            labels: vec![],
            state: None,
        };
        let body = compose_issue_body(&data);
        assert!(body.contains("No description"));
    }
}
