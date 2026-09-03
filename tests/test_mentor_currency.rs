//! A mentor is paid in the money they announced.
//!
//! Every mentor on the platform was priced in euros: `mentor_profiles` carried
//! `hourly_rate_eur_cents` and no currency, and `book_session` forced
//! `Currency::Eur`. Storing euros and converting at display time would leave a
//! Beninese mentor's income floating with the euro — they announce 15 000 XOF,
//! the euro moves, and they receive something else. What somebody is owed has
//! to be denominated in the money they are owed it in.

mod common;

use reqwest::StatusCode;
use serde_json::json;

use common::TestApp;

/// The ECB reference rate the listing converts through. Seeded per test
/// because `services::fx` fetches it from the network at runtime and a test
/// must not depend on that.
async fn seed_fx(app: &TestApp, eur_to_xof: &str) {
    sqlx::query(
        "INSERT INTO fx_rates (base_currency, quote_currency, rate)
         VALUES ('EUR', 'XOF', $1::NUMERIC)
         ON CONFLICT (base_currency, quote_currency) DO UPDATE SET rate = EXCLUDED.rate",
    )
    .bind(eur_to_xof)
    .execute(&app.db)
    .await
    .expect("seed fx rate");
}

async fn become_mentor(app: &TestApp, rate_cents: i64, currency: Option<&str>) -> StatusCode {
    let mut body = json!({
        "headline": "Backend, sans détour",
        "bio": "Dix ans de production, surtout des bases de données.",
        "expertise_areas": ["backend"],
        "languages_spoken": ["fr"],
        "hourly_rate_cents": rate_cents,
        "min_session_minutes": 30,
    });
    if let Some(c) = currency {
        body["currency"] = json!(c);
    }
    app.put("/api/mentors/me", &body).await.status()
}

/// A mentor announces a price in francs and it comes back in francs.
#[tokio::test]
async fn a_mentor_prices_in_their_own_currency() {
    let app = TestApp::spawn().await;
    app.register_user("mentorxof").await;
    app.login("mentorxof").await;

    assert_eq!(
        become_mentor(&app, 15_000, Some("XOF")).await,
        StatusCode::OK
    );

    let body: serde_json::Value = app.get("/api/mentors/me").await.json().await.unwrap();
    // 15 000 F CFA, not 150,00 of anything: the franc has no minor unit.
    assert_eq!(body["data"]["profile"]["hourly_rate_cents"], 15_000);
    assert_eq!(body["data"]["profile"]["currency"], "XOF");
}

/// Omitting the currency means euros, which is what every profile written
/// before the column existed meant.
#[tokio::test]
async fn an_omitted_currency_is_euros() {
    let app = TestApp::spawn().await;
    app.register_user("mentordefault").await;
    app.login("mentordefault").await;

    assert_eq!(become_mentor(&app, 4_000, None).await, StatusCode::OK);

    let body: serde_json::Value = app.get("/api/mentors/me").await.json().await.unwrap();
    assert_eq!(body["data"]["profile"]["currency"], "EUR");
}

