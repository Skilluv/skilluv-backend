mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_list_challenges_empty() {
    let app = common::TestApp::spawn().await;
    app.register_user("chaluser").await;
    app.login("chaluser").await;

    let resp = app.get("/api/challenges").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["pagination"]["total"], 0);
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
    assert_eq!(body["data"]["submission"]["status"], "success");
    assert!(body["data"]["fragments_earned"].as_i64().unwrap() > 0);
    assert_eq!(body["data"]["user"]["profile_active"], true);
}

#[tokio::test]
async fn test_badge_earned_after_first_challenge() {
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
    let user_id: String = sqlx::query_scalar("SELECT id::TEXT FROM users WHERE username = 'badgeuser'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let badge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1::UUID")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert!(badge_count >= 1, "Should have earned at least 1 badge");
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
    let resp = app
        .get(&format!("/api/challenges/{cid}/submissions"))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["submissions"].as_array().unwrap().len(), 1);
}
