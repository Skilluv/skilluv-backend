mod common;

use reqwest::StatusCode;
use serde_json::json;

/// Contact interest cost = 1 credit ; enterprise_credits default balance = 0.
/// Seed 100 credits pour un enterprise donné (par company_name).
async fn seed_enterprise_credits(app: &common::TestApp, company_name: &str) {
    sqlx::query(
        r#"
        INSERT INTO enterprise_credits (enterprise_id, balance, total_purchased)
        SELECT id, 100, 100 FROM enterprises WHERE company_name = $1
        ON CONFLICT (enterprise_id) DO UPDATE SET
            balance = enterprise_credits.balance + 100
        "#,
    )
    .bind(company_name)
    .execute(&app.db)
    .await
    .expect("seed credits");
}

#[tokio::test]
async fn test_send_interest_and_receive() {
    let app = common::TestApp::spawn().await;

    // Create talent
    let talent = app.register_user("contacttalent").await;
    let talent_id = talent["data"]["user"]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'contacttalent'")
        .execute(&app.db)
        .await
        .unwrap();

    // Create enterprise and send interest
    app.register_enterprise("ContactCorp").await;
    seed_enterprise_credits(&app, "ContactCorp").await;
    app.login("contactcorp").await;

    let resp = app
        .post(
            "/api/contact/interest",
            &json!({
                "talent_id": talent_id,
                "message": "We are interested in your profile!"
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Login as talent and check received interests
    app.login("contacttalent").await;
    let received = app.get("/api/contact/interest/received").await;
    let body: serde_json::Value = received.json().await.unwrap();
    assert_eq!(body["pagination"]["total"], 1);
    assert_eq!(body["data"][0]["status"], "pending");
}

#[tokio::test]
async fn test_accept_interest_opens_conversation() {
    let app = common::TestApp::spawn().await;

    // Setup talent + enterprise + interest
    let talent = app.register_user("accepttalent").await;
    let talent_id = talent["data"]["user"]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'accepttalent'")
        .execute(&app.db)
        .await
        .unwrap();

    app.register_enterprise("AcceptCorp").await;
    seed_enterprise_credits(&app, "AcceptCorp").await;
    app.login("acceptcorp").await;
    app.post(
        "/api/contact/interest",
        &json!({ "talent_id": talent_id, "message": "Hello!" }),
    )
    .await;

    // Accept as talent
    app.login("accepttalent").await;
    let received = app.get("/api/contact/interest/received").await;
    let body: serde_json::Value = received.json().await.unwrap();
    let request_id = body["data"][0]["id"].as_str().unwrap();

    let accept_resp = app
        .post(
            &format!("/api/contact/interest/{request_id}/accept"),
            &json!({}),
        )
        .await;
    assert_eq!(accept_resp.status(), StatusCode::OK);

    let accept_body: serde_json::Value = accept_resp.json().await.unwrap();
    assert!(accept_body["data"]["conversation"]["id"].is_string());

    // Check conversations
    let convs = app.get("/api/contact/conversations").await;
    let convs_body: serde_json::Value = convs.json().await.unwrap();
    assert_eq!(
        convs_body["data"]["conversations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_decline_sets_cooldown() {
    let app = common::TestApp::spawn().await;

    let talent = app.register_user("declinetalent").await;
    let talent_id = talent["data"]["user"]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'declinetalent'")
        .execute(&app.db)
        .await
        .unwrap();

    app.register_enterprise("DeclineCorp").await;
    seed_enterprise_credits(&app, "DeclineCorp").await;
    app.login("declinecorp").await;
    app.post(
        "/api/contact/interest",
        &json!({ "talent_id": talent_id, "message": "Hello!" }),
    )
    .await;

    // Decline
    app.login("declinetalent").await;
    let received = app.get("/api/contact/interest/received").await;
    let body: serde_json::Value = received.json().await.unwrap();
    let request_id = body["data"][0]["id"].as_str().unwrap();

    let decline_resp = app
        .post(
            &format!("/api/contact/interest/{request_id}/decline"),
            &json!({}),
        )
        .await;
    assert_eq!(decline_resp.status(), StatusCode::OK);

    // Try to re-send interest — should be blocked by cooldown
    app.login("declinecorp").await;
    let retry = app
        .post(
            "/api/contact/interest",
            &json!({ "talent_id": talent_id, "message": "Try again" }),
        )
        .await;
    assert_eq!(retry.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_block_enterprise() {
    let app = common::TestApp::spawn().await;

    let talent = app.register_user("blocktalent").await;
    let talent_id = talent["data"]["user"]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'blocktalent'")
        .execute(&app.db)
        .await
        .unwrap();

    app.register_enterprise("BlockCorp").await;
    app.login("blockcorp").await;

    let enterprise_id: String =
        sqlx::query_scalar("SELECT id::TEXT FROM enterprises WHERE company_name = 'BlockCorp'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Block
    app.login("blocktalent").await;
    let block_resp = app
        .post(&format!("/api/contact/block/{enterprise_id}"), &json!({}))
        .await;
    assert_eq!(block_resp.status(), StatusCode::OK);

    // Enterprise tries to send interest — should be blocked
    app.login("blockcorp").await;
    let interest = app
        .post(
            "/api/contact/interest",
            &json!({ "talent_id": talent_id, "message": "Hey" }),
        )
        .await;
    assert_eq!(interest.status(), StatusCode::FORBIDDEN);
}
