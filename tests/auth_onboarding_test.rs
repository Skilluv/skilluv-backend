//! Pattern C onboarding — SSO/magic-link signups start incomplete and must go through
//! `/auth/complete-profile` before touching write endpoints.

mod common;

use reqwest::StatusCode;
use serde_json::json;

use common::{Mailpit, TestApp};

/// Helper: create a fresh user via magic-link signup. That path leaves
/// `skill_domain = NULL` and `terms_accepted_at = NULL`, exactly like OAuth.
async fn signup_via_magic_link(email: &str) -> (TestApp, reqwest::Client) {
    let mp = Mailpit::new();
    mp.wipe().await;

    let app = TestApp::spawn().await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();

    let resp = client
        .post(format!("{}/api/auth/magic-link/request", app.addr))
        .json(&json!({ "email": email, "intent": "signup" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let msg = mp.wait_for(email, 5_000).await;
    let token = Mailpit::extract_token(&msg, "token").expect("no token in magic link");

    let resp = client
        .post(format!("{}/api/auth/magic-link/consume", app.addr))
        .json(&json!({ "token": token }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    (app, client)
}

#[tokio::test]
async fn test_magic_link_signup_reports_profile_incomplete() {
    let (app, client) = signup_via_magic_link("incomplete@test.com").await;

    let me: serde_json::Value = client
        .get(format!("{}/api/auth/me", app.addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(me["data"]["user"]["profile_completed"], false);
    assert!(me["data"]["user"]["skill_domain"].is_null());
}

#[tokio::test]
async fn test_guarded_endpoint_rejects_incomplete_profile() {
    let (app, client) = signup_via_magic_link("guarded@test.com").await;

    // AuthUserComplete runs BEFORE handler body/path resolution, so a fake UUID is fine —
    // the extractor rejects first with 403 AUTH_PROFILE_INCOMPLETE.
    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{}/api/challenges/{fake_id}/submit", app.addr))
        .json(&json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "AUTH_PROFILE_INCOMPLETE");
}

#[tokio::test]
async fn test_complete_profile_unlocks_write_endpoints() {
    let (app, client) = signup_via_magic_link("unlocker@test.com").await;

    // Refuse: skill_domain invalid.
    let resp = client
        .post(format!("{}/api/auth/complete-profile", app.addr))
        .json(&json!({
            "skill_domain": "not-a-real-domain",
            "terms_accepted": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Refuse: terms not accepted.
    let resp = client
        .post(format!("{}/api/auth/complete-profile", app.addr))
        .json(&json!({
            "skill_domain": "code",
            "terms_accepted": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // OK.
    let resp = client
        .post(format!("{}/api/auth/complete-profile", app.addr))
        .json(&json!({
            "skill_domain": "code",
            "terms_accepted": true,
            "country": "FR",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // /me now reports profile_completed = true.
    let me: serde_json::Value = client
        .get(format!("{}/api/auth/me", app.addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["data"]["user"]["profile_completed"], true);
    assert_eq!(me["data"]["user"]["skill_domain"], "code");
}

#[tokio::test]
async fn test_complete_profile_refuses_second_call() {
    let (app, client) = signup_via_magic_link("twicer@test.com").await;

    let call_complete = |sd: &str| {
        let client = client.clone();
        let addr = app.addr.clone();
        let sd = sd.to_string();
        async move {
            client
                .post(format!("{addr}/api/auth/complete-profile"))
                .json(&json!({ "skill_domain": sd, "terms_accepted": true }))
                .send()
                .await
                .unwrap()
        }
    };

    assert_eq!(call_complete("code").await.status(), StatusCode::OK);
    // Second call is refused so the user can't retroactively change skill_domain here.
    assert_eq!(
        call_complete("design").await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn test_classic_register_bypasses_onboarding_gate() {
    // Users who go through /auth/register (with terms + skill_domain in the body) are
    // profile_complete from the start.
    let app = TestApp::spawn().await;
    let resp = app.register_user("classic").await;
    assert_eq!(resp["data"]["user"]["profile_completed"], true);
    assert_eq!(resp["data"]["user"]["skill_domain"], "code");
}
