mod common;

use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_register_enterprise() {
    let app = common::TestApp::spawn().await;
    let result = app.register_enterprise("TestCorp").await;

    assert_eq!(result["data"]["enterprise"]["company_name"], "TestCorp");
    assert_eq!(result["data"]["user"]["role"], "enterprise");
}

#[tokio::test]
async fn test_enterprise_profile() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("ProfileCorp").await;
    app.login("profilecorp").await;
    app.enable_totp_for("profilecorp").await;

    let resp = app.get("/api/enterprise/profile").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["enterprise"]["company_name"], "ProfileCorp");
    assert_eq!(body["data"]["member_count"], 1);
}

#[tokio::test]
async fn test_enterprise_webauthn_session_bypasses_totp_gate() {
    // WebAuthn is a strong factor (device + biometric, phishing-resistant),
    // so a session labelled `webauthn` in user_sessions should satisfy the
    // enterprise TOTP requirement without an explicit TOTP setup.
    let app = common::TestApp::spawn().await;
    app.register_enterprise("StrongCorp").await;
    app.login("strongcorp").await;

    // The register helper flips email_verified=true ; deliberately leave
    // totp_enabled=false so we're isolating the strong-factor bypass.
    // Simulate a webauthn login by rewriting the login_method on the current
    // session (in real life this happens via /auth/webauthn/login/finish).
    let user_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM users WHERE username = 'strongcorp'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE user_sessions SET login_method = 'webauthn' WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id.0)
    .execute(&app.db)
    .await
    .unwrap();
    // The client's access token was minted with login_method='password' by the
    // login helper — for the JWT claim to reflect the new session label we
    // rotate through /auth/refresh, which pulls the updated method.
    let refresh = app.post("/api/auth/refresh", &json!({})).await;
    assert_eq!(refresh.status(), StatusCode::OK);
    let refresh_body: serde_json::Value = refresh.json().await.unwrap();
    assert_eq!(refresh_body["data"]["login_method"], "webauthn");

    // No TOTP configured, but the enterprise workspace is accessible because
    // the session is on a strong factor.
    let profile = app.get("/api/enterprise/profile").await;
    assert_eq!(profile.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_enterprise_magic_link_session_still_needs_totp() {
    // Magic link only proves email possession — it's a low-assurance factor
    // and MUST NOT bypass the mandatory-TOTP gate on enterprise/recruiter
    // accounts. This test locks in the policy so a future "just add
    // 'magic_link' to the strong factors" refactor is caught.
    let app = common::TestApp::spawn().await;
    app.register_enterprise("MagicCorp").await;
    app.login("magiccorp").await;

    let user_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM users WHERE username = 'magiccorp'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE user_sessions SET login_method = 'magic_link' WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id.0)
    .execute(&app.db)
    .await
    .unwrap();
    let refresh = app.post("/api/auth/refresh", &json!({})).await;
    assert_eq!(refresh.status(), StatusCode::OK);

    let profile = app.get("/api/enterprise/profile").await;
    assert_eq!(profile.status(), StatusCode::FORBIDDEN);
    let err: serde_json::Value = profile.json().await.unwrap();
    assert_eq!(err["error"]["code"], "AUTH_TOTP_SETUP_REQUIRED");
}

