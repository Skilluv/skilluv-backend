//! End-to-end OIDC roundtrip test for enterprise B2B SSO.
//!
//! Drives the real openidconnect flow (`/start` → mock IdP `/authorize` →
//! `/callback` with signed ID token) against the mock IdP defined in
//! `tests/common/mock_oidc.rs`. Cookies are set on the test host after the
//! last hop, so we can also verify the resulting session (`/auth/me`) and the
//! JIT-provisioned user in the database.

mod common;

use common::mock_oidc::MockIdp;
use reqwest::redirect::Policy;
use reqwest::{Client, StatusCode};
use serde_json::json;

fn manual_client(app: &common::TestApp) -> Client {
    // We must NOT follow redirects automatically: two of the three hops target
    // a different host (the mock IdP), and we need to inspect each Location
    // header to extract the authorize URL and the callback URL. Cookies are
    // still shared across hops via the cookie store.
    let _ = app; // suppress unused when tests grow
    Client::builder()
        .cookie_store(true)
        .redirect(Policy::none())
        .build()
        .expect("build manual client")
}

async fn setup_sso(app: &common::TestApp, slug: &str, idp: &MockIdp, enforce: bool) {
    // Owner-authenticated write path for the SSO config.
    let resp = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": idp.base_url,
                "client_id": idp.client_id,
                "client_secret": idp.client_secret,
                "email_domains": [format!("{slug}.example")],
                "enforce_sso": enforce,
                "auto_provision": true,
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "SSO config upsert failed: {}",
        resp.text().await.unwrap_or_default()
    );
}

fn extract_location(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .expect("no Location header")
        .to_string()
}

#[tokio::test]
async fn test_full_oidc_roundtrip_jit_provisions_recruiter() {
    let app = common::TestApp::spawn().await;
    let idp = MockIdp::spawn("jane@sso-e2e.example", "Jane Doe").await;

    app.register_enterprise("SsoE2ECorp").await;
    app.login("ssoe2ecorp").await;
    app.enable_totp_for("ssoe2ecorp").await;
    // Slug is derived from the company name via slugify() → "ssoe2ecorp".
    setup_sso(&app, "ssoe2ecorp", &idp, false).await;

    // Fresh client with cookie jar, manual redirect handling. No auth cookie.
    let client = manual_client(&app);

    // ── Hop 1: /start ────────────────────────────────────────────
    let start_resp = client
        .get(format!("{}/api/enterprise/sso/ssoe2ecorp/start", app.addr))
        .send()
        .await
        .expect("start");
    assert!(
        start_resp.status().is_redirection(),
        "expected redirect from /start, got {}",
        start_resp.status()
    );
    let authorize_url = extract_location(&start_resp);
    assert!(
        authorize_url.starts_with(&format!("{}/authorize", idp.base_url)),
        "expected authorize URL to point at mock IdP; got {authorize_url}"
    );

    // ── Hop 2: mock IdP /authorize ───────────────────────────────
    let authz_resp = client.get(&authorize_url).send().await.expect("authorize");
    assert!(
        authz_resp.status().is_redirection(),
        "mock authorize did not redirect: status {}, body {:?}",
        authz_resp.status(),
        authz_resp.text().await.ok()
    );
    let callback_url = extract_location(&authz_resp);
    assert!(
        callback_url.contains("/api/enterprise/sso/ssoe2ecorp/callback"),
        "authorize should redirect to our callback ; got {callback_url}"
    );

    // ── Hop 3: our /callback exchanges the code with the IdP token endpoint
    // and mints a session ───────────────────────────────────────
    let callback_resp = client.get(&callback_url).send().await.expect("callback");
    assert!(
        callback_resp.status().is_redirection(),
        "callback did not redirect: status {}, body {:?}",
        callback_resp.status(),
        callback_resp.text().await.ok()
    );

    // Session cookies must be set on the callback response.
    let set_cookies: Vec<String> = callback_resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok().map(|s| s.to_string()))
        .collect();
    assert!(
        set_cookies.iter().any(|c| c.starts_with("access_token=")),
        "expected access_token cookie in {:?}",
        set_cookies
    );
    assert!(
        set_cookies.iter().any(|c| c.starts_with("refresh_token=")),
        "expected refresh_token cookie in {:?}",
        set_cookies
    );

    // ── JIT provisioning verified in DB ──────────────────────────
    let user: (uuid::Uuid, String, String) = sqlx::query_as(
        "SELECT id, email, role FROM users WHERE LOWER(email) = 'jane@sso-e2e.example'",
    )
    .fetch_one(&app.db)
    .await
    .expect("provisioned user missing");
    assert_eq!(user.1, "jane@sso-e2e.example");
    assert_eq!(user.2, "recruiter");

    let member: (String,) =
        sqlx::query_as("SELECT em.status FROM enterprise_members em WHERE em.user_id = $1")
            .bind(user.0)
            .fetch_one(&app.db)
            .await
            .expect("membership missing");
    assert_eq!(member.0, "active");

    // Session was created with login_method='sso'.
    let session_method: (String,) = sqlx::query_as(
        "SELECT login_method FROM user_sessions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user.0)
    .fetch_one(&app.db)
    .await
    .expect("session missing");
    assert_eq!(session_method.0, "sso");
}

