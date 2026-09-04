mod common;

use reqwest::StatusCode;
use serde_json::json;

/// The listing answers with a well-formed page, and its total is what is
/// actually published.
///
/// This asserted `total == 0` against the whole catalogue, which was true
/// while nothing anywhere was published. Migration 0615 publishes the six code
/// exercises a newcomer climbs, so the unfiltered catalogue is no longer
/// empty — and an assertion that only held because the platform had nothing to
/// offer was never testing the endpoint.
///
/// Scoped to a domain with nothing published instead: `soft_skills` has its
/// entry rite and nothing else, and a rite is excluded from the listing by
/// `is_onboarding = FALSE`. So this still exercises the empty page, and it
/// exercises the populated one too.
#[tokio::test]
async fn test_list_challenges_page_shape() {
    let app = common::TestApp::spawn().await;
    app.register_user("chaluser").await;
    app.login("chaluser").await;

    let resp = app.get("/api/challenges?domain=soft_skills").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["pagination"]["total"], 0);
    assert!(body["data"].as_array().unwrap().is_empty());

    let resp = app.get("/api/challenges?domain=code").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["pagination"]["total"].as_i64().unwrap() >= 6,
        "the code ladder should be listed: {body}"
    );
}

#[tokio::test]
async fn test_start_and_submit_challenge() {
    let app = common::TestApp::spawn().await;

    // Create admin and publish a challenge
    app.register_admin("chaladmin").await;
    app.login("chaladmin").await;

    let create_resp = app
        .post(
            "/api/admin/challenges",
            &json!({
                "title": "Hello Test",
                "description": "Print hello",
                "instructions": "Write code that prints Hello, Skilluv!",
                "skill_domain": "code",
                "difficulty": 1,
                "expected_output": "Hello, Skilluv!",
                "reward_fragments": 30,
                "is_onboarding": true,
            }),
        )
        .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let challenge_id = create_resp.json::<serde_json::Value>().await.unwrap()["data"]["challenge"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Publish
    app.post(
        &format!("/api/admin/challenges/{challenge_id}/publish"),
        &json!({}),
    )
    .await;

    // Register regular user and start challenge
    app.register_user("solver").await;
    app.login("solver").await;

    let start_resp = app
        .post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    assert_eq!(start_resp.status(), StatusCode::CREATED);

    // Submit correct solution
    let submit_resp = app
        .post(
            &format!("/api/challenges/{challenge_id}/submit"),
            &json!({ "code": "print('Hello, Skilluv!')" }),
        )
        .await;
    assert_eq!(submit_resp.status(), StatusCode::OK);

    let body: serde_json::Value = submit_resp.json().await.unwrap();

    // This assertion used to read `"success"`, and it is worth saying why it
    // passed. CI has never had a Judge0 to run anything, so the grader fell
    // through to asking whether the *source* contained `expected_output` as a
    // substring — and `print('Hello, Skilluv!')` contains "Hello, Skilluv!".
    // The submission was never executed. Pasting the expected output into a
    // comment would have passed just as well, which is what made the fallback
    // a fraud surface rather than a degradation.
    //
    // Nothing grades code now. A submission goes to the queue a person reads,
    // like every other domain, and fragments follow the reviewer's verdict.
    assert_eq!(body["data"]["submission"]["status"], "pending_review");
    assert_eq!(
        body["data"]["fragments_earned"], 0,
        "fragments are the reviewer's to award, not a string comparison's"
    );
    assert_eq!(
        body["data"]["user"]["profile_active"], false,
        "handing work in is not the same as having it read: the profile goes          live when a reviewer settles the deliverable (services/reviews.rs),          which is what test_code_newcomer_path drives end to end"
    );
}

/// Submitting earns nothing on its own.
///
/// This asserted `badge_count >= 1` straight after a submission, which held
/// only because the submission was auto-graded `success` — and it was graded
/// success because the source contained the expected output as a substring,
/// never because anything ran it. With the grader gone, a badge is something
/// a reviewer's verdict produces (`proof_hooks` on a settled deliverable), and
/// this test now pins the half it can see: the work is queued, and no badge
/// has been minted for work nobody has read.
#[tokio::test]
async fn test_no_badge_before_anybody_has_read_the_work() {
    let app = common::TestApp::spawn().await;

    // Setup: admin creates and publishes challenge
    app.register_admin("badgeadmin").await;
    app.login("badgeadmin").await;

    let cr = app
        .post(
            "/api/admin/challenges",
            &json!({
                "title": "Badge Test",
                "description": "Test",
                "instructions": "Do it",
                "skill_domain": "code",
                "difficulty": 1,
                "expected_output": "Hello, Skilluv!",
                "reward_fragments": 20,
                "is_onboarding": true,
            }),
        )
        .await;
    let cid = cr.json::<serde_json::Value>().await.unwrap()["data"]["challenge"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(&format!("/api/admin/challenges/{cid}/publish"), &json!({}))
        .await;

    // User completes challenge
    app.register_user("badgeuser").await;
    app.login("badgeuser").await;
    app.post(&format!("/api/challenges/{cid}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{cid}/submit"),
        &json!({ "code": "print('Hello, Skilluv!')" }),
    )
    .await;

    // Check badges via profile
    let user_id: String =
        sqlx::query_scalar("SELECT id::TEXT FROM users WHERE username = 'badgeuser'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    let badge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1::UUID")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert_eq!(badge_count, 0, "no badge for work nobody has read yet");

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions
          WHERE user_id = $1::UUID AND status = 'pending_review'",
    )
    .bind(&user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(queued, 1, "and the submission is waiting for a reviewer");
}

#[tokio::test]
async fn test_submissions_history() {
    let app = common::TestApp::spawn().await;

    // Setup challenge
    app.register_admin("histadmin").await;
    app.login("histadmin").await;
    let cr = app
        .post(
            "/api/admin/challenges",
            &json!({
                "title": "History Test",
                "description": "Test",
                "instructions": "Do it",
                "skill_domain": "code",
                "difficulty": 1,
                "expected_output": "test",
                "reward_fragments": 10,
                "is_onboarding": true,
            }),
        )
        .await;
    let cid = cr.json::<serde_json::Value>().await.unwrap()["data"]["challenge"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(&format!("/api/admin/challenges/{cid}/publish"), &json!({}))
        .await;

    // User starts and submits
    app.register_user("histuser").await;
    app.login("histuser").await;
    app.post(&format!("/api/challenges/{cid}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{cid}/submit"),
        &json!({ "code": "wrong" }),
    )
    .await;

    // Check submissions
    let resp = app.get(&format!("/api/challenges/{cid}/submissions")).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["submissions"].as_array().unwrap().len(), 1);
}
