mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_submit_community_challenge() {
    let app = common::TestApp::spawn().await;
    app.register_user("commuser").await;
    app.login("commuser").await;

    let resp = app
        .post(
            "/api/community/challenges",
            &json!({
                "title": "FizzBuzz",
                "description": "Classic FizzBuzz problem",
                "instructions": "Write a FizzBuzz function",
                "skill_domain": "code",
                "difficulty": 2,
                "tags": ["algorithmes", "debutant"],
                "submit_for_review": true,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["challenge"]["community_status"], "review");
    assert_eq!(body["data"]["challenge"]["is_community"], true);
}

#[tokio::test]
async fn test_admin_approve_community_challenge() {
    let app = common::TestApp::spawn().await;

    // User submits challenge
    app.register_user("submitter").await;
    app.login("submitter").await;
    let cr = app
        .post(
            "/api/community/challenges",
            &json!({
                "title": "Approve Me",
                "description": "Test",
                "instructions": "Do it",
                "skill_domain": "code",
                "difficulty": 1,
                "submit_for_review": true,
            }),
        )
        .await;
    let cid = cr.json::<serde_json::Value>().await.unwrap()["data"]["challenge"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Admin approves
    app.register_admin("commadmin").await;
    app.login("commadmin").await;

    let approve_resp = app
        .post(&format!("/api/admin/community/{cid}/approve"), &json!({}))
        .await;
    assert_eq!(approve_resp.status(), StatusCode::OK);

    let body: serde_json::Value = approve_resp.json().await.unwrap();
    assert_eq!(body["data"]["challenge"]["community_status"], "approved");
    assert_eq!(body["data"]["challenge"]["status"], "published");
}

#[tokio::test]
async fn test_vote_challenge() {
    let app = common::TestApp::spawn().await;

    // Create and publish a challenge
    app.register_admin("voteadmin").await;
    app.login("voteadmin").await;
    let cr = app
        .post(
            "/api/admin/challenges",
            &json!({
                "title": "Vote Me",
                "description": "Test",
                "instructions": "Do it",
                "skill_domain": "code",
                "difficulty": 1,
                "is_training": true,
            }),
        )
        .await;
    let cid = cr.json::<serde_json::Value>().await.unwrap()["data"]["challenge"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(&format!("/api/admin/challenges/{cid}/publish"), &json!({}))
        .await;

    // Vote
    app.register_user("voter").await;
    app.login("voter").await;

    let vote_resp = app
        .post(
            &format!("/api/community/challenges/{cid}/vote"),
            &json!({}),
        )
        .await;
    assert_eq!(vote_resp.status(), StatusCode::CREATED);

    // Check vote count in DB
    let count: i32 =
        sqlx::query_scalar("SELECT vote_count FROM challenge_templates WHERE id = $1::UUID")
            .bind(&cid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 1);

    // Unvote
    let unvote_resp = app
        .delete(&format!("/api/community/challenges/{cid}/vote"))
        .await;
    assert_eq!(unvote_resp.status(), StatusCode::OK);

    let count2: i32 =
        sqlx::query_scalar("SELECT vote_count FROM challenge_templates WHERE id = $1::UUID")
            .bind(&cid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count2, 0);
}
