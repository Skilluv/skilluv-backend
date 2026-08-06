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

/// SKI-70 — a freshly registered user has `profile_active = FALSE` until their
/// first successful challenge. That flag gates the listing surfaces, not the
/// public profile page, which must be reachable straight after signup.
#[tokio::test]
async fn test_public_profile_visible_before_first_challenge() {
    let app = common::TestApp::spawn().await;
    app.register_user("freshuser").await;

    let active: bool =
        sqlx::query_scalar("SELECT profile_active FROM users WHERE username = 'freshuser'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!active, "profile_active should still default to FALSE");

    let resp = app.get("/api/profile/freshuser").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["user"]["username"], "freshuser");
}

#[tokio::test]
async fn test_hidden_profile_404() {
    let app = common::TestApp::spawn().await;
    app.register_user("hiddenuser").await;
    app.login("hiddenuser").await;

    app.put("/api/profile/me/privacy", &json!({ "hide_profile": true }))
        .await;

    let resp = app.get("/api/profile/hiddenuser").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_hide_profile_round_trips_through_privacy_settings() {
    let app = common::TestApp::spawn().await;
    app.register_user("toggleuser").await;
    app.login("toggleuser").await;

    let body: serde_json::Value = app
        .get("/api/profile/me/privacy")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["privacy"]["hide_profile"], false);

    let body: serde_json::Value = app
        .put("/api/profile/me/privacy", &json!({ "hide_profile": true }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["privacy"]["hide_profile"], true);

    // An unrelated update must not silently un-hide the profile.
    let body: serde_json::Value = app
        .put("/api/profile/me/privacy", &json!({ "show_email": true }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["privacy"]["hide_profile"], true);
    assert_eq!(body["data"]["privacy"]["show_email"], true);

    assert_eq!(
        app.get("/api/profile/toggleuser").await.status(),
        StatusCode::NOT_FOUND
    );
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
