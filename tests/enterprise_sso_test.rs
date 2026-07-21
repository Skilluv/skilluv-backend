mod common;

use reqwest::StatusCode;
use serde_json::json;

/// Full OIDC roundtrip (authorize + callback + JWKS verify) is out of scope for
/// this suite because it requires mocking an IdP. These tests cover:
///   - Config CRUD gated on enterprise owner
///   - Discovery by email domain
///   - `enforce_sso` blocking password login with the correct error payload
///   - Config secret is never returned in cleartext

#[tokio::test]
async fn test_sso_config_owner_only() {
    let app = common::TestApp::spawn().await;

    // A regular user cannot upsert an SSO config.
    app.register_user("normal_user").await;
    app.login("normal_user").await;
    let resp = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": "https://accounts.google.com",
                "client_id": "test-client",
                "client_secret": "super-secret",
                "email_domains": ["acme.com"],
            }),
        )
        .await;
    // Non-owner is refused. `require_enterprise` returns NotFound (404) when the
    // user has no enterprise membership at all, or Forbidden (403) when they do
    // but are not the owner. Both are correct outcomes for this test.
    assert!(
        resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::NOT_FOUND,
        "expected 403 or 404, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_sso_config_upsert_and_get() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("SsoCorp").await;
    app.login("ssocorp").await;
    app.enable_totp_for("ssocorp").await;

    let resp = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": "https://accounts.google.com",
                "client_id": "test-client-id",
                "client_secret": "super-secret-do-not-leak",
                "email_domains": ["ssocorp.example", "SSOCORP.CO"],
                "enforce_sso": false,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["config"]["issuer"],
        "https://accounts.google.com"
    );
    assert_eq!(body["data"]["config"]["client_secret"], "***REDACTED***");
    // Domains normalised to lowercase.
    let domains = body["data"]["config"]["email_domains"].as_array().unwrap();
    assert!(domains.iter().any(|d| d == "ssocorp.example"));
    assert!(domains.iter().any(|d| d == "ssocorp.co"));

    let get_resp = app.get("/api/enterprise/sso/config").await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body: serde_json::Value = get_resp.json().await.unwrap();
    assert_eq!(get_body["data"]["config"]["client_id"], "test-client-id");
    assert_eq!(
        get_body["data"]["config"]["client_secret"],
        "***REDACTED***"
    );
}

#[tokio::test]
async fn test_sso_discover_by_email_domain() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("DiscoverCorp").await;
    app.login("discovercorp").await;
    app.enable_totp_for("discovercorp").await;

    let _ = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": "https://accounts.google.com",
                "client_id": "cid",
                "client_secret": "csec",
                "email_domains": ["discover-me.io"],
            }),
        )
        .await;

    // Match → sso_available true + start_url present.
    let match_resp = app
        .get("/api/enterprise/sso/discover?email=jane@discover-me.io")
        .await;
    assert_eq!(match_resp.status(), StatusCode::OK);
    let match_body: serde_json::Value = match_resp.json().await.unwrap();
    assert_eq!(match_body["data"]["sso_available"], true);
    let start_url = match_body["data"]["start_url"].as_str().unwrap();
    assert!(start_url.contains("/api/enterprise/sso/discovercorp/start"));

    // No match → false.
    let miss_resp = app
        .get("/api/enterprise/sso/discover?email=elsewhere@gmail.com")
        .await;
    let miss_body: serde_json::Value = miss_resp.json().await.unwrap();
    assert_eq!(miss_body["data"]["sso_available"], false);
}

#[tokio::test]
async fn test_enforce_sso_blocks_password_login() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("EnforceCorp").await;
    app.login("enforcecorp").await;
    app.enable_totp_for("enforcecorp").await;

    // Configure SSO with enforce_sso=true on a domain and register a user with that domain.
    let _ = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": "https://accounts.google.com",
                "client_id": "cid",
                "client_secret": "csec",
                "email_domains": ["forced-sso.co"],
                "enforce_sso": true,
            }),
        )
        .await;

    // Register a user whose email domain matches — done via direct DB insert since
    // the standard register helper uses @skilluv.test.
    let user_id = uuid::Uuid::new_v4();
    let password_hash =
        "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$hV5eBz2Utw24oJ48kmUZ9lJf3jV+VMuiMONHrGYA/S8";
    sqlx::query(
        "INSERT INTO users (id, email, username, password_hash, first_name, last_name, display_name, role, email_verified) VALUES ($1, $2, $3, $4, 'A', 'B', 'A B', 'user', TRUE)",
    )
    .bind(user_id)
    .bind("alice@forced-sso.co")
    .bind("alice_forced")
    .bind(password_hash)
    .execute(&app.db)
    .await
    .unwrap();

    // Logout the current owner session so the login endpoint sees no cookies.
    let logout = app.post("/api/auth/logout", &json!({})).await;
    let _ = logout;

    let resp = app
        .post(
            "/api/auth/login",
            &json!({
                "identifier": "alice@forced-sso.co",
                "password": common::TestApp::TEST_PASSWORD,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "AUTH_SSO_REQUIRED");
    assert!(
        body["error"]["start_url"]
            .as_str()
            .unwrap()
            .contains("/api/enterprise/sso/enforcecorp/start"),
        "expected start_url to point at enforcecorp; got {:?}",
        body["error"]["start_url"]
    );
}

#[tokio::test]
async fn test_sso_disable() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("DisableCorp").await;
    app.login("disablecorp").await;
    app.enable_totp_for("disablecorp").await;

    let _ = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": "https://accounts.google.com",
                "client_id": "cid",
                "client_secret": "csec",
                "email_domains": ["disable.example"],
            }),
        )
        .await;

    let del = app.delete("/api/enterprise/sso/config").await;
    assert_eq!(del.status(), StatusCode::OK);

    // After disable, discovery must not match anymore.
    let disc = app
        .get("/api/enterprise/sso/discover?email=someone@disable.example")
        .await;
    let body: serde_json::Value = disc.json().await.unwrap();
    assert_eq!(body["data"]["sso_available"], false);
}