#[tokio::test]
async fn test_enterprise_routes_require_totp_setup() {
    let app = common::TestApp::spawn().await;

    // Register bypassing the helper so we DON'T flip totp_enabled=true.
    let resp = app
        .post(
            "/api/enterprise/register",
            &json!({
                "email": "notptcorp@enterprise.com",
                "username": "notptcorp",
                "password": common::TestApp::TEST_PASSWORD,
                "first_name": "No",
                "last_name": "Totp",
                "company_name": "NoTotpCorp",
                "company_size": "11-50",
                "terms_accepted": true,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["requires_totp_setup"], true);

    // Force-verify email so we're testing the TOTP gate, not the verification gate.
    sqlx::query("UPDATE users SET email_verified = TRUE WHERE username = 'notptcorp'")
        .execute(&app.db)
        .await
        .unwrap();

    // Accessing an enterprise route without TOTP → 403 TOTP setup required.
    let profile = app.get("/api/enterprise/profile").await;
    assert_eq!(profile.status(), StatusCode::FORBIDDEN);
    let err: serde_json::Value = profile.json().await.unwrap();
    assert_eq!(err["error"]["code"], "AUTH_TOTP_SETUP_REQUIRED");

    // Simulate a completed TOTP setup and retry — now allowed.
    sqlx::query("UPDATE users SET totp_enabled = TRUE WHERE username = 'notptcorp'")
        .execute(&app.db)
        .await
        .unwrap();
    let profile2 = app.get("/api/enterprise/profile").await;
    assert_eq!(profile2.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_bookmark_talent() {
    let app = common::TestApp::spawn().await;

    // Create a talent with active profile
    let talent = app.register_user("talent1").await;
    let talent_id = talent["data"]["user"]["id"].as_str().unwrap();
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'talent1'")
        .execute(&app.db)
        .await
        .unwrap();

    // Register enterprise and bookmark the talent
    app.register_enterprise("BookmarkCorp").await;
    app.login("bookmarkcorp").await;
    app.enable_totp_for("bookmarkcorp").await;

    let resp = app
        .post(
            &format!("/api/enterprise/bookmarks/{talent_id}"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // List bookmarks
    let list_resp = app.get("/api/enterprise/bookmarks").await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    let body: serde_json::Value = list_resp.json().await.unwrap();
    assert_eq!(body["pagination"]["total"], 1);
}

#[tokio::test]
async fn test_invite_accept_email_match() {
    let app = common::TestApp::spawn().await;

    // Owner + candidate (invitee) accounts.
    app.register_enterprise("InviteMatchCorp").await;
    app.login("invitematchcorp").await;
    app.enable_totp_for("invitematchcorp").await;

    let candidate = app.register_user("candidate_ok").await;
    let candidate_email = candidate["data"]["user"]["email"]
        .as_str()
        .unwrap()
        .to_string();

    // Owner sends an invite to the candidate's email. `register_user` above
    // rewrote the cookie jar to the candidate's session — re-login as owner.
    app.relogin_with_totp("invitematchcorp").await;
    let resp = app
        .post(
            "/api/enterprise/invite",
            &json!({ "email": candidate_email }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["data"]["invite_token"].as_str().unwrap().to_string();

    // Wrong user tries to accept → 403.
    let wrong = app.register_user("wrong_user").await;
    let _ = wrong;
    app.login("wrong_user").await;
    let bad = app
        .post("/api/enterprise/invite/accept", &json!({ "token": token }))
        .await;
    assert_eq!(bad.status(), StatusCode::FORBIDDEN);

    // Correct user accepts → 200 and becomes recruiter.
    app.login("candidate_ok").await;
    let ok = app
        .post("/api/enterprise/invite/accept", &json!({ "token": token }))
        .await;
    assert_eq!(ok.status(), StatusCode::OK);

    let role: (String,) = sqlx::query_as("SELECT role FROM users WHERE username = 'candidate_ok'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(role.0, "recruiter");

    let member: (String,) = sqlx::query_as(
        "SELECT em.status FROM enterprise_members em JOIN users u ON u.id = em.user_id WHERE u.username = 'candidate_ok'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(member.0, "active");
}

#[tokio::test]
async fn test_invite_accept_invalid_token() {
    let app = common::TestApp::spawn().await;
    app.register_user("solo").await;
    app.login("solo").await;

    let resp = app
        .post(
            "/api/enterprise/invite/accept",
            &json!({ "token": "does-not-exist" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn test_talent_list_crud() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("ListCorp").await;
    app.login("listcorp").await;
    app.enable_totp_for("listcorp").await;

    // Create list
    let create_resp = app
        .post(
            "/api/enterprise/lists",
            &json!({ "name": "Backend Devs", "description": "Top backend talent" }),
        )
        .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    // List all lists
    let list_resp = app.get("/api/enterprise/lists").await;
    let body: serde_json::Value = list_resp.json().await.unwrap();
    assert_eq!(body["data"]["lists"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["lists"][0]["name"], "Backend Devs");
}