#[tokio::test]
async fn test_sso_bypasses_totp_gate() {
    let app = common::TestApp::spawn().await;
    let idp = MockIdp::spawn("bob@bypass.example", "Bob").await;

    app.register_enterprise("BypassCorp").await;
    app.login("bypasscorp").await;
    app.enable_totp_for("bypasscorp").await;
    setup_sso(&app, "bypasscorp", &idp, false).await;

    // Roundtrip logs Bob in via SSO. The full flow is exercised by the other
    // test ; here we care about the session's TOTP gate.
    let client = manual_client(&app);
    let r1 = client
        .get(format!("{}/api/enterprise/sso/bypasscorp/start", app.addr))
        .send()
        .await
        .unwrap();
    let r2 = client.get(extract_location(&r1)).send().await.unwrap();
    let r3 = client.get(extract_location(&r2)).send().await.unwrap();
    assert!(r3.status().is_redirection());

    // Bob's totp_enabled is false (fresh JIT), yet /enterprise/profile must
    // succeed because his session's login_method='sso' bypasses the gate.
    let profile = client
        .get(format!("{}/api/enterprise/profile", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(
        profile.status(),
        StatusCode::OK,
        "SSO session should bypass the mandatory-TOTP gate; got {} : {}",
        profile.status(),
        profile.text().await.unwrap_or_default()
    );
}

#[tokio::test]
async fn test_sso_wrong_client_secret_fails() {
    let app = common::TestApp::spawn().await;
    let idp = MockIdp::spawn("mallory@wrong.example", "Mallory").await;

    app.register_enterprise("WrongSecretCorp").await;
    app.login("wrongsecretcorp").await;
    app.enable_totp_for("wrongsecretcorp").await;

    // Configure SSO with a client_secret that does NOT match what the mock IdP expects.
    let resp = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": idp.base_url,
                "client_id": idp.client_id,
                "client_secret": "totally-wrong-secret",
                "email_domains": ["wrong.example"],
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let client = manual_client(&app);
    let r1 = client
        .get(format!(
            "{}/api/enterprise/sso/wrongsecretcorp/start",
            app.addr
        ))
        .send()
        .await
        .unwrap();
    let r2 = client.get(extract_location(&r1)).send().await.unwrap();
    let callback = client.get(extract_location(&r2)).send().await.unwrap();

    // Callback must reject: token exchange fails with the mock IdP.
    assert!(
        callback.status().is_server_error() || callback.status() == StatusCode::UNAUTHORIZED,
        "expected failure status ; got {}",
        callback.status()
    );

    // No user was provisioned.
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE LOWER(email) = 'mallory@wrong.example'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count.0, 0);
}
