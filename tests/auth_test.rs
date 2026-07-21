mod common;

use reqwest::StatusCode;
use serde_json::json;

use common::TestApp;

// ─── Register ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_register_success() {
    let app = TestApp::spawn().await;
    let result = app.register_user("alice").await;

    assert_eq!(result["data"]["user"]["username"], "alice");
    assert_eq!(result["data"]["user"]["skill_domain"], "code");
    // Vague 1/2: refresh_token is no longer in the body — it lives in an httpOnly cookie.
    assert!(result["data"].get("refresh_token").is_none());
    assert!(result["data"]["csrf_token"].is_string());
}

#[tokio::test]
async fn test_register_duplicate() {
    let app = TestApp::spawn().await;
    app.register_user("bob").await;

    let resp = app
        .post(
            "/api/auth/register",
            &json!({
                "email": "bob2@test.com",
                "username": "bob",
                "password": TestApp::TEST_PASSWORD,
                "first_name": "Bob",
                "last_name": "Two",
                "skill_domain": "code",
                "terms_accepted": true,
            }),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_without_terms_accepted_rejected() {
    let app = TestApp::spawn().await;
    let resp = app
        .post(
            "/api/auth/register",
            &json!({
                "email": "notterms@test.com",
                "username": "notterms",
                "password": TestApp::TEST_PASSWORD,
                "first_name": "No",
                "last_name": "Terms",
                "skill_domain": "code",
                // terms_accepted omitted
            }),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_weak_password_rejected() {
    let app = TestApp::spawn().await;
    let resp = app
        .post(
            "/api/auth/register",
            &json!({
                "email": "weak@test.com",
                "username": "weakpass",
                // Missing symbol.
                "password": "TestPass123",
                "first_name": "Weak",
                "last_name": "Pass",
                "skill_domain": "code",
                "terms_accepted": true,
            }),
        )
        .await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Login ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_login_by_username() {
    let app = TestApp::spawn().await;
    app.register_user("charlie").await;
    let result = app.login("charlie").await;
    assert_eq!(result["data"]["user"]["username"], "charlie");
    assert!(result["data"]["csrf_token"].is_string());
}

#[tokio::test]
async fn test_login_by_email() {
    let app = TestApp::spawn().await;
    app.register_user("diana").await;
    let result = app.login("diana@test.com").await;
    assert_eq!(result["data"]["user"]["username"], "diana");
}

#[tokio::test]
async fn test_login_wrong_password() {
    let app = TestApp::spawn().await;
    app.register_user("eve").await;
    let resp = app
        .post(
            "/api/auth/login",
            &json!({
                "identifier": "eve",
                "password": "WrongPassword!42",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_lockout_after_five_failures() {
    let app = TestApp::spawn().await;
    app.register_user("lockme").await;

    // Fail 5 times → account locked.
    for _ in 0..5 {
        let resp = app
            .post(
                "/api/auth/login",
                &json!({
                    "identifier": "lockme",
                    "password": "WrongPassword!42",
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 6th attempt with the CORRECT password must still be rejected — account is locked.
    let resp = app
        .post(
            "/api/auth/login",
            &json!({
                "identifier": "lockme",
                "password": TestApp::TEST_PASSWORD,
            }),
        )
        .await;
    // Backend responds Validation (400) with a "locked" message.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── /me ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_me_authenticated() {
    let app = TestApp::spawn().await;
    app.register_user("frank").await;
    app.login("frank").await;

    let resp = app.get("/api/auth/me").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["user"]["username"], "frank");
    assert!(body["data"]["rank"].is_object());
}

#[tokio::test]
async fn test_me_unauthenticated() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/auth/me").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Refresh via cookie ───────────────────────────────────────────

#[tokio::test]
async fn test_refresh_rotates_cookie() {
    let app = TestApp::spawn().await;
    app.register_user("refresher").await;
    // Register already set the access + refresh cookies via the cookie jar.

    // First refresh must succeed (cookies are stored in the client jar).
    let resp = app.post("/api/auth/refresh", &json!({})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["ok"], true);
    assert!(body["data"]["csrf_token"].is_string());
}

#[tokio::test]
async fn test_refresh_reuse_detection_revokes_all_sessions() {
    // A rotated refresh token, replayed later, must be flagged as reuse and revoke everything.
    let app = TestApp::spawn().await;

    // Fresh client, so we control exactly which Set-Cookie we see.
    let legit = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let resp = legit
        .post(format!("{}/api/auth/register", app.addr))
        .json(&json!({
            "email": "reused@test.com",
            "username": "reused",
            "password": TestApp::TEST_PASSWORD,
            "first_name": "Re",
            "last_name": "Used",
            "skill_domain": "code",
            "terms_accepted": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    // Snapshot the initial refresh cookie value from the Set-Cookie header.
    let old_refresh = extract_set_cookie_value(&resp, "refresh_token")
        .expect("refresh_token Set-Cookie on register");

    // Rotate as the legitimate client — the jar auto-updates to the new cookie.
    let resp = legit
        .post(format!("{}/api/auth/refresh", app.addr))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Attacker replays the OLD refresh cookie via a bare client.
    let attacker = reqwest::Client::new();
    let resp = attacker
        .post(format!("{}/api/auth/refresh", app.addr))
        .header(
            reqwest::header::COOKIE,
            format!("refresh_token={old_refresh}"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "reuse of a rotated refresh token must be rejected"
    );

    // The legit client's fresh cookie must now also be revoked (family-wide).
    let resp = legit
        .post(format!("{}/api/auth/refresh", app.addr))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "reuse detection must revoke the whole session family"
    );
}

// ─── Sessions ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_sessions_list_and_revoke_all_others() {
    let app = TestApp::spawn().await;
    app.register_user("multidev").await;

    // Simulate a second device by logging in a second time from a separate cookie jar.
    let device_b = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let resp = device_b
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "multidev",
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Device A (the TestApp client) lists sessions — should see 2.
    let list = app.get("/api/auth/sessions").await;
    assert_eq!(list.status(), StatusCode::OK);
    let body: serde_json::Value = list.json().await.unwrap();
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2, "two active sessions expected");
    assert!(body["data"]["current_session_id"].is_string());

    // Revoke everything except current.
    let resp = app.post("/api/auth/sessions/revoke-all", &json!({})).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Device B can no longer refresh.
    let resp = device_b
        .post(format!("{}/api/auth/refresh", app.addr))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Logout ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_logout_revokes_session() {
    let app = TestApp::spawn().await;
    app.register_user("logoutme").await;

    // Refresh works before logout.
    let resp = app.post("/api/auth/refresh", &json!({})).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Logout.
    let resp = app.post("/api/auth/logout", &json!({})).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // /me is unauthorized (access cookie cleared).
    let resp = app.get("/api/auth/me").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Change password ──────────────────────────────────────────────

#[tokio::test]
async fn test_change_password_and_relogin() {
    let app = TestApp::spawn().await;
    app.register_user("changer").await;
    app.login("changer").await;

    let new_pw = "NewPass456!";
    let resp = app
        .post(
            "/api/auth/change-password",
            &json!({
                "current_password": TestApp::TEST_PASSWORD,
                "new_password": new_pw,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Old password must fail.
    let fresh = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "changer",
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // New password succeeds.
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "changer",
            "password": new_pw,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─── Delete account ───────────────────────────────────────────────

#[tokio::test]
async fn test_delete_account() {
    let app = TestApp::spawn().await;
    app.register_user("deleteme").await;

    let resp = app
        .client
        .delete(format!("{}/api/auth/account", app.addr))
        .json(&json!({ "password": TestApp::TEST_PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let login_resp = app
        .post(
            "/api/auth/login",
            &json!({
                "identifier": "deleteme",
                "password": TestApp::TEST_PASSWORD,
            }),
        )
        .await;
    assert_eq!(login_resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Forgot / reset password ──────────────────────────────────────

#[tokio::test]
async fn test_forgot_password_always_ok_for_unknown_email() {
    let app = TestApp::spawn().await;
    let resp = app
        .post(
            "/api/auth/forgot-password",
            &json!({ "email": "nobody@example.com" }),
        )
        .await;
    // Anti-enumeration: always 200 even for unknown email.
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─── Admin ban ────────────────────────────────────────────────────

#[tokio::test]
async fn test_admin_ban_revokes_sessions_and_blocks_login() {
    let app = TestApp::spawn().await;
    let victim = app.register_user("victim").await;
    let victim_id = victim["data"]["user"]["id"].as_str().unwrap().to_string();

    // Fresh admin client so it has its own cookies.
    let admin_client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let resp = admin_client
        .post(format!("{}/api/auth/register", app.addr))
        .json(&json!({
            "email": "admin@test.com",
            "username": "adminuser",
            "password": TestApp::TEST_PASSWORD,
            "first_name": "Admin",
            "last_name": "User",
            "skill_domain": "code",
            "terms_accepted": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    sqlx::query("UPDATE users SET role = 'admin' WHERE username = 'adminuser'")
        .execute(&app.db)
        .await
        .unwrap();
    // Re-login to refresh the JWT with the new role.
    admin_client
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "adminuser",
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();

    let resp = admin_client
        .post(format!("{}/api/admin/users/{victim_id}/ban", app.addr))
        .json(&json!({ "reason": "TOS violation" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Victim can no longer log in.
    let victim_client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let resp = victim_client
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "victim",
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Helpers ──────────────────────────────────────────────────────

/// Extract the value of a named cookie from the response's `Set-Cookie` header(s).
fn extract_set_cookie_value(resp: &reqwest::Response, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    for header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        let s = header.to_str().ok()?;
        for part in s.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix(&prefix) {
                return Some(rest.to_string());
            }
        }
    }
    None
}
