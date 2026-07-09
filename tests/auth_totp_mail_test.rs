//! End-to-end auth tests that hit Mailpit for email flows and compute live TOTP codes.
//!
//! Prereqs (started via `docker compose up -d`):
//! - Postgres :5433
//! - Redis :6379
//! - Mailpit SMTP :1025 / HTTP UI :8025

mod common;

use reqwest::StatusCode;
use serde_json::json;

use common::{Mailpit, TestApp, totp_now};

// ─── TOTP ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_totp_setup_enable_login_and_disable() {
    let app = TestApp::spawn().await;
    app.register_user("totpguy").await;

    // Setup — returns otpauth_url + secret_base32.
    let setup: serde_json::Value = app
        .post("/api/auth/totp/setup", &json!({}))
        .await
        .json()
        .await
        .unwrap();
    let secret = setup["data"]["secret_base32"]
        .as_str()
        .expect("secret_base32 missing")
        .to_string();
    assert!(setup["data"]["otpauth_url"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/"));

    // Enable with the current code.
    let code = totp_now(&secret);
    let enable_resp: serde_json::Value = app
        .post("/api/auth/totp/enable", &json!({ "code": code }))
        .await
        .json()
        .await
        .unwrap();
    let backup_codes = enable_resp["data"]["backup_codes"]
        .as_array()
        .cloned()
        .expect("backup_codes not returned on enable");
    assert_eq!(backup_codes.len(), 10, "10 backup codes expected");

    // Login without TOTP code → TotpRequired.
    let fresh = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "totpguy",
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    // TotpRequired maps to 403 (see errors/codes.rs).
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Login WITH a valid live TOTP code succeeds.
    let code = totp_now(&secret);
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "totpguy",
            "password": TestApp::TEST_PASSWORD,
            "totp_code": code,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Disable with a fresh code.
    // Small delay so we don't hit the same 30-sec window twice in a row (harmless but cleaner).
    let disable_code = totp_now(&secret);
    let resp = app
        .post(
            "/api/auth/totp/disable",
            &json!({ "code": disable_code }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_totp_backup_code_consumes_and_cannot_be_reused() {
    let app = TestApp::spawn().await;
    app.register_user("bkuser").await;

    let setup: serde_json::Value = app
        .post("/api/auth/totp/setup", &json!({}))
        .await
        .json()
        .await
        .unwrap();
    let secret = setup["data"]["secret_base32"].as_str().unwrap().to_string();

    let enable: serde_json::Value = app
        .post(
            "/api/auth/totp/enable",
            &json!({ "code": totp_now(&secret) }),
        )
        .await
        .json()
        .await
        .unwrap();
    let backup_codes = enable["data"]["backup_codes"]
        .as_array()
        .cloned()
        .unwrap();
    let first_code = backup_codes[0].as_str().unwrap().to_string();

    // Login with the backup code (fresh client, needs 2FA).
    let fresh = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "bkuser",
            "password": TestApp::TEST_PASSWORD,
            "backup_code": first_code,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Reuse the SAME code → rejected.
    let fresh2 = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh2
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "bkuser",
            "password": TestApp::TEST_PASSWORD,
            "backup_code": first_code,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Email verification via Mailpit ──────────────────────────────

#[tokio::test]
async fn test_email_verification_link() {
    let mp = Mailpit::new();
    mp.wipe().await;

    let app = TestApp::spawn().await;
    app.register_user("verifme").await;

    let msg = mp.wait_for("verifme@test.com", 5_000).await;
    let token = Mailpit::extract_token(&msg, "token").expect("no token in verify email");

    let resp = app
        .get(&format!("/api/auth/verify-email?token={token}"))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // /me should now report email_verified.
    let me: serde_json::Value = app.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(me["data"]["user"]["email_verified"], true);
}

// ─── Magic link end-to-end ────────────────────────────────────────

#[tokio::test]
async fn test_magic_link_login_flow() {
    let mp = Mailpit::new();
    mp.wipe().await;

    let app = TestApp::spawn().await;
    // Signup a user via magic link (no password needed for consume side).
    let email = "magic@test.com";

    // 1. Request signup link.
    let resp = app
        .post(
            "/api/auth/magic-link/request",
            &json!({ "email": email, "intent": "signup" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Fetch email, extract token.
    let msg = mp.wait_for(email, 5_000).await;
    let token = Mailpit::extract_token(&msg, "token").expect("no token in magic link email");

    // 3. Consume via a fresh client (no jar contamination).
    let fresh = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/magic-link/consume", app.addr))
        .json(&json!({ "token": token }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. /me must return the freshly created user.
    let resp = fresh
        .get(format!("{}/api/auth/me", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["user"]["email"], email);
}

// ─── Forgot / reset password end-to-end ──────────────────────────

#[tokio::test]
async fn test_forgot_and_reset_password_full_flow() {
    let mp = Mailpit::new();
    mp.wipe().await;

    let app = TestApp::spawn().await;
    app.register_user("forgotguy").await;

    // Request reset.
    let resp = app
        .post(
            "/api/auth/forgot-password",
            &json!({ "email": "forgotguy@test.com" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Two emails: the verification (from register) and the reset. Pick the one that has
    // a `?token=...&` URL matching the reset link (contains `/reset-password`).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    let token = loop {
        let msg = mp.wait_for("forgotguy@test.com", 5_000).await;
        // Look for reset-password link. If we caught the verify email, wipe & wait again.
        let html = msg["HTML"].as_str().unwrap_or_default();
        if html.contains("reset-password") {
            break Mailpit::extract_token(&msg, "token").expect("no token in reset email");
        }
        // Otherwise wipe and retry.
        mp.wipe().await;
        if std::time::Instant::now() >= deadline {
            panic!("no reset-password email seen");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    };

    let new_pw = "BrandNewPass!42";
    let resp = app
        .post(
            "/api/auth/reset-password",
            &json!({ "token": token, "new_password": new_pw }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Login with the new password from a fresh client (old sessions revoked).
    let fresh = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "forgotguy",
            "password": new_pw,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─── Email 2FA end-to-end ────────────────────────────────────────

#[tokio::test]
async fn test_email_2fa_login_flow() {
    let mp = Mailpit::new();
    mp.wipe().await;

    let app = TestApp::spawn().await;
    app.register_user("email2fa").await;

    // Verify email first (email_2fa/enable requires email_verified).
    let verify_msg = mp.wait_for("email2fa@test.com", 5_000).await;
    let vtoken =
        Mailpit::extract_token(&verify_msg, "token").expect("verify token missing");
    let resp = app
        .get(&format!("/api/auth/verify-email?token={vtoken}"))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Enable email 2FA on the current session.
    let resp = app.post("/api/auth/email-2fa/enable", &json!({})).await;
    assert_eq!(resp.status(), StatusCode::OK);

    mp.wipe().await;

    // Fresh client logs in → should receive `requires_email_2fa` + a code by email.
    let fresh = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": "email2fa",
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["requires_email_2fa"], true);
    let user_id = body["data"]["user_id"]
        .as_str()
        .expect("user_id missing")
        .to_string();

    // Fetch the code sent by email.
    let code_msg = mp.wait_for("email2fa@test.com", 5_000).await;
    let code = Mailpit::extract_6digit_code(&code_msg).expect("no 6-digit code in email");

    // Complete the flow.
    let resp = fresh
        .post(format!("{}/api/auth/email-2fa/verify", app.addr))
        .json(&json!({ "code": code, "user_id": user_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─── Change email end-to-end ─────────────────────────────────────

#[tokio::test]
async fn test_change_email_end_to_end() {
    let mp = Mailpit::new();
    mp.wipe().await;

    let app = TestApp::spawn().await;
    app.register_user("mover").await;

    let new_email = "moved@test.com";
    let resp = app
        .post(
            "/api/auth/change-email",
            &json!({
                "current_password": TestApp::TEST_PASSWORD,
                "new_email": new_email,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Fetch the confirmation email (goes to the NEW address).
    let msg = mp.wait_for(new_email, 5_000).await;
    let token = Mailpit::extract_token(&msg, "token").expect("no token in confirm email");

    // Anyone can confirm — the token itself is the capability.
    let confirm_client = reqwest::Client::new();
    let resp = confirm_client
        .get(format!(
            "{}/api/auth/change-email/confirm?token={token}",
            app.addr
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Login with the new email works.
    let fresh = reqwest::Client::builder().cookie_store(true).build().unwrap();
    let resp = fresh
        .post(format!("{}/api/auth/login", app.addr))
        .json(&json!({
            "identifier": new_email,
            "password": TestApp::TEST_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
