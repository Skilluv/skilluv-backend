//! SKI-58 — every `GET /admin/*` that returns a list must answer
//! `{data: [...], pagination: {page, per_page, total, total_pages}, meta}`.
//!
//! When a handler wraps its list in a named key instead (`{data: {queue: []}}`),
//! nothing errors: the admin panel iterates an object, finds nothing, and renders
//! an empty table. The SSO-sessions bug went unnoticed for exactly that reason,
//! so the convention is asserted here rather than left to review.

mod common;
use common::TestApp;

async fn setup_admin(app: &TestApp, username: &str) {
    app.register_admin(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO webauthn_credentials (user_id, credential_id, credential, label)
         VALUES ($1, $2, '{\"stub\":true}'::jsonb, 'test-passkey')",
    )
    .bind(uid)
    .bind(format!("cred-{uid}").into_bytes())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();
    app.login(username).await;
}

async fn admin_get(app: &TestApp, path: &str) -> reqwest::Response {
    app.client
        .get(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap()
}

async fn assert_listing_convention(app: &TestApp, path: &str) {
    let resp = admin_get(app, path).await;
    assert_eq!(resp.status().as_u16(), 200, "{path} should answer 200");

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["data"].is_array(),
        "{path}: `data` must be the array itself, got {}",
        body["data"]
    );
    for key in ["page", "per_page", "total", "total_pages"] {
        assert!(
            body["pagination"][key].is_number(),
            "{path}: missing pagination.{key}"
        );
    }
    assert!(
        body["meta"]["request_id"].is_string(),
        "{path}: missing meta"
    );
}

/// Endpoints converted for SKI-58. They live outside the `admin_*` route files,
/// which is why the earlier audit missed them. The fourth,
/// `/admin/enterprises/{id}/agency-clients`, needs a seeded enterprise and is
/// covered in `test_adm_m4_enterprises.rs`.
#[tokio::test]
async fn admin_list_endpoints_follow_the_data_pagination_convention() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_conv").await;

    assert_listing_convention(&app, "/api/admin/enterprise-kyc").await;
    assert_listing_convention(&app, "/api/admin/tenants").await;
    assert_listing_convention(&app, "/api/admin/sponsored-challenges").await;
}

/// Endpoints already conforming before SKI-58 — asserted so a future refactor
/// cannot quietly regress them back to a wrapped shape.
#[tokio::test]
async fn previously_conforming_admin_listings_stay_conforming() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_conv2").await;

    assert_listing_convention(&app, "/api/admin/users").await;
    assert_listing_convention(&app, "/api/admin/skills").await;
    assert_listing_convention(&app, "/api/admin/enterprises").await;
    assert_listing_convention(&app, "/api/admin/community/review").await;
}

#[tokio::test]
async fn admin_listings_honour_pagination_params() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_conv3").await;

    let resp = admin_get(&app, "/api/admin/tenants?page=2&per_page=5").await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["pagination"]["page"], 2);
    assert_eq!(body["pagination"]["per_page"], 5);
}
