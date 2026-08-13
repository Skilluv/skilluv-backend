//! Contract fixes found by auditing the front against the back, plus the
//! payout paths that failed without telling anyone.
//!
//! Each of the first three was a 422 on a request the client actually sends,
//! so the feature was unusable end to end while every unit test passed —
//! the tests asserted the contract the back had invented for itself.

mod common;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

// ─── BE-P0-03 — enabling email 2FA takes no body ──────────────────

#[tokio::test]
async fn enabling_email_2fa_needs_no_password() {
    let app = TestApp::spawn().await;
    app.register_user("twofa_on").await;
    app.login("twofa_on").await;
    sqlx::query("UPDATE users SET email_verified = TRUE WHERE username = 'twofa_on'")
        .execute(&app.db)
        .await
        .unwrap();

    // What the client sends: nothing at all.
    let resp = app.post("/api/auth/email-2fa/enable", &json!({})).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "turning a second factor on cannot be abused from a stolen session: {}",
        resp.text().await.unwrap_or_default()
    );

    let enabled: bool =
        sqlx::query_scalar("SELECT email_2fa_enabled FROM users WHERE username = 'twofa_on'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(enabled);
}

#[tokio::test]
async fn enabling_email_2fa_still_checks_a_password_when_given_one() {
    let app = TestApp::spawn().await;
    app.register_user("twofa_on2").await;
    app.login("twofa_on2").await;
    sqlx::query("UPDATE users SET email_verified = TRUE WHERE username = 'twofa_on2'")
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app
        .post(
            "/api/auth/email-2fa/enable",
            &json!({ "password": "definitely-not-the-password" }),
        )
        .await;
    assert_eq!(
        resp.status().as_u16(),
        401,
        "a supplied password is verified — optional is not ignored"
    );
}

// ─── BE-P0-04 — the client names it current_password ──────────────

#[tokio::test]
async fn disabling_email_2fa_accepts_the_clients_field_name() {
    let app = TestApp::spawn().await;
    app.register_user("twofa_off").await;
    app.login("twofa_off").await;
    sqlx::query(
        "UPDATE users SET email_verified = TRUE, email_2fa_enabled = TRUE \
         WHERE username = 'twofa_off'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    // The front reuses its change-password form, so the field arrives under
    // its other name. Rejecting a correct password over its label helps
    // nobody.
    let resp = app
        .post(
            "/api/auth/email-2fa/disable",
            &json!({ "current_password": TestApp::TEST_PASSWORD, "new_password": TestApp::TEST_PASSWORD }),
        )
        .await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "body: {}",
        resp.text().await.unwrap_or_default()
    );
}

#[tokio::test]
async fn disabling_email_2fa_still_requires_the_password() {
    let app = TestApp::spawn().await;
    app.register_user("twofa_off2").await;
    app.login("twofa_off2").await;
    sqlx::query(
        "UPDATE users SET email_verified = TRUE, email_2fa_enabled = TRUE \
         WHERE username = 'twofa_off2'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.post("/api/auth/email-2fa/disable", &json!({})).await;
    assert!(
        resp.status().is_client_error(),
        "disabling is a downgrade and stays gated"
    );
}

// ─── BE-P0-02 — totp/disable keeps both factors, but says so ──────

#[tokio::test]
async fn disabling_totp_without_a_password_explains_itself() {
    let app = TestApp::spawn().await;
    app.register_user("totp_off").await;
    app.login("totp_off").await;

    // The client sends only the code. The rule stands — dropping a second
    // factor needs the password too — but the answer has to say that.
    let resp = app
        .post("/api/auth/totp/disable", &json!({ "code": "123456" }))
        .await;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "a serde 422 tells the client nothing actionable"
    );
    let body = resp.text().await.unwrap_or_default();
    assert!(
        body.contains("password"),
        "the message must name what is missing, got: {body}"
    );
}

// ─── BE-P0-12 — Momo withdrawal without an explicit provider ──────

