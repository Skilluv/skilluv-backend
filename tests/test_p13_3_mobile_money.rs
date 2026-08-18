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
            idempotency_key: "test:e164",
        })
        .await;
    assert!(res.is_err(), "a number without a + is refused");
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
            idempotency_key: "test:currency",
        })
        .await;
    assert!(res.is_err(), "Orange refuses EUR");
}

// ═══════════════════════════════════════════════════════════════════
// With no credentials, the provider says so instead of inventing a receipt
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn orange_refuses_when_it_holds_no_credentials() {
    // SAFETY: single-threaded env removal.
    unsafe {
        std::env::remove_var("ORANGE_MONEY_API_KEY");
    }
    let amt = BigDecimal::from(1000);
    let err = OrangeMoneyProvider
        .initiate_payout(&PayoutParams {
            user_id: Uuid::new_v4(),
            phone: "+22507222222",
            currency: "XOF",
            amount: &amt,
            note: "test",
            idempotency_key: "test:unconfigured",
        })
        .await
        .expect_err("an unconfigured operator cannot accept a payout");

    // The old behaviour returned Pending with a synthetic reference, which
    // is the one failure mode a payout path must not have: it says money is
    // on its way when nothing was called.
    let message = err.to_string();
    assert!(
        message.contains("not configured"),
        "the error must say why, got: {message}"
    );
    assert!(
        message.contains("ORANGE_MONEY_API_KEY"),
        "and name what is missing, got: {message}"
    );
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
// End-to-end: a withdrawal on a deployment with no operator credentials
// ═══════════════════════════════════════════════════════════════════
//
// This test used to assert a 200 and a decremented balance, against a
// provider that called nothing. What it proved was that the stub returned
// what the stub returned. What matters instead is that the money stays where
// it is: the ledger is reversed, the payout is recorded as failed with the
// reason, and the caller is told rather than thanked.

#[tokio::test]
async fn momo_withdraw_leaves_the_money_alone_when_the_operator_is_unreachable() {
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
    assert_eq!(
        status, 500,
        "an operator that cannot be reached is a server-side failure, not a \
         user error — body: {jv}"
    );

    // The ledger is what a withdrawal moves, and nothing moved: the debit
    // posted before the call was reversed when the call failed.
    assert_eq!(
        available_xof(&app, user_id).await,
        BigDecimal::from(5000),
        "the balance must be exactly what it was before"
    );

    // And the attempt is not silent. A payout nobody recorded is one nobody
    // will ever chase.
    let (payout_status, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT status, failure_reason FROM payouts
          WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("the attempt left a row");

    assert_eq!(payout_status, "failed");
    assert!(
        reason.unwrap_or_default().contains("not configured"),
        "the reason names the missing configuration"
    );

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
