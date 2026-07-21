//! Tests d'intégration P13.3 : Mobile Money endpoints + trait.

mod common;

use std::str::FromStr;

use bigdecimal::BigDecimal;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

use skilluv_backend::services::mobile_money::{
    self, MobileMoneyProvider, OrangeMoneyProvider, PayoutParams, ProviderName,
};

// ═══════════════════════════════════════════════════════════════════
// ProviderName::from_str case-insensitive + rejette inconnu
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn provider_name_from_str_accepts_variants() {
    assert_eq!(
        ProviderName::from_str("orange").unwrap(),
        ProviderName::Orange
    );
    assert_eq!(
        ProviderName::from_str("ORANGE").unwrap(),
        ProviderName::Orange
    );
    assert_eq!(
        ProviderName::from_str("orange_money").unwrap(),
        ProviderName::Orange
    );
    assert_eq!(ProviderName::from_str("Mtn").unwrap(), ProviderName::Mtn);
    assert_eq!(ProviderName::from_str("wave").unwrap(), ProviderName::Wave);
    assert!(ProviderName::from_str("paypal").is_err());
}

// ═══════════════════════════════════════════════════════════════════
// Provider initiate_payout : validation phone E.164
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn orange_rejects_non_e164_phone() {
    let amt = BigDecimal::from(500);
    let res = OrangeMoneyProvider
        .initiate_payout(&PayoutParams {
            user_id: Uuid::new_v4(),
            phone: "0507000000",
            currency: "XOF",
            amount: &amt,
            note: "test",
        })
        .await;
    assert!(res.is_err(), "phone sans + refuse");
}

// ═══════════════════════════════════════════════════════════════════
// Orange : XOF only en P13
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn orange_rejects_non_xof_currency() {
    let amt = BigDecimal::from(500);
    let res = OrangeMoneyProvider
        .initiate_payout(&PayoutParams {
            user_id: Uuid::new_v4(),
            phone: "+22507111111",
            currency: "EUR",
            amount: &amt,
            note: "test",
        })
        .await;
    assert!(res.is_err(), "EUR refuse par Orange");
}

// ═══════════════════════════════════════════════════════════════════
// Sans credentials env, Orange retourne Pending + message dev
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn orange_returns_pending_stub_in_dev_mode() {
    // SAFETY: single-threaded env removal.
    unsafe {
        std::env::remove_var("ORANGE_MONEY_API_KEY");
    }
    let amt = BigDecimal::from(1000);
    let res = OrangeMoneyProvider
        .initiate_payout(&PayoutParams {
            user_id: Uuid::new_v4(),
            phone: "+22507222222",
            currency: "XOF",
            amount: &amt,
            note: "test",
        })
        .await
        .expect("dev stub OK");
    assert_eq!(res.provider, ProviderName::Orange);
    assert_eq!(res.status, mobile_money::PayoutStatus::Pending);
    assert!(res.provider_txn_id.starts_with("orange:dev:"));
    assert!(res.message.unwrap().contains("dev mode"));
}

// ═══════════════════════════════════════════════════════════════════
// Factory renvoie le bon provider
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn factory_returns_correct_provider() {
    assert_eq!(
        mobile_money::get_provider(ProviderName::Orange).name(),
        ProviderName::Orange
    );
    assert_eq!(
        mobile_money::get_provider(ProviderName::Mtn).name(),
        ProviderName::Mtn
    );
    assert_eq!(
        mobile_money::get_provider(ProviderName::Wave).name(),
        ProviderName::Wave
    );
}

// ═══════════════════════════════════════════════════════════════════
// End-to-end : register phone → withdraw XOF réussi
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn momo_withdraw_full_flow_from_wallet() {
    let app = TestApp::spawn().await;
    let body = app.register_user("u_momo").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();
    app.login("u_momo").await;

    // Seed 5000 XOF sur le wallet
    sqlx::query(
        "INSERT INTO talent_wallets (user_id, balance_xof, momo_phone, momo_phone_verified)
         VALUES ($1, 5000, '+22507333333', TRUE)
         ON CONFLICT (user_id) DO UPDATE SET
             balance_xof = 5000,
             momo_phone = '+22507333333',
             momo_phone_verified = TRUE",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed wallet");

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw/momo",
            &json!({
                "provider": "orange",
                "amount": "2000",
                "currency": "XOF"
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let jv: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(jv["data"]["provider"], "orange");
    assert!(
        jv["data"]["provider_txn_id"]
            .as_str()
            .unwrap()
            .starts_with("orange:dev:")
    );

    // Balance décrémentée
    let bal: BigDecimal =
        sqlx::query_scalar("SELECT balance_xof FROM talent_wallets WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .expect("bal");
    assert_eq!(bal, BigDecimal::from(3000));

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Withdraw refuse si téléphone pas enregistré
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn momo_withdraw_refuses_without_verified_phone() {
    let app = TestApp::spawn().await;
    app.register_user("u_no_phone").await;
    app.login("u_no_phone").await;

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw/momo",
            &json!({ "provider": "orange", "amount": "500", "currency": "XOF" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// KYC lite limit : refuse au-delà de 100 000 XOF
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn momo_withdraw_refuses_above_kyc_lite_limit() {
    let app = TestApp::spawn().await;
    let body = app.register_user("u_big").await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();
    app.login("u_big").await;

    // Seed 500 000 XOF
    sqlx::query(
        "INSERT INTO talent_wallets (user_id, balance_xof, momo_phone, momo_phone_verified)
         VALUES ($1, 500000, '+22507444444', TRUE)
         ON CONFLICT (user_id) DO UPDATE SET
             balance_xof = 500000,
             momo_phone = '+22507444444',
             momo_phone_verified = TRUE",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed");

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw/momo",
            &json!({ "provider": "orange", "amount": "150000", "currency": "XOF" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
    let jv: serde_json::Value = resp.json().await.unwrap();
    assert!(
        jv["error"]["message"]
            .as_str()
            .unwrap()
            .contains("KYC-lite limit")
    );

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Register phone : E.164 required
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn register_phone_requires_e164() {
    let app = TestApp::spawn().await;
    app.register_user("u_bad_phone").await;
    app.login("u_bad_phone").await;

    let resp = app
        .post(
            "/api/users/me/wallet/momo/phone",
            &json!({ "phone": "0757000000", "verified": true }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    drop(app);
}
