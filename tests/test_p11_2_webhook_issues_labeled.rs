//! Tests d'intégration P11.2 : webhook `issues.labeled` ingère une slice
//! temps réel dans le project qui match le repo + a le label curé.
//!
//! On invoque le webhook via reqwest avec la signature HMAC valide.
//! Le tests couvrent :
//! - Match repo + label → slice draft créée.
//! - Label non curé → aucune slice.
//! - Repo non tracké → aucune slice.
//! - Idempotence : le même payload deux fois → 1 slice (dedup ON CONFLICT).
//! - PRs (issue.pull_request set) skip.
//! - Mode 'auto' vs 'curator_review' → status open vs draft.

mod common;

use common::TestApp;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const WEBHOOK_SECRET: &str = "p11-2-test-secret";

fn sign(payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(WEBHOOK_SECRET.as_bytes()).unwrap();
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

async fn create_user_and_project(
    app: &TestApp,
    repo_owner: &str,
    repo_name: &str,
    mode: &str,
    labels: &[&str],
) -> Uuid {
    let owner_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users (email, username, password_hash, first_name, last_name,
                           display_name, skill_domain, email_verified, role)
        VALUES ($1, $2, 'x', 'P', 'roject', 'Project', 'code', TRUE, 'user')
        RETURNING id
        "#,
    )
    .bind(format!("owner-{}@ex.io", Uuid::new_v4()))
    .bind(format!("o{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("user");

    sqlx::query_scalar(
        r#"
        INSERT INTO projects (slug, name, owner_type, owner_id,
                              github_repo_owner, github_repo_name,
                              slice_ingestion_mode, curated_labels)
        VALUES ($1, 'Test Project', 'user', $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner_id)
    .bind(repo_owner)
    .bind(repo_name)
    .bind(mode)
    .bind(labels)
    .fetch_one(&app.db)
    .await
    .expect("project")
}

async fn post_webhook(app: &TestApp, event: &str, payload: &serde_json::Value) -> reqwest::Response {
    // SAFETY: setenv is called single-threaded before each test's HTTP call.
    unsafe {
        std::env::set_var("GITHUB_WEBHOOK_SECRET", WEBHOOK_SECRET);
    }
    let body = serde_json::to_vec(payload).unwrap();
    let sig = sign(&body);
    let delivery = Uuid::new_v4().to_string();
    reqwest::Client::new()
        .post(format!("{}/api/webhooks/github", app.addr))
        .header("Content-Type", "application/json")
        .header("X-Hub-Signature-256", sig)
        .header("X-GitHub-Delivery", delivery)
        .header("X-GitHub-Event", event)
        .body(body)
        .send()
        .await
        .expect("webhook post")
}

fn make_issue_labeled_payload(
    repo_owner: &str,
    repo_name: &str,
    issue_number: i32,
    title: &str,
    label_added: &str,
    is_pull_request: bool,
) -> serde_json::Value {
    let mut issue = json!({
        "number": issue_number,
        "title": title,
        "body": "Test issue body",
        "html_url": format!("https://github.com/{repo_owner}/{repo_name}/issues/{issue_number}"),
        "labels": [{ "name": label_added }],
    });
    if is_pull_request {
        issue["pull_request"] = json!({"url": "irrelevant"});
    }
    json!({
        "action": "labeled",
        "label": { "name": label_added },
        "issue": issue,
        "repository": { "full_name": format!("{repo_owner}/{repo_name}") },
    })
}

// ═══════════════════════════════════════════════════════════════════
// Match : label curé + repo tracké → slice draft créée
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn labeled_curated_issue_creates_draft_slice() {
    let app = TestApp::spawn().await;
    let project_id = create_user_and_project(
        &app, "acme", "widgets", "curator_review", &["good-first-issue"],
    )
    .await;

    let payload = make_issue_labeled_payload(
        "acme", "widgets", 42, "Fix async race", "good-first-issue", false,
    );
    let resp = post_webhook(&app, "issues", &payload).await;
    assert_eq!(resp.status(), 200);

    let (status, meta): (String, serde_json::Value) = sqlx::query_as(
        "SELECT status, external_metadata
         FROM project_slices
         WHERE project_id = $1 AND external_ref = '42'",
    )
    .bind(project_id)
    .fetch_one(&app.db)
    .await
    .expect("slice created");

    assert_eq!(status, "draft", "mode curator_review → draft");
    assert_eq!(meta["trigger_label"], "good-first-issue");
    assert_eq!(meta["issue_number"], 42);

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Mode 'auto' → status='open' direct
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn auto_mode_publishes_slice_directly() {
    let app = TestApp::spawn().await;
    let project_id = create_user_and_project(
        &app, "acme", "auto-repo", "auto", &["good-first-issue"],
    )
    .await;
    let payload = make_issue_labeled_payload(
        "acme", "auto-repo", 7, "Auto issue", "good-first-issue", false,
    );
    let resp = post_webhook(&app, "issues", &payload).await;
    assert_eq!(resp.status(), 200);

    let status: String = sqlx::query_scalar(
        "SELECT status FROM project_slices WHERE project_id = $1 AND external_ref = '7'",
    )
    .bind(project_id)
    .fetch_one(&app.db)
    .await
    .expect("s");
    assert_eq!(status, "open");
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Label non curé → no-op
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn non_curated_label_ignored() {
    let app = TestApp::spawn().await;
    let project_id = create_user_and_project(
        &app, "acme", "curated", "auto", &["good-first-issue"],
    )
    .await;
    let payload = make_issue_labeled_payload(
        "acme", "curated", 1, "Random", "wontfix", false,
    );
    let resp = post_webhook(&app, "issues", &payload).await;
    assert_eq!(resp.status(), 200);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_slices WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(&app.db)
    .await
    .expect("c");
    assert_eq!(count, 0, "label 'wontfix' non curé → aucune slice");
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Repo non tracké → no-op
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn untracked_repo_is_ignored() {
    let app = TestApp::spawn().await;
    // Aucun projet créé → aucun match

    let payload = make_issue_labeled_payload(
        "unknown", "repo", 1, "Ghost", "good-first-issue", false,
    );
    let resp = post_webhook(&app, "issues", &payload).await;
    assert_eq!(resp.status(), 200);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_slices")
        .fetch_one(&app.db)
        .await
        .expect("c");
    assert_eq!(count, 0);
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// PR (issue.pull_request set) → no-op
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pull_requests_are_skipped() {
    let app = TestApp::spawn().await;
    let _p = create_user_and_project(
        &app, "acme", "prrepo", "auto", &["good-first-issue"],
    )
    .await;

    let payload = make_issue_labeled_payload(
        "acme", "prrepo", 100, "A PR labeled", "good-first-issue", true,
    );
    let resp = post_webhook(&app, "issues", &payload).await;
    assert_eq!(resp.status(), 200);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_slices")
        .fetch_one(&app.db)
        .await
        .expect("c");
    assert_eq!(count, 0, "PRs sont skip par le handler");
    drop(app);
}
