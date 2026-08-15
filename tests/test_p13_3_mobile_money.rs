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

    // A wallet that can be paid into. The operator is not asked for: it
    // belongs to the number, and routing decides the rest.
    sqlx::query(
        "INSERT INTO talent_wallets
            (user_id, residency_country, momo_phone, momo_phone_verified, momo_provider)
         VALUES ($1, 'CI', '+22507333333', TRUE, 'orange')
         ON CONFLICT (user_id) DO UPDATE SET
             residency_country = 'CI',
             momo_phone = '+22507333333',
             momo_phone_verified = TRUE,
             momo_provider = 'orange'",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed wallet");

    fund_xof(&app, user_id, "5000").await;

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw",
            &json!({ "amount": "2000", "currency": "XOF", "rail": "mobile_money" }),
        )
        .await;
    let status = resp.status();
    let jv: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "body: {jv}");
    assert_eq!(jv["data"]["currency"], "XOF");

    // The ledger is what a withdrawal moves. The wallet column it used to
    // decrement is gone from this path entirely.
    assert_eq!(available_xof(&app, user_id).await, BigDecimal::from(3000));

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
            "/api/users/me/wallet/withdraw",
            &json!({ "amount": "500", "currency": "XOF", "rail": "mobile_money" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "with no number on file there is nowhere to send it"
    );

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

    sqlx::query(
        "INSERT INTO talent_wallets
            (user_id, residency_country, momo_phone, momo_phone_verified)
         VALUES ($1, 'CI', '+22507444444', TRUE)
         ON CONFLICT (user_id) DO UPDATE SET
             residency_country = 'CI',
             momo_phone = '+22507444444',
             momo_phone_verified = TRUE",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed");

    // Funded past the limit on purpose: the balance is checked before the
    // limit is, so an unfunded account would be refused for the wrong
    // reason and this would pass without testing anything.
    fund_xof(&app, user_id, "500000").await;

    let resp = app
        .post(
            "/api/users/me/wallet/withdraw",
            &json!({ "amount": "150000", "currency": "XOF", "rail": "mobile_money" }),
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

/// Give someone withdrawable XOF, the way a real flow would.
///
/// Money lives in the ledger now, not in a wallet column, and only the
/// `available` state can leave. Seeding a balance directly would test a
/// path production no longer has.
async fn fund_xof(app: &TestApp, user: Uuid, amount: &str) {
    use skilluv_backend::services::ledger::{self, Currency};

    let subject = Uuid::new_v4();
    let amount = BigDecimal::from_str(amount).unwrap();
    ledger::capture_for_recipient(
        &app.db,
        "mtn",
        format!("seed:{subject}"),
        user,
        amount.clone(),
        BigDecimal::from(0),
        Currency::Xof,
        "bounty_slice",
        subject,
    )
    .await
    .expect("capture");
    ledger::release(
        &app.db,
        user,
        amount,
        Currency::Xof,
        "bounty_slice",
        subject,
    )
    .await
    .expect("release");
}

/// What is left that could be withdrawn.
async fn available_xof(app: &TestApp, user: Uuid) -> BigDecimal {
    use skilluv_backend::services::ledger::{self, Currency, State};

    ledger::user_balance(&app.db, user, State::Available, Currency::Xof)
        .await
        .expect("balance")
}
