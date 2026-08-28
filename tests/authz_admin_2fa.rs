//! AZ-03 — the admin 2FA gate holds.
//!
//! THREAT_MODEL: an administrator with no second factor is refused at the
//! admin portal (`AdminTwoFaSetupRequired`), not merely nagged. The `AdminGate`
//! extractor enforces it by looking up (role, totp_enabled, has_passkey) on
//! every gated admin route. This proves both directions: an admin with no
//! factor is stopped with the specific 2FA code, and an admin with one is not.
//!
//! The check is a DB lookup, so it holds regardless of the connection's
//! privileges (unlike the audit-log REVOKE, which a superuser test role
//! bypasses — see AZ-04's note).

mod common;

use common::TestApp;
use serde_json::Value;

const GATED_ROUTE: &str = "/api/admin/challenges"; // list_all_challenges, `_gate: AdminGate`

/// An admin with the capability but no second factor — register_admin without
/// the passkey it normally inserts to satisfy the gate.
async fn admin_without_2fa(app: &TestApp, username: &str) {
    let r = app.register_user(username).await;
    let uid = r["data"]["user"]["id"]
        .as_str()
        .expect("user id")
        .to_string();
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1::UUID")
        .bind(&uid)
        .execute(&app.db)
        .await
        .expect("set admin role");
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1::UUID, 'admin', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(&uid)
    .execute(&app.db)
    .await
    .expect("grant admin capability");
    // Deliberately no webauthn_credentials row and no totp — no second factor.
    app.login(username).await;
}

#[tokio::test]
async fn an_admin_without_a_second_factor_is_refused_at_the_gate() {
    let app = TestApp::spawn().await;

    admin_without_2fa(&app, "no2fa_admin").await;
    let resp = app.get(GATED_ROUTE).await;
    let status = resp.status();
    let body: Value = resp.json().await.expect("json error body");
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "an admin without 2FA reached a gated route: {body}"
    );
    assert_eq!(
        body["error"]["code"], "AUTH_ADMIN_2FA_SETUP_REQUIRED",
        "expected the 2FA gate, got {body}"
    );

    // Positive control: register_admin adds a passkey, so the same admin on the
    // same route is not stopped by the gate.
    app.register_admin("with2fa_admin").await;
    let resp = app.get(GATED_ROUTE).await;
    assert!(
        resp.status().is_success(),
        "an admin with a second factor was blocked from a gated route: {}",
        resp.status()
    );
}
