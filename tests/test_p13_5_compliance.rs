//! Tests d'intégration P13.5 : limites + statement CSV.

mod common;

use bigdecimal::BigDecimal;
use common::TestApp;
use serde_json::json;
use std::sync::Mutex;
use uuid::Uuid;

use skilluv_backend::services::talent_wallet::{self, Currency, LedgerEntry};

// Sérialise les tests qui mutent des env vars (WALLET_DAILY_LIMIT_*).
static ENV_MUTEX: Mutex<()> = Mutex::new(());

// ═══════════════════════════════════════════════════════════════════
// debits_within somme les debits sur la fenetre
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn debits_within_sums_recent_debits_only() {
    let app = TestApp::spawn().await;
    let body = app.register_user("u_dbt").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();

    // Seed wallet + 3 credits (assez pour couvrir les debits)
    talent_wallet::credit(
        &app.db,
        LedgerEntry {
            user_id,
            delta: &BigDecimal::from(1000),
            currency: Currency::Eur,
            reason: "seed",
            related_slice_id: None,
            related_provider_txn_id: None,
            notes: None,
        },
    )
    .await
    .expect("seed");

    // 2 débits successifs
    for amount in [50, 30] {
        talent_wallet::debit(
            &app.db,
            LedgerEntry {
                user_id,
                delta: &BigDecimal::from(amount),
                currency: Currency::Eur,
                reason: "withdraw_stripe",
                related_slice_id: None,
                related_provider_txn_id: None,
                notes: None,
            },
        )
        .await
        .expect("debit");
    }

    let total = talent_wallet::debits_within(&app.db, user_id, Currency::Eur, 24)
        .await
        .expect("d");
    assert_eq!(total, BigDecimal::from(80));

    // Sur XOF, aucun débit = 0
    let xof = talent_wallet::debits_within(&app.db, user_id, Currency::Xof, 24)
        .await
        .expect("x");
    assert_eq!(xof, BigDecimal::from(0));

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Enforce limit refuse au-dela de la limite journaliere (via momo withdraw)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn daily_limit_blocks_withdraw_above_threshold() {
    let _env_guard = ENV_MUTEX.lock().unwrap();
    // SAFETY: env muté sous garde du mutex.
    unsafe {
        std::env::set_var("WALLET_DAILY_LIMIT_XOF", "50000");
    }
    let app = TestApp::spawn().await;
    let body = app.register_user("u_daily").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();
    app.login("u_daily").await;

    // 100 000 XOF sur wallet, avec phone verified
    sqlx::query(
        "INSERT INTO talent_wallets (user_id, balance_xof, momo_phone, momo_phone_verified)
         VALUES ($1, 100000, '+22507555555', TRUE)
         ON CONFLICT (user_id) DO UPDATE SET
             balance_xof = 100000,
             momo_phone = '+22507555555',
             momo_phone_verified = TRUE",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed");

    // Withdraw 30 000 : OK (< 50000)
    let ok = app
        .post(
            "/api/users/me/wallet/withdraw/momo",
            &json!({ "provider": "orange", "amount": "30000", "currency": "XOF" }),
        )
        .await;
    assert_eq!(ok.status(), 200);

    // Withdraw 25 000 : refusé (30 000 + 25 000 = 55 000 > 50 000)
    let ko = app
        .post(
            "/api/users/me/wallet/withdraw/momo",
            &json!({ "provider": "orange", "amount": "25000", "currency": "XOF" }),
        )
        .await;
    assert_eq!(ko.status(), 400);
    let body: serde_json::Value = ko.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("daily withdraw limit exceeded"));

    // Clean up env
    unsafe {
        std::env::remove_var("WALLET_DAILY_LIMIT_XOF");
    }
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Statement CSV : headers + toutes les lignes du user
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn statement_csv_includes_all_transactions() {
    let app = TestApp::spawn().await;
    let body = app.register_user("u_csv").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();
    app.login("u_csv").await;

    for i in 1..=3 {
        let amt = BigDecimal::from(i * 10);
        talent_wallet::credit(
            &app.db,
            LedgerEntry {
                user_id,
                delta: &amt,
                currency: Currency::Eur,
                reason: "test_credit",
                related_slice_id: None,
                related_provider_txn_id: None,
                notes: None,
            },
        )
        .await
        .expect("c");
    }

    let resp = app.get("/api/users/me/wallet/statement.csv").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    let csv = resp.text().await.unwrap();
    // 1 header + 3 rows
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].starts_with("id,created_at,reason,delta,currency"));
    // Chaque ligne contient 'test_credit'
    for line in &lines[1..] {
        assert!(line.contains("test_credit"));
        assert!(line.contains("EUR"));
    }

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// CSV vide = juste le header
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn statement_csv_empty_for_new_user() {
    let app = TestApp::spawn().await;
    app.register_user("u_empty_csv").await;
    app.login("u_empty_csv").await;

    let resp = app.get("/api/users/me/wallet/statement.csv").await;
    assert_eq!(resp.status(), 200);
    let csv = resp.text().await.unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 1, "header seul si aucune transaction");
    assert!(lines[0].starts_with("id,created_at,reason,delta,currency"));

    drop(app);
}
