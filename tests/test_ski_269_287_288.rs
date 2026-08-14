//! Integration tests for three contract fixes found on staging:
//!
//! * SKI-288 — attestation verification reachable under `/api`.
//! * SKI-269 — detaching a GitHub repo from a project actually works.
//! * SKI-287 — email preferences on the path the front end calls, with a
//!   strict PUT and a one-click unsubscribe link.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

// ═══════════════════════════════════════════════════════════════════
// SKI-288 — /api/verify/{hash}
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn verify_is_reachable_under_api_and_mirrors_the_root_route() {
    let app = TestApp::spawn().await;
    let unknown = "a".repeat(64);

    // The root route keeps working for external consumers...
    let root = app
        .client
        .get(format!("{}/verify/{unknown}", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(root.status(), StatusCode::OK);
    let root_body: Value = root.json().await.unwrap();

    // ...and the same answer is now available under /api, which is the only
    // path the browser app can reach (the front end owns /verify on its own
    // origin).
    let api = app.get(&format!("/api/verify/{unknown}")).await;
    assert_eq!(api.status(), StatusCode::OK);
    let api_body: Value = api.json().await.unwrap();

    assert_eq!(
        root_body, api_body,
        "both mounts share one handler, so the shapes cannot drift"
    );
    assert_eq!(api_body["valid"], false);
    assert_eq!(api_body["reason"], "unknown attestation hash");
}

#[tokio::test]
async fn verify_under_api_rejects_a_malformed_hash_without_500() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/verify/not-a-hash").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["valid"], false);
    assert_eq!(body["reason"], "malformed attestation hash");
}

// ═══════════════════════════════════════════════════════════════════
// SKI-269 — detaching a GitHub repo
// ═══════════════════════════════════════════════════════════════════

/// Create a project through the admin API and return its slug.
async fn create_project_with_repo(app: &TestApp, slug: &str, owner_id: Uuid) -> String {
    let resp = app
        .post(
            "/api/admin/projects",
            &json!({
                "slug": slug,
                "name": "Detach test project",
                "owner_type": "user",
                "owner_id": owner_id,
                "github_repo_owner": "launchbadge",
                "github_repo_name": "sqlx",
                "curated_labels": ["good first issue"],
                "skill_domains": ["code"],
            }),
        )
        .await;
    // The handler answers 200, not the 201 its utoipa annotation claims —
    // one of the spec drifts SKI-111 exists to surface. Accept the real
    // behaviour here rather than couple this fix to that one.
    assert!(
        resp.status().is_success(),
        "project creation failed: {:?}",
        resp.text().await
    );
    slug.to_string()
}

async fn repo_of(app: &TestApp, slug: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT github_repo_owner, github_repo_name FROM projects WHERE slug = $1")
        .bind(slug)
        .fetch_one(&app.db)
        .await
        .expect("read project")
}

#[tokio::test]
async fn explicit_null_detaches_the_repo() {
    let app = TestApp::spawn().await;
    let admin = app.register_admin("detachadmin").await;
    let admin_id = user_id_of(&admin);
    app.login("detachadmin").await;
    let slug = create_project_with_repo(&app, "detach-me", admin_id).await;

    let (owner, name) = repo_of(&app, &slug).await;
    assert_eq!(owner.as_deref(), Some("launchbadge"));
    assert_eq!(name.as_deref(), Some("sqlx"));

    let resp = app
        .client
        .patch(format!("{}/api/admin/projects/{slug}", app.addr))
        .json(&json!({ "github_repo_owner": null, "github_repo_name": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let (owner, name) = repo_of(&app, &slug).await;
    assert!(
        owner.is_none() && name.is_none(),
        "the repo must actually be detached — the old COALESCE answered 200 \
         while leaving it wired, so ingestion kept running"
    );
}

#[tokio::test]
async fn omitting_the_github_fields_leaves_the_repo_alone() {
    let app = TestApp::spawn().await;
    let admin = app.register_admin("keepadmin").await;
    let admin_id = user_id_of(&admin);
    app.login("keepadmin").await;
    let slug = create_project_with_repo(&app, "keep-me", admin_id).await;

    // A patch about something else entirely must not disturb the repo.
    let resp = app
        .client
        .patch(format!("{}/api/admin/projects/{slug}", app.addr))
        .json(&json!({ "name": "Renamed project" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let (owner, name) = repo_of(&app, &slug).await;
    assert_eq!(owner.as_deref(), Some("launchbadge"));
    assert_eq!(name.as_deref(), Some("sqlx"));
}

#[tokio::test]
async fn half_specified_github_changes_are_refused() {
    let app = TestApp::spawn().await;
    let admin = app.register_admin("halfadmin").await;
    let admin_id = user_id_of(&admin);
    app.login("halfadmin").await;
    let slug = create_project_with_repo(&app, "half-me", admin_id).await;

    for payload in [
        json!({ "github_repo_owner": null }),
        json!({ "github_repo_name": null }),
        json!({ "github_repo_owner": "launchbadge" }),
        json!({ "github_repo_owner": null, "github_repo_name": "sqlx" }),
    ] {
        let resp = app
            .client
            .patch(format!("{}/api/admin/projects/{slug}", app.addr))
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "payload {payload} must be refused, never answered 200 without effect"
        );
    }

    // Nothing moved.
    let (owner, name) = repo_of(&app, &slug).await;
    assert_eq!(owner.as_deref(), Some("launchbadge"));
    assert_eq!(name.as_deref(), Some("sqlx"));
}

#[tokio::test]
async fn rewiring_to_another_repo_still_works() {
    let app = TestApp::spawn().await;
    let admin = app.register_admin("rewireadmin").await;
    let admin_id = user_id_of(&admin);
    app.login("rewireadmin").await;
    let slug = create_project_with_repo(&app, "rewire-me", admin_id).await;

    let resp = app
        .client
        .patch(format!("{}/api/admin/projects/{slug}", app.addr))
        .json(&json!({ "github_repo_owner": "tokio-rs", "github_repo_name": "tokio" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let (owner, name) = repo_of(&app, &slug).await;
    assert_eq!(owner.as_deref(), Some("tokio-rs"));
    assert_eq!(name.as_deref(), Some("tokio"));
}

// ═══════════════════════════════════════════════════════════════════
// SKI-287 — email preferences
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn preferences_return_defaults_for_an_untouched_account() {
    let app = TestApp::spawn().await;
    app.register_user("prefsfresh").await;
    app.login("prefsfresh").await;

    let resp = app.get("/api/users/me/email-preferences").await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a user who never opened the screen gets defaults, not a 404"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["digest_weekly"], true);
    assert_eq!(body["data"]["streak_reminder"], true);
    assert_eq!(
        body["data"]["marketing"], false,
        "marketing is opt-in — GDPR requires explicit consent"
    );

    // Reading must not have created a row: consent is recorded when given,
    // not when the screen is opened. The three words are a view over the
    // notification catalogue now, so the row that must not exist is there.
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notification_preferences
          WHERE kind IN (SELECT kind FROM notification_kinds
                          WHERE category IN ('digest', 'lifecycle'))",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn put_replaces_all_three_flags_and_rejects_partial_payloads() {
    let app = TestApp::spawn().await;
    app.register_user("prefsput").await;
    app.login("prefsput").await;

    let resp = app
        .put(
            "/api/users/me/email-preferences",
            &json!({ "digest_weekly": true, "streak_reminder": false, "marketing": true }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["streak_reminder"], false);
    assert_eq!(body["data"]["marketing"], true);

    // Partial payloads are refused: with an opt-in flag, "absent" is
    // ambiguous and guessing wrong on consent is not acceptable.
    for bad in [
        json!({ "digest_weekly": true }),
        json!({ "digest_weekly": true, "streak_reminder": false }),
        json!({}),
        json!({ "digest_weekly": "yes", "streak_reminder": false, "marketing": false }),
    ] {
        let resp = app.put("/api/users/me/email-preferences", &bad).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "payload {bad} must be refused"
        );
    }

    // The refused payloads changed nothing.
    let body: Value = app
        .get("/api/users/me/email-preferences")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["streak_reminder"], false);
    assert_eq!(body["data"]["marketing"], true);
}

#[tokio::test]
async fn one_click_unsubscribe_works_without_a_session_and_is_idempotent() {
    let app = TestApp::spawn().await;
    let me = app.register_user("prefsunsub").await;
    let my_id = user_id_of(&me);

    // Build the same token the email footer carries.
    let secret = skilluv_backend::routes::email_prefs::unsub_secret("test-secret-key-for-testing");
    let token =
        skilluv_backend::services::digest::build_unsubscribe_token(my_id, "digest_weekly", &secret);

    // No cookie jar: an unsubscribe link must work from a mail client.
    let anon = reqwest::Client::new();
    let resp = anon
        .get(format!("{}/api/email/unsubscribe/{token}", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .contains("text/html"),
        "the link is opened in a browser, so it answers HTML"
    );

    let digest_weekly: bool = sqlx::query_scalar(
        "SELECT enabled FROM notification_preferences
          WHERE user_id = $1 AND kind = 'digest.weekly' AND channel = 'email'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!digest_weekly);

    // Mail clients prefetch links: a second hit must not fail.
    let resp = anon
        .get(format!("{}/api/email/unsubscribe/{token}", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Only the encoded category was touched. Nothing was written for the
    // other two, so they still read as their defaults.
    let touched: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT kind FROM notification_preferences WHERE user_id = $1")
            .bind(my_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(
        touched,
        vec!["digest.weekly".to_string()],
        "unsubscribing from one category leaves the others alone"
    );
}

#[tokio::test]
async fn a_forged_unsubscribe_token_is_rejected() {
    let app = TestApp::spawn().await;
    app.register_user("prefsforged").await;

    let anon = reqwest::Client::new();
    for token in ["garbage", "aaaa.bbbb", ""] {
        let resp = anon
            .get(format!("{}/api/email/unsubscribe/{token}", app.addr))
            .send()
            .await
            .unwrap();
        assert!(
            resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::NOT_FOUND,
            "token {token:?} must not unsubscribe anyone, got {}",
            resp.status()
        );
    }
}
