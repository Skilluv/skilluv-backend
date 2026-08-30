//! What an API key may do, and what revoking one actually does (SKI-172).
//!
//! ## Why this suite exists
//!
//! SKI-172 asked for a VS Code extension. The extension was never the hard
//! part: `POST /api/security/reports` took a session cookie, and a cookie is a
//! browser thing — so nothing without a browser could file a finding,
//! whatever it was written in. An editor extension, a CLI, a CI job and a
//! GitHub Action all hit the same wall.
//!
//! Reading that route turned up a second thing, in the table rather than the
//! route. Migration 0359 added `revoked_at` and `revoked_reason` to `api_keys`
//! **and an index keyed on `revoked_at IS NULL`** — which says, in the schema,
//! that a NULL there is what makes a key live. The authenticator only ever
//! read `active`, and revocation only ever wrote it.
//!
//! Nothing was broken, because nothing wrote `revoked_at`. But two revocation
//! mechanisms existed, one of them was the obvious one to reach for, and
//! reaching for it would have revoked a key that kept working. These tests
//! hold both halves shut.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

/// Mint a key straight into the table, with the scopes the test wants.
///
/// The raw key is returned because it is never recoverable afterwards — only
/// its hash is stored, which is the property worth having.
async fn a_key(app: &TestApp, user: Uuid, scopes: &[&str]) -> String {
    let raw = format!("sk_live_{}", Uuid::new_v4().simple());
    let prefix = &raw[..12];
    let hash = skilluv_backend::services::AuthService::hash_password(&raw).unwrap();
    sqlx::query(
        "INSERT INTO api_keys (user_id, name, key_prefix, key_hash, permissions)
         VALUES ($1, 'test key', $2, $3, $4)",
    )
    .bind(user)
    .bind(prefix)
    .bind(&hash)
    .bind(json!(scopes))
    .execute(&app.db)
    .await
    .expect("a key");
    raw
}

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A report the platform will actually accept.
///
/// `target_host` has to be one of the published scope hosts — `submit` refuses
/// anything else, and says why: the safe harbour covers what is in scope and
/// nothing outside it. Using a made-up host here would have the suite fail on
/// the scope rule rather than on what it is testing.
fn a_report() -> Value {
    json!({
        "title": "Authenticated SQL injection on the applications export",
        "description_md": "The sort parameter of the export is concatenated into the query.",
        "reproduction_steps_md": "1. Sign in as a recruiter\n2. Call the export with sort=1;--",
        "impact_md": "A recruiter account reads the whole applications table.",
        "target_kind": "platform",
        "target_host": "staging.skill-uv.com",
        "severity_tier": "high",
        "cwe_id": "CWE-89",
    })
}

/// The gap SKI-172 actually names: a program with no browser can file a
/// finding, from where it found it.
#[tokio::test]
async fn a_key_with_the_scope_can_file_a_finding() {
    let app = TestApp::spawn().await;
    app.register_user("key_researcher").await;
    let uid = user_id(&app, "key_researcher").await;
    let key = a_key(&app, uid, &["security:report"]).await;

    let response = reqwest::Client::new()
        .post(format!("{}/api/security/reports", app.addr))
        .bearer_auth(&key)
        .json(&a_report())
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "a scoped key cannot file a finding: {}",
        response.text().await.unwrap_or_default()
    );

    // And it is the key's owner who filed it, not nobody.
    let filed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM security_findings WHERE reporter_user_id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(filed, 1);
}

/// A key minted to read a profile must not be able to file a report.
///
/// 403 and not 401 on purpose: the key is valid and is not allowed to do this.
/// Answering 401 would send somebody hunting for a bad token.
#[tokio::test]
async fn a_key_without_the_scope_is_refused_as_forbidden() {
    let app = TestApp::spawn().await;
    app.register_user("key_narrow").await;
    let uid = user_id(&app, "key_narrow").await;
    let key = a_key(&app, uid, &["profile:read"]).await;

    let status = reqwest::Client::new()
        .post(format!("{}/api/security/reports", app.addr))
        .bearer_auth(&key)
        .json(&a_report())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16();

    assert_eq!(status, 403, "a key without the scope reached the route");
}