/// A currency the ledger cannot settle is refused, and the message says which
/// two it can — rather than surfacing a constraint violation.
#[tokio::test]
async fn a_currency_the_ledger_cannot_settle_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("mentorusd").await;
    app.login("mentorusd").await;

    let resp = app
        .put(
            "/api/mentors/me",
            &json!({
                "headline": "h", "bio": "b",
                "expertise_areas": ["backend"], "languages_spoken": ["fr"],
                "hourly_rate_cents": 5000, "currency": "USD",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.to_string().contains("XOF"),
        "the refusal should name what is settleable: {body}"
    );
}

/// The listing converts to filter, and says both numbers.
///
/// A ceiling given in euros still has to see a mentor priced in francs — but
/// what is billed stays the mentor's own figure, and the euro equivalent is
/// marked as what it is.
#[tokio::test]
async fn a_euro_ceiling_still_sees_a_franc_mentor() {
    let app = TestApp::spawn().await;
    seed_fx(&app, "655.957").await;

    app.register_user("xofmentor").await;
    app.login("xofmentor").await;
    // 15 000 XOF ≈ 22,87 € at the fixed parity.
    become_mentor(&app, 15_000, Some("XOF")).await;

    app.register_user("seeker").await;
    app.login("seeker").await;

    // A 30 € ceiling: the franc mentor is under it once converted.
    let body: serde_json::Value = app
        .get("/api/mentors?max_rate_cents=3000")
        .await
        .json()
        .await
        .unwrap();
    let found = body["data"]["mentors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["username"] == "xofmentor")
        .expect("the franc mentor should be under a 30 € ceiling");

    assert_eq!(found["hourly_rate_cents"], 15_000);
    assert_eq!(found["currency"], "XOF");
    let approx: i64 = found["approx_eur_cents"]
        .as_str()
        .expect("an indicative euro figure")
        .parse()
        .unwrap();
    assert!(
        (2200..2400).contains(&approx),
        "15 000 XOF should read as about 23 €, got {approx}"
    );

    // And a ceiling below their converted price hides them.
    let body: serde_json::Value = app
        .get("/api/mentors?max_rate_cents=1000")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        !body["data"]["mentors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["username"] == "xofmentor"),
        "a 10 € ceiling is below 15 000 XOF"
    );
}

/// A mentor whose currency the feed has no rate for is shown, not hidden.
///
/// Dropping somebody from a list because our FX feed is stale serves nobody:
/// the reader can read the price themselves.
#[tokio::test]
async fn a_mentor_is_not_hidden_by_a_missing_rate() {
    let app = TestApp::spawn().await;
    sqlx::query("DELETE FROM fx_rates WHERE base_currency = 'EUR' AND quote_currency = 'XOF'")
        .execute(&app.db)
        .await
        .unwrap();

    app.register_user("norate").await;
    app.login("norate").await;
    become_mentor(&app, 15_000, Some("XOF")).await;

    app.register_user("norateseeker").await;
    app.login("norateseeker").await;
    let body: serde_json::Value = app
        .get("/api/mentors?max_rate_cents=1")
        .await
        .json()
        .await
        .unwrap();

    let found = body["data"]["mentors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["username"] == "norate")
        .expect("a mentor with no convertible rate is still listed");
    assert!(
        found["approx_eur_cents"].is_null(),
        "and the euro equivalent is honestly absent"
    );
}

/// Booking prices the session in the mentor's currency — and a booking that
/// cannot be routed holds nothing.
///
/// This test cannot complete an XOF checkout: taking francs needs FedaPay, and
/// neither CI nor a developer's machine holds those credentials. What it can
/// assert is the part that broke twice. The session used to be inserted before
/// the payment was routed, so a missing corridor left a `pending` row — and
/// `pending` blocks the overlap check, meaning a payment nobody ever took
/// silently held an hour of the mentor's calendar.
#[tokio::test]
async fn a_franc_booking_that_cannot_be_routed_holds_no_slot() {
    let app = TestApp::spawn().await;
    app.register_user("xofcoach").await;
    app.login("xofcoach").await;
    become_mentor(&app, 12_000, Some("XOF")).await;

    let mentor_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'xofcoach'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.register_user("mentee").await;
    app.login("mentee").await;
    let when = chrono::Utc::now() + chrono::Duration::days(2);
    let resp = app
        .post(
            "/api/mentorship/sessions",
            &json!({
                "mentor_user_id": mentor_id,
                "scheduled_at": when.to_rfc3339(),
                "duration_minutes": 60,
            }),
        )
        .await;
    let booked = resp.status().is_success();

    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT currency, price_total_cents FROM mentorship_sessions
          WHERE mentor_user_id = $1",
    )
    .bind(mentor_id)
    .fetch_optional(&app.db)
    .await
    .unwrap();

    match (booked, row) {
        // Wherever francs can actually be collected, the row says so.
        (true, Some((currency, total))) => {
            assert_eq!(
                currency.trim(),
                "XOF",
                "the session inherits the mentor's money, not a hardcoded EUR"
            );
            assert_eq!(total, 12_000, "one hour at 12 000 F CFA is 12 000 F CFA");
        }
        // And where they cannot, the refusal costs the mentor nothing.
        (false, None) => {}
        (true, None) => panic!("a successful booking left no session"),
        (false, Some(_)) => {
            panic!("a booking that was refused still holds the mentor's hour")
        }
    }
}