async fn wallet_owner(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    app.login(username).await;
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn registering_a_momo_phone_remembers_the_operator() {
    let app = TestApp::spawn().await;
    let uid = wallet_owner(&app, "momo_reg").await;

    let resp = app
        .post(
            "/api/users/me/wallet/momo/phone",
            &json!({ "phone": "+22997000000", "provider": "mtn" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let stored: Option<String> =
        sqlx::query_scalar("SELECT momo_provider FROM talent_wallets WHERE user_id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some("mtn"),
        "the operator belongs to the number — storing it is what lets the \
         withdrawal endpoint stop demanding it"
    );
}

#[tokio::test]
async fn registering_a_phone_rejects_an_unknown_operator() {
    let app = TestApp::spawn().await;
    wallet_owner(&app, "momo_bad").await;

    let resp = app
        .post(
            "/api/users/me/wallet/momo/phone",
            &json!({ "phone": "+22997000000", "provider": "not-an-operator" }),
        )
        .await;
    assert!(
        resp.status().is_client_error(),
        "a typo caught here beats one discovered on the first payout"
    );
}

#[tokio::test]
async fn a_second_registration_keeps_a_known_operator() {
    let app = TestApp::spawn().await;
    let uid = wallet_owner(&app, "momo_keep").await;

    app.post(
        "/api/users/me/wallet/momo/phone",
        &json!({ "phone": "+22997000000", "provider": "wave" }),
    )
    .await;
    // Same number, no operator this time.
    app.post(
        "/api/users/me/wallet/momo/phone",
        &json!({ "phone": "+22997000001" }),
    )
    .await;

    let stored: Option<String> =
        sqlx::query_scalar("SELECT momo_provider FROM talent_wallets WHERE user_id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some("wave"),
        "blanking it would break the next withdrawal"
    );
}

#[tokio::test]
async fn withdrawing_takes_no_provider_and_no_rail() {
    let app = TestApp::spawn().await;
    let uid = wallet_owner(&app, "momo_wd").await;

    app.post(
        "/api/users/me/wallet/momo/phone",
        &json!({ "phone": "+22997000000", "provider": "mtn" }),
    )
    .await;
    sqlx::query("UPDATE talent_wallets SET residency_country = 'BJ' WHERE user_id = $1")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    // Exactly what a client sends. Which rail reaches Benin is a routing
    // question the server answers, not something a URL should encode.
    let resp = app
        .post(
            "/api/users/me/wallet/withdraw",
            &json!({ "amount": "1000", "currency": "XOF" }),
        )
        .await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    assert_ne!(status, 422, "no field may be missing: {body}");
    assert!(
        !body.contains("missing field"),
        "the client should not have to name a provider: {body}"
    );
}

#[tokio::test]
async fn held_funds_cannot_be_withdrawn() {
    use skilluv_backend::services::ledger::{self, Currency};
    use std::str::FromStr;

    let app = TestApp::spawn().await;
    let uid = wallet_owner(&app, "momo_held").await;
    sqlx::query(
        "UPDATE talent_wallets
            SET residency_country = 'BJ', momo_phone = '+22997000000',
                momo_phone_verified = TRUE
          WHERE user_id = $1",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    // Captured, so it exists — but still inside its release window.
    ledger::capture_for_recipient(
        &app.db,
        "mtn",
        "mm_held",
        uid,
        bigdecimal::BigDecimal::from_str("5000").unwrap(),
        bigdecimal::BigDecimal::from_str("0").unwrap(),
        Currency::Xof,
        "bounty_slice",
        Uuid::new_v4(),
    )
    .await
    .unwrap();

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw",
            &json!({ "amount": "1000", "currency": "XOF" }),
        )
        .await;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "money the payer can still reclaim must not be withdrawable"
    );
    let body = resp.text().await.unwrap_or_default();
    assert!(
        body.contains("insufficient available balance"),
        "the answer should say why, got: {body}"
    );
}

#[tokio::test]
async fn withdrawing_without_a_destination_says_what_to_do() {
    let app = TestApp::spawn().await;
    let uid = wallet_owner(&app, "momo_none").await;

    // A wallet with a country and nothing to pay into.
    sqlx::query("UPDATE talent_wallets SET residency_country = 'BJ' WHERE user_id = $1")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw",
            &json!({ "amount": "1000", "currency": "XOF" }),
        )
        .await;
    let body = resp.text().await.unwrap_or_default();
    assert!(
        body.contains("destination") || body.contains("Mobile Money"),
        "the answer must name the remedy, got: {body}"
    );
}

// ─── The payout paths no longer lie ───────────────────────────────

#[tokio::test]
async fn the_mentorship_payout_columns_exist() {
    let app = TestApp::spawn().await;
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_name = 'mentorship_sessions'
            AND column_name IN ('payout_status', 'payout_error', 'payout_reference')
          ORDER BY column_name",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        cols,
        vec!["payout_error", "payout_reference", "payout_status"],
        "without these, a session whose transfer failed is indistinguishable \
         from one that paid"
    );
}

#[tokio::test]
async fn payout_status_is_constrained_to_known_values() {
    let app = TestApp::spawn().await;
    let mentor = wallet_owner(&app, "mentor_x").await;
    let mentee = wallet_owner(&app, "mentee_x").await;

    let bad = sqlx::query(
        "INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents, payout_status)
         VALUES ($1, $2, NOW(), 60, 5000, 4000, 1000, 'whatever')",
    )
    .bind(mentor)
    .bind(mentee)
    .execute(&app.db)
    .await;
    assert!(bad.is_err(), "an unknown payout_status must be refused");
}

#[tokio::test]
async fn a_completed_session_defaults_to_an_unpaid_payout() {
    let app = TestApp::spawn().await;
    let mentor = wallet_owner(&app, "mentor_y").await;
    let mentee = wallet_owner(&app, "mentee_y").await;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents)
         VALUES ($1, $2, NOW(), 60, 5000, 4000, 1000)
         RETURNING id",
    )
    .bind(mentor)
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let (status, released): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT payout_status, payout_released_at FROM mentorship_sessions WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    assert!(
        released.is_none(),
        "nothing was released, so nothing is stamped"
    );
}

// ─── SKI-292 — the share card ─────────────────────────────────────

#[tokio::test]
async fn an_unknown_hash_still_returns_a_card() {
    let app = TestApp::spawn().await;
    let hash = "f".repeat(64);

    let resp = app.get(&format!("/api/verify/{hash}/og.png")).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "a crawler that gets a 404 shows no preview at all, and the person \
         who shared the link never learns why"
    );
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
    let bytes = resp.bytes().await.unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[tokio::test]
async fn a_malformed_hash_returns_a_card_too() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/verify/not-a-hash/og.png").await;
    assert_eq!(resp.status().as_u16(), 200);
    let bytes = resp.bytes().await.unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[tokio::test]
async fn the_card_is_cacheable() {
    let app = TestApp::spawn().await;
    let hash = "e".repeat(64);
    let resp = app.get(&format!("/api/verify/{hash}/og.png")).await;
    let cache = resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        cache.contains("max-age"),
        "a validated attestation never changes, got: {cache}"
    );
}
