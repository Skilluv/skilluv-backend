mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_report_user() {
    let app = common::TestApp::spawn().await;
    app.register_user("reporter").await;
    let target = app.register_user("spammer").await;
    let target_id = target["data"]["user"]["id"].as_str().unwrap();

    app.login("reporter").await;

    let resp = app
        .post(
            "/api/reports",
            &json!({
                "target_type": "user",
                "target_id": target_id,
                "reason": "spam",
                "details": "Posting spam content",
            }),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["report"]["status"], "pending");
}

#[tokio::test]
async fn test_ban_unban_user() {
    let app = common::TestApp::spawn().await;

    // Register victim first (with a separate client to not pollute cookies)
    let tmp_client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let victim_resp = tmp_client
        .post(format!("{}/api/auth/register", app.addr))
        .json(&json!({
            "email": "banme@test.com",
            "username": "banme",
            "password": "TestPass123",
            "first_name": "Ban",
            "last_name": "Me",
            "skill_domain": "code",
        }))
        .send()
        .await
        .unwrap();
    let victim_body: serde_json::Value = victim_resp.json().await.unwrap();
    let victim_id = victim_body["data"]["user"]["id"].as_str().unwrap();

    // Now register + login admin (this sets the admin cookie on app.client)
    app.register_admin("modadmin").await;
    app.login("modadmin").await;

    // Ban
    let resp = app
        .post(
            &format!("/api/admin/users/{victim_id}/ban"),
            &json!({ "reason": "Spam bot" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify banned user can't login
    let login_resp = tmp_client
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({ "identifier": "banme", "password": "TestPass123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp.status(), StatusCode::FORBIDDEN);

    // Unban (app.client still has admin cookies)
    let unban_resp = app
        .post(&format!("/api/admin/users/{victim_id}/unban"), &json!({}))
        .await;
    assert_eq!(unban_resp.status(), StatusCode::OK);

    // Verify can login again
    let login_resp2 = tmp_client
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({ "identifier": "banme", "password": "TestPass123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp2.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_audit_log_entry() {
    let app = common::TestApp::spawn().await;
    app.register_admin("auditadmin").await;
    let victim = app.register_user("audittarget").await;
    let victim_id = victim["data"]["user"]["id"].as_str().unwrap();

    app.login("auditadmin").await;

    // Ban to create audit entry
    app.post(
        &format!("/api/admin/users/{victim_id}/ban"),
        &json!({ "reason": "Test" }),
    )
    .await;

    // Check audit log
    let resp = app.get("/api/admin/audit-log").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"].as_array().unwrap().len() >= 1);
    assert_eq!(body["data"][0]["action"], "user.ban");
}
