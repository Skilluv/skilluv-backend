mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_leaderboard_empty() {
    let app = common::TestApp::spawn().await;

    let resp = app.get("/api/leaderboards/global?period=alltime").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["entries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_leaderboard_list() {
    let app = common::TestApp::spawn().await;

    let resp = app.get("/api/leaderboards").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    let leaderboards = body["data"]["leaderboards"].as_array().unwrap();
    assert_eq!(leaderboards.len(), 5); // global, code, design, game, security
}

#[tokio::test]
async fn test_leaderboard_validation() {
    let app = common::TestApp::spawn().await;

    let resp = app.get("/api/leaderboards/invalid_domain").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp2 = app
        .get("/api/leaderboards/global?period=invalid")
        .await;
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_my_rank_requires_auth() {
    let app = common::TestApp::spawn().await;

    // No login — should fail
    let resp = app.get("/api/leaderboards/global/me").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
