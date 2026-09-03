//! A price is never labelled in a currency it was not converted to.
//!
//! `/api/pricing` quotes the credit packs in the visitor's currency. When
//! `fx_rates` had no row for that currency the conversion failed, the handler
//! fell back to the euro *amount*, and the response went on saying ZAR. A
//! South African visitor was quoted "39" and told it was rands — about two
//! euros, for a pack that costs thirty-nine. Observed in production on
//! 2026-09-03 for ZAR, NGN and KES.
//!
//! The ECB publishes no rate for NGN, KES, UGX, EGP or GHS and never will, so
//! this is not a transient gap to wait out: those countries resolve to a
//! currency the platform cannot quote, and the honest answer is euros.

mod common;

use common::TestApp;

/// Seeded by migration 0037: the CFA franc is pegged to the euro by treaty.
const XOF_PEG: f64 = 655.957;

async fn pricing(app: &TestApp, country: &str) -> serde_json::Value {
    app.get(&format!("/api/pricing?country={country}"))
        .await
        .json()
        .await
        .unwrap()
}

/// A currency we hold no rate for is quoted in euros, and says so.
#[tokio::test]
async fn an_unconvertible_currency_is_quoted_in_euros() {
    let app = TestApp::spawn().await;
    sqlx::query("DELETE FROM fx_rates WHERE quote_currency IN ('ZAR', 'NGN')")
        .execute(&app.db)
        .await
        .unwrap();

    for country in ["ZA", "NG"] {
        let body = pricing(&app, country).await;
        assert_eq!(
            body["data"]["currency"], "EUR",
            "{country} has no rate, so the quote is in euros and says euros"
        );
        let pack = &body["data"]["packs"][0];
        assert_eq!(
            pack["price"], pack["price_eur"],
            "the number is the euro number; the label now agrees with it"
        );
        assert_eq!(
            pack["fx_rate_applied"], 1.0,
            "euros into euros, at one: the figure describes what was actually              done rather than pointing at a rate we never had"
        );
    }
}

/// A currency we do hold a rate for is converted, and labelled as itself.
#[tokio::test]
async fn a_pegged_currency_is_converted_and_labelled() {
    let app = TestApp::spawn().await;

    let body = pricing(&app, "BJ").await;
    assert_eq!(body["data"]["currency"], "XOF");

    let pack = &body["data"]["packs"][0];
    let price = pack["price"].as_f64().expect("a converted price");
    let eur = pack["price_eur"].as_f64().expect("the euro reference");
    let rate = pack["fx_rate_applied"]
        .as_f64()
        .expect("the rate that was applied");

    // The peg plus the margin the service adds on top of it.
    assert!(
        rate >= XOF_PEG,
        "the applied rate carries a margin above the peg: {rate}"
    );
    assert!(
        (price - eur * rate).abs() < price * 0.01,
        "{eur} EUR at {rate} should be about {price} XOF"
    );
    assert!(
        price > eur,
        "a franc price is a much larger number than a euro one, \
         which is the whole reason the label matters"
    );
}
