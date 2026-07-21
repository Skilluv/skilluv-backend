//! Tests d'intégration P13.2 : Stripe Connect endpoints.
//!
//! On ne teste PAS l'intégration Stripe réelle (nécessite API keys en test mode).
//! On vérifie :
//! - Endpoints refusent 500 quand STRIPE_SECRET_KEY absente.
//! - Withdraw refuse si stripe_kyc_status != 'verified'.
//! - Webhook `account.updated` met à jour stripe_kyc_status quand la signature
//!   est valide + payouts_enabled=true.

mod common;

use common::TestApp;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use tokio::sync::Mutex;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

// Sérialise les tests P13.2 : ils mutent des env vars process-globales
// (STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET). Sans mutex, un test qui unset
// pendant qu'un autre est mid-request → race. On utilise `tokio::sync::Mutex`
// pour rester valide `Send` cross-`.await` (clippy::await_holding_lock).
static ENV_MUTEX: Mutex<()> = Mutex::const_new(());

fn sign_stripe_webhook(secret: &str, payload: &[u8]) -> String {
    let ts = chrono::Utc::now().timestamp();
    let signed = format!("{ts}.{}", String::from_utf8_lossy(payload));
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signed.as_bytes());
    let hex_sig = hex::encode(mac.finalize().into_bytes());
    format!("t={ts},v1={hex_sig}")
}

fn set_stripe_env() {
    // SAFETY: single-threaded env set before each test's HTTP call.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_dummy");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", "whsec_dummy_test_secret");
    }
}

// ═══════════════════════════════════════════════════════════════════
// stripe_onboard refuse si config absente
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn stripe_onboard_fails_without_config() {
    let _env_guard = ENV_MUTEX.lock().await;
    // Snapshot + clear + restore : évite de piéger les autres tests parallèles
    // qui partagent le même process env.
    let saved_key = std::env::var("STRIPE_SECRET_KEY").ok();
    let saved_wh = std::env::var("STRIPE_WEBHOOK_SECRET").ok();
    // SAFETY: env set + restore encadre le HTTP call.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", "");
    }
    let app = TestApp::spawn().await;
    app.register_user("u_no_stripe").await;
    app.login("u_no_stripe").await;

    let resp = app
        .post(
            "/api/users/me/wallet/stripe/onboard",
            &json!({ "country": "FR" }),
        )
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();

    // Restore avant les assertions pour libérer l'env pour les autres tests.
    unsafe {
        match saved_key {
            Some(v) => std::env::set_var("STRIPE_SECRET_KEY", v),
            None => std::env::remove_var("STRIPE_SECRET_KEY"),
        }
        match saved_wh {
            Some(v) => std::env::set_var("STRIPE_WEBHOOK_SECRET", v),
            None => std::env::remove_var("STRIPE_WEBHOOK_SECRET"),
        }
    }

    assert_eq!(status, 500, "Stripe non configuré → 500");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Stripe is not configured")
    );

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// stripe_withdraw refuse si KYC pas verified
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn stripe_withdraw_refuses_when_kyc_not_verified() {
    let _env_guard = ENV_MUTEX.lock().await;
    set_stripe_env();
    let app = TestApp::spawn().await;
    let body = app.register_user("u_kyc").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();
    app.login("u_kyc").await;

    // Crée wallet avec un stripe_account_id mais kyc = 'pending'
    sqlx::query(
        "INSERT INTO talent_wallets
            (user_id, stripe_account_id, stripe_kyc_status)
         VALUES ($1, 'acct_test_pending', 'pending')
         ON CONFLICT (user_id) DO UPDATE SET
             stripe_account_id = 'acct_test_pending',
             stripe_kyc_status = 'pending'",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed wallet");

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw/stripe",
            &json!({ "amount": "10.00", "currency": "EUR" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("KYC status is 'pending'")
    );

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// stripe_withdraw refuse si currency != EUR
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn stripe_withdraw_refuses_non_eur() {
    let _env_guard = ENV_MUTEX.lock().await;
    set_stripe_env();
    let app = TestApp::spawn().await;
    app.register_user("u_xof").await;
    app.login("u_xof").await;

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw/stripe",
            &json!({ "amount": "10.00", "currency": "XOF" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Webhook `account.updated` verified → stripe_kyc_status='verified'
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn webhook_account_updated_marks_kyc_verified() {
    let _env_guard = ENV_MUTEX.lock().await;
    set_stripe_env();
    let app = TestApp::spawn().await;
    let body = app.register_user("u_wh").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();

    sqlx::query(
        "INSERT INTO talent_wallets
            (user_id, stripe_account_id, stripe_kyc_status)
         VALUES ($1, 'acct_verify_me', 'pending')
         ON CONFLICT (user_id) DO UPDATE SET
             stripe_account_id = 'acct_verify_me',
             stripe_kyc_status = 'pending'",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed");

    let payload = json!({
        "type": "account.updated",
        "data": {
            "object": {
                "id": "acct_verify_me",
                "details_submitted": true,
                "charges_enabled": true,
                "payouts_enabled": true
            }
        }
    });
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let sig = sign_stripe_webhook("whsec_dummy_test_secret", &body_bytes);

    let resp = reqwest::Client::new()
        .post(format!("{}/api/webhooks/stripe-connect", app.addr))
        .header("Content-Type", "application/json")
        .header("Stripe-Signature", sig)
        .body(body_bytes)
        .send()
        .await
        .expect("webhook post");
    assert_eq!(resp.status(), 200);

    let status: String =
        sqlx::query_scalar("SELECT stripe_kyc_status FROM talent_wallets WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .expect("s");
    assert_eq!(status, "verified");

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Webhook non-account.updated event → ignored
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn webhook_ignores_unrelated_events() {
    let _env_guard = ENV_MUTEX.lock().await;
    set_stripe_env();
    let app = TestApp::spawn().await;

    let payload = json!({ "type": "invoice.paid", "data": { "object": {} } });
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let sig = sign_stripe_webhook("whsec_dummy_test_secret", &body_bytes);

    let resp = reqwest::Client::new()
        .post(format!("{}/api/webhooks/stripe-connect", app.addr))
        .header("Content-Type", "application/json")
        .header("Stripe-Signature", sig)
        .body(body_bytes)
        .send()
        .await
        .expect("webhook");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["data"]["ignored"], "invoice.paid");

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Webhook signature invalide → 401
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn webhook_rejects_invalid_signature() {
    let _env_guard = ENV_MUTEX.lock().await;
    set_stripe_env();
    let app = TestApp::spawn().await;

    let payload = json!({ "type": "account.updated", "data": { "object": {} } });
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    // Bad secret → signature invalide
    let bad_sig = sign_stripe_webhook("wrong_secret", &body_bytes);

    let resp = reqwest::Client::new()
        .post(format!("{}/api/webhooks/stripe-connect", app.addr))
        .header("Content-Type", "application/json")
        .header("Stripe-Signature", bad_sig)
        .body(body_bytes)
        .send()
        .await
        .expect("wh");
    assert_eq!(resp.status(), 401);

    drop(app);
}
