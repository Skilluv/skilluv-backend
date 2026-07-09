mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_update_bio() {
    let app = common::TestApp::spawn().await;
    app.register_user("profuser").await;
    app.login("profuser").await;

    let resp = app
        .put(
            "/api/profile/me",
            &json!({ "bio": "Rust developer", "github": "profuser" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["user"]["bio"], "Rust developer");
    assert_eq!(body["data"]["user"]["github"], "profuser");
}

#[tokio::test]
async fn test_privacy_defaults() {
    let app = common::TestApp::spawn().await;
    app.register_user("privuser").await;
    app.login("privuser").await;

    let resp = app.get("/api/profile/me/privacy").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["privacy"]["show_heatmap"], true);
    assert_eq!(body["data"]["privacy"]["show_email"], false);
    assert_eq!(body["data"]["privacy"]["allow_interest_requests"], true);
}

#[tokio::test]
async fn test_privacy_hides_heatmap_in_public_profile() {
    let app = common::TestApp::spawn().await;
    app.register_user("hideuser").await;
    app.login("hideuser").await;

    // Activate profile
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'hideuser'")
        .execute(&app.db)
        .await
        .unwrap();

    // Set privacy to hide heatmap
    app.put("/api/profile/me/privacy", &json!({ "show_heatmap": false }))
        .await;

    // Check public profile
    let resp = app.get("/api/profile/hideuser").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"]["heatmap_summary"].is_null());
    assert!(body["data"]["skill_tree"].is_array()); // skill_tree should still be visible
}

#[tokio::test]
async fn test_inactive_profile_404() {
    let app = common::TestApp::spawn().await;
    app.register_user("inactiveuser").await;
    // profile_active defaults to false

    let resp = app.get("/api/profile/inactiveuser").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_display_name() {
    let app = common::TestApp::spawn().await;
    app.register_user("nameuser").await;
    app.login("nameuser").await;

    let resp = app
        .put(
            "/api/auth/me/display-name",
            &json!({ "display_name": "New Name" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["display_name"], "New Name");
}
