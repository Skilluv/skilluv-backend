mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_create_api_key() {
    let app = common::TestApp::spawn().await;
    app.register_user("devuser").await;
    app.login("devuser").await;

    let resp = app
        .post(
            "/api/developer/keys",
            &json!({
                "name": "My App",
                "permissions": ["read:profile", "read:badges"],
            }),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = resp.json().await.unwrap();
    let secret = body["data"]["secret"].as_str().unwrap();
    assert!(secret.starts_with("sk_live_"));
}

#[tokio::test]
async fn test_public_api_with_key() {
    let app = common::TestApp::spawn().await;
    app.register_user("apiuser").await;
    app.login("apiuser").await;

    // Activate profile
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'apiuser'")
        .execute(&app.db)
        .await
        .unwrap();

    // Create API key
    let key_resp = app
        .post(
            "/api/developer/keys",
            &json!({
                "name": "Test Key",
                "permissions": ["read:profile", "read:badges", "read:skills"],
            }),
        )
        .await;

    let key_body: serde_json::Value = key_resp.json().await.unwrap();
    let secret = key_body["data"]["secret"].as_str().unwrap().to_string();

    // Use API key to access public API (new client without cookies)
    let api_client = reqwest::Client::new();

    let resp = api_client
        .get(format!("{}/api/v1/users/apiuser", app.addr))
        .header("Authorization", format!("Bearer {secret}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["user"]["username"], "apiuser");
}

#[tokio::test]
async fn test_public_api_without_key() {
    let app = common::TestApp::spawn().await;

    let api_client = reqwest::Client::new();
    let resp = api_client
        .get(format!("{}/api/v1/users/anyone", app.addr))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
