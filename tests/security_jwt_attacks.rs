//! DA-05 — the JWT verifier rejects the classic forgeries.
//!
//! We issue session tokens, so the token is a direct attack surface. The
//! verifier uses `Validation::default()` (HS256 + exp), which should refuse an
//! `alg:none` token, a token signed with another key, and an expired one, while
//! accepting a correctly signed, current one. This presents each as the
//! `access_token` cookie on a bare client and checks the outcome.

mod common;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::TestApp;
use serde_json::{Value, json};
use skilluv_backend::services::AuthService;

const SECRET: &str = "test-secret-key-for-testing"; // matches tests/common/mod.rs
const AUTHED_ROUTE: &str = "/api/auth/me";

/// Present exactly one forged token, no cookie jar, admin origin so only the
/// token decides.
async fn get_with_token(app: &TestApp, token: &str) -> reqwest::StatusCode {
    reqwest::Client::new()
        .get(format!("{}{}", app.addr, AUTHED_ROUTE))
        .header("cookie", format!("access_token={token}"))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .expect("request sent")
        .status()
}

fn alg_none_token(sub: &str) -> String {
    // jsonwebtoken refuses to *encode* alg:none (by design), so hand-build it.
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let now = chrono::Utc::now().timestamp();
    let payload = URL_SAFE_NO_PAD
        .encode(json!({ "sub": sub, "role": "user", "iat": now, "exp": now + 900 }).to_string());
    format!("{header}.{payload}.")
}

fn expired_token(sub: &str) -> String {
    // jsonwebtoken encodes any Serialize; a json! value avoids depending on the
    // private Claims struct. Correctly signed, but exp is in the past.
    let past = chrono::Utc::now().timestamp() - 3600;
    let claims = json!({ "sub": sub, "role": "user", "iat": past, "exp": past + 1 });
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("encode expired token")
}

#[tokio::test]
async fn forged_jwts_are_refused_and_a_valid_one_is_accepted() {
    let app = TestApp::spawn().await;
    let reg = app.register_user("jwt_victim").await;
    let uid = reg["data"]["user"]["id"]
        .as_str()
        .expect("user id")
        .to_string();

    use reqwest::StatusCode;

    // alg:none — the header claims no signature is needed.
    assert_eq!(
        get_with_token(&app, &alg_none_token(&uid)).await,
        StatusCode::UNAUTHORIZED,
        "an alg:none token was accepted"
    );

    // Key confusion — a well-formed HS256 token signed with the wrong secret.
    let wrong_key = AuthService::generate_access_token(
        uid.parse().unwrap(),
        "user",
        "an-entirely-different-secret",
    )
    .unwrap();
    assert_eq!(
        get_with_token(&app, &wrong_key).await,
        StatusCode::UNAUTHORIZED,
        "a token signed with the wrong key was accepted"
    );

    // Expired — correctly signed but past its exp.
    assert_eq!(
        get_with_token(&app, &expired_token(&uid)).await,
        StatusCode::UNAUTHORIZED,
        "an expired token was accepted"
    );

    // Garbage in the cookie is refused, not a 500.
    assert_eq!(
        get_with_token(&app, "not.a.jwt").await,
        StatusCode::UNAUTHORIZED,
        "a malformed token was not cleanly refused"
    );

    // Control: a correctly signed, current token for a real user is accepted.
    let good = AuthService::generate_access_token(uid.parse().unwrap(), "user", SECRET).unwrap();
    let status = get_with_token(&app, &good).await;
    assert!(
        status.is_success(),
        "a valid token was refused ({status}) -- the control failed"
    );
}