/// The defect this suite was written around.
///
/// `revoked_at` is what migration 0359's index treats as the liveness test.
/// Before this, the authenticator read only `active`, so a key revoked this
/// way — the obvious way, given the column exists — kept authenticating.
#[tokio::test]
async fn a_key_revoked_by_its_timestamp_stops_working() {
    let app = TestApp::spawn().await;
    app.register_user("key_revoked").await;
    let uid = user_id(&app, "key_revoked").await;
    let key = a_key(&app, uid, &["security:report"]).await;

    // It works first, so the assertion below cannot pass for the wrong reason.
    let before = reqwest::Client::new()
        .post(format!("{}/api/security/reports", app.addr))
        .bearer_auth(&key)
        .json(&a_report())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16();
    assert_eq!(before, 200);

    // Revoked the way the schema says a key is revoked, and *only* that way:
    // `active` is deliberately left TRUE, because that is the case the old
    // authenticator got wrong.
    sqlx::query(
        "UPDATE api_keys SET revoked_at = NOW(), revoked_reason = 'test'
          WHERE user_id = $1",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    let after = reqwest::Client::new()
        .post(format!("{}/api/security/reports", app.addr))
        .bearer_auth(&key)
        .json(&a_report())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16();
    assert_eq!(
        after, 401,
        "a key with revoked_at set still authenticated — the column and the \
         index of migration 0359 say it is dead"
    );
}

/// And the other direction: revoking through the API writes both columns.
///
/// Writing only `active` left the two disagreeing, so anything reading
/// `revoked_at` — or the index built on it — counted a revoked key as live.
#[tokio::test]
async fn revoking_through_the_api_leaves_both_columns_agreeing() {
    let app = TestApp::spawn().await;
    app.register_user("key_owner").await;
    let uid = user_id(&app, "key_owner").await;
    a_key(&app, uid, &["security:report"]).await;
    app.login("key_owner").await;

    let id: Uuid = sqlx::query_scalar("SELECT id FROM api_keys WHERE user_id = $1")
        .bind(uid)
        .fetch_one(&app.db)
        .await
        .unwrap();

    let status = app
        .delete(&format!("/api/developer/keys/{id}"))
        .await
        .status();
    assert!(status.is_success(), "revoking answered {status}");

    let (active, revoked, reason): (bool, bool, Option<String>) = sqlx::query_as(
        "SELECT active, revoked_at IS NOT NULL, revoked_reason FROM api_keys WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(!active, "active was not cleared");
    assert!(
        revoked,
        "revoked_at was not written, so the index still calls it live"
    );
    assert!(
        reason.is_some(),
        "a revocation with no reason is a revocation nobody can explain"
    );
}

/// A session still works, and needs no scope.
///
/// The point of one handler rather than two: a person in a browser must never
/// be refused because a key would have needed a permission.
#[tokio::test]
async fn a_browser_session_still_files_a_finding_without_any_scope() {
    let app = TestApp::spawn().await;
    app.register_user("key_browser").await;
    app.login("key_browser").await;

    let status = app
        .post("/api/security/reports", &a_report())
        .await
        .status();
    assert_eq!(status.as_u16(), 200, "the session path regressed");
}

/// An invented key is refused, and a valid prefix with a wrong body too.
#[tokio::test]
async fn a_key_that_was_never_minted_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("key_forger").await;
    let uid = user_id(&app, "key_forger").await;
    let real = a_key(&app, uid, &["security:report"]).await;

    // Same prefix, different secret: the prefix is only an index, and the hash
    // is what decides.
    let forged = format!("{}{}", &real[..12], Uuid::new_v4().simple());

    for candidate in [forged.as_str(), "sk_live_000000000000", "not-a-key"] {
        let status = reqwest::Client::new()
            .post(format!("{}/api/security/reports", app.addr))
            .bearer_auth(candidate)
            .json(&a_report())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
        assert_eq!(status, 401, "{candidate} was accepted");
    }
}
