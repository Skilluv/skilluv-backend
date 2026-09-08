//! SKI-369 - an address given to the footer form is kept, or let go of.
//!
//! The form used to wait 400ms so the button looked busy, clear the field so
//! it looked accepted, and discard the address. These tests pin the three
//! properties that make the replacement honest: the address is really stored,
//! nothing is mailed until the person holding it clicks, and the endpoint
//! cannot be used to find out who is on the list.

use crate::common::TestApp;
use serde_json::json;

async fn subscribe(app: &TestApp, email: &str) -> reqwest::Response {
    app.post(
        "/api/newsletter/subscriptions",
        &json!({ "email": email, "locale": "fr", "source": "footer" }),
    )
    .await
}

/// A new address is stored pending, never confirmed by the act of typing it.
#[tokio::test]
async fn an_address_starts_pending_and_is_not_mailable() {
    let app = TestApp::spawn().await;

    let resp = subscribe(&app, "Someone@Example.COM").await;
    assert!(resp.status().is_success(), "subscribing should be accepted");

    let (email, status, confirmed): (String, String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT email, status, confirmed_at FROM newsletter_subscriptions
              WHERE email = 'someone@example.com'",
        )
        .fetch_one(&app.db)
        .await
        .expect("the address is stored, which is the whole point");

    assert_eq!(email, "someone@example.com", "stored lowercased");
    assert_eq!(status, "pending");
    assert!(
        confirmed.is_none(),
        "typing an address is not consent to be mailed at it"
    );
}

/// The answer never reveals whether the address was already known.
///
/// A different body or status for a known address turns a public endpoint
/// into an oracle for "is this person on the list", on exactly the data a
/// newsletter holds.
#[tokio::test]
async fn the_answer_is_the_same_for_a_new_and_a_confirmed_address() {
    let app = TestApp::spawn().await;

    let first = subscribe(&app, "known@example.com").await;
    let first_status = first.status();
    let first_body = first.text().await.unwrap();

    // Confirm it, so the second call hits a genuinely different row state.
    sqlx::query(
        "UPDATE newsletter_subscriptions SET status = 'confirmed', confirmed_at = NOW(),
                confirm_token = NULL, confirm_token_expires_at = NULL
          WHERE email = 'known@example.com'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let second = subscribe(&app, "known@example.com").await;
    let second_status = second.status();
    let second_body = second.text().await.unwrap();

    assert_eq!(first_status, second_status, "the status must not differ");

    // The envelope carries a fresh request_id and timestamp per call, so the
    // comparison is on the part that describes the outcome.
    let msg = |raw: &str| -> String {
        serde_json::from_str::<serde_json::Value>(raw).unwrap()["data"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        msg(&first_body),
        msg(&second_body),
        "a caller must not be able to tell a new address from a confirmed one"
    );
}

/// Re-subscribing a confirmed address does not disturb it.
#[tokio::test]
async fn a_resubscribe_cannot_reset_a_confirmed_row() {
    let app = TestApp::spawn().await;
    subscribe(&app, "settled@example.com").await;
    sqlx::query(
        "UPDATE newsletter_subscriptions SET status = 'confirmed', confirmed_at = NOW(),
                confirm_token = NULL WHERE email = 'settled@example.com'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    subscribe(&app, "settled@example.com").await;

    let (status, token): (String, Option<String>) = sqlx::query_as(
        "SELECT status, confirm_token FROM newsletter_subscriptions
          WHERE email = 'settled@example.com'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(status, "confirmed", "still confirmed");
    assert!(
        token.is_none(),
        "no new confirmation token was minted for somebody already subscribed"
    );
}

/// The confirmation link is what subscribes, and it is spent on use.
#[tokio::test]
async fn confirming_subscribes_and_the_link_stops_working() {
    let app = TestApp::spawn().await;
    subscribe(&app, "clicker@example.com").await;

    let token: String = sqlx::query_scalar(
        "SELECT confirm_token FROM newsletter_subscriptions WHERE email = 'clicker@example.com'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let resp = app.get(&format!("/api/newsletter/confirm/{token}")).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["confirmed"], true);
    assert!(
        body["data"]["unsubscribe_token"].is_string(),
        "the way out is handed over at once, not only in the first mail"
    );

    let (status,): (String,) = sqlx::query_as(
        "SELECT status FROM newsletter_subscriptions WHERE email = 'clicker@example.com'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "confirmed");

    // A confirmation link that keeps working is a link that keeps sitting in
    // a mailbox somebody else may one day read.
    let again = app.get(&format!("/api/newsletter/confirm/{token}")).await;
    assert_eq!(
        again.status(),
        reqwest::StatusCode::NOT_FOUND,
        "the token is spent"
    );
}

/// Unsubscribing needs no account, works twice, and keeps working.
#[tokio::test]
async fn unsubscribing_needs_no_account_and_is_idempotent() {
    let app = TestApp::spawn().await;
    subscribe(&app, "leaver@example.com").await;

    let (confirm, unsub): (String, String) = sqlx::query_as(
        "SELECT confirm_token, unsubscribe_token FROM newsletter_subscriptions
          WHERE email = 'leaver@example.com'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    app.get(&format!("/api/newsletter/confirm/{confirm}")).await;

    // No session, no cookie, no account.
    for attempt in 1..=2 {
        let resp = app
            .get(&format!("/api/newsletter/unsubscribe/{unsub}"))
            .await;
        assert!(
            resp.status().is_success(),
            "attempt {attempt} should succeed: clicking unsubscribe twice is not an error"
        );
    }

    let (status,): (String,) = sqlx::query_as(
        "SELECT status FROM newsletter_subscriptions WHERE email = 'leaver@example.com'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "unsubscribed");
}

/// An address that is not one is refused, rather than stored and never mailed.
#[tokio::test]
async fn a_malformed_address_is_refused() {
    let app = TestApp::spawn().await;
    for bad in ["", "nope", "@nodomain.com", "no-tld@localhost"] {
        let resp = subscribe(&app, bad).await;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{bad:?} should be refused"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM newsletter_subscriptions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0, "nothing malformed was stored");
}

/// Consent is a record, not a boolean.
#[tokio::test]
async fn the_wording_shown_beside_the_field_is_kept() {
    let app = TestApp::spawn().await;
    let wording = "J'accepte de recevoir la newsletter Skilluv.";

    app.post(
        "/api/newsletter/subscriptions",
        &json!({
            "email": "consenting@example.com",
            "locale": "fr",
            "source": "footer",
            "consent_text": wording,
        }),
    )
    .await;

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT consent_text FROM newsletter_subscriptions WHERE email = 'consenting@example.com'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(
        stored.as_deref(),
        Some(wording),
        "if the wording changes, a consent gathered under the old one has to \
         stay readable as that consent"
    );
}

/// Any opt-out wins, which is the rule the sending code must obey.
///
/// An account holder who turns marketing off is not mailed, even though the
/// newsletter row still says confirmed. Without this, somebody who clicks
/// unsubscribe in one place keeps being mailed from the other.
#[tokio::test]
async fn an_account_that_refuses_marketing_is_not_mailable() {
    let app = TestApp::spawn().await;

    app.register_user("newsreader").await;
    let (user_id, user_email): (uuid::Uuid, String) =
        sqlx::query_as("SELECT id, email FROM users WHERE username = 'newsreader'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // The same person subscribes through the footer, anonymously.
    subscribe(&app, &user_email).await;
    sqlx::query(
        "UPDATE newsletter_subscriptions SET status = 'confirmed', confirmed_at = NOW(),
                confirm_token = NULL WHERE email = lower($1)",
    )
    .bind(&user_email)
    .execute(&app.db)
    .await
    .unwrap();

    let mailable = skilluv_backend::routes::newsletter::mailable_addresses(&app.db)
        .await
        .unwrap();
    assert!(
        mailable
            .iter()
            .any(|(e, _)| e == &user_email.to_lowercase()),
        "confirmed and nothing refusing: mailable"
    );

    // Now the account says no, through the preference system that actually
    // exists. `user_email_preferences` was dropped by migration 0164 as a
    // second system that disagreed with the catalogue; the catalogue won.
    sqlx::query(
        "INSERT INTO notification_preferences (user_id, kind, channel, enabled)
         VALUES ($1, 'newsletter.issue', 'email', FALSE)
         ON CONFLICT (user_id, kind, channel) DO UPDATE SET enabled = FALSE",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .unwrap();

    let mailable = skilluv_backend::routes::newsletter::mailable_addresses(&app.db)
        .await
        .unwrap();
    assert!(
        !mailable
            .iter()
            .any(|(e, _)| e == &user_email.to_lowercase()),
        "the account refused marketing, so the address is not mailable even \
         though its newsletter row is still confirmed"
    );
}

/// An unconfirmed address is never in the send list.
#[tokio::test]
async fn a_pending_address_is_never_mailable() {
    let app = TestApp::spawn().await;
    subscribe(&app, "waiting@example.com").await;

    let mailable = skilluv_backend::routes::newsletter::mailable_addresses(&app.db)
        .await
        .unwrap();
    assert!(
        !mailable.iter().any(|(e, _)| e == "waiting@example.com"),
        "double opt-in means pending is not a licence to send"
    );
}

/// The coarse `marketing` toggle cannot switch the newsletter off.
///
/// `GET/PUT /users/me/email-preferences` is a narrower view over the
/// catalogue: its `marketing` boolean reads true when any `lifecycle` kind has
/// email enabled, and writing it false writes false across every one of them.
/// A newsletter filed under `lifecycle` would be switched off by somebody
/// using that toggle to stop the onboarding drip, while their subscription row
/// still read `confirmed` and nothing showed them why the mail stopped.
///
/// So it has its own category, and this is what says so. If anybody moves it
/// back, the newsletter starts answering to a control that was never about it.
#[tokio::test]
async fn the_newsletter_is_not_a_lifecycle_kind() {
    let app = TestApp::spawn().await;

    let category: String = sqlx::query_scalar(
        "SELECT category FROM notification_kinds WHERE kind = 'newsletter.issue'",
    )
    .fetch_one(&app.db)
    .await
    .expect("the newsletter is registered in the catalogue");

    assert_eq!(
        category, "newsletter",
        "under `lifecycle`, PUT /users/me/email-preferences with marketing=false \
         would silently unsubscribe somebody who double opted in"
    );
}

/// Turning marketing off in the settings screen leaves the newsletter alone.
///
/// The property above, exercised through the endpoint rather than asserted
/// about a column, because that is where it would actually break.
#[tokio::test]
async fn refusing_marketing_does_not_unsubscribe_the_newsletter() {
    let app = TestApp::spawn().await;
    app.register_user("bothways").await;
    app.login("bothways").await;

    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE username = 'bothways'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    subscribe(&app, &email).await;
    sqlx::query(
        "UPDATE newsletter_subscriptions SET status = 'confirmed', confirmed_at = NOW(),
                confirm_token = NULL WHERE email = lower($1)",
    )
    .bind(&email)
    .execute(&app.db)
    .await
    .unwrap();

    // The person stops the onboarding drip. They said nothing about the
    // newsletter.
    let resp = app
        .put(
            "/api/users/me/email-preferences",
            &json!({ "digest_weekly": true, "streak_reminder": true, "marketing": false }),
        )
        .await;
    assert!(
        resp.status().is_success(),
        "the settings screen still works: {}",
        resp.text().await.unwrap()
    );

    let mailable = skilluv_backend::routes::newsletter::mailable_addresses(&app.db)
        .await
        .unwrap();
    assert!(
        mailable.iter().any(|(e, _)| e == &email.to_lowercase()),
        "refusing marketing must not revoke a newsletter consent that was \
         given separately and confirmed by clicking a link"
    );
}

/// The two shapes a contract fuzzer found, refused as 400 and not as 500.
///
/// `0@com` is what a generator produces from `format: email`, and the schema
/// used to say no more than that, so schemathesis read the endpoint as
/// rejecting valid data. `a@b.c` is worse: the handler accepted it and the
/// column's CHECK did not, which is a 500 handed to somebody who typed an
/// address slightly wrong. Both belong to the same mistake, a validator
/// looser than the thing it validates for.
#[tokio::test]
async fn a_domain_without_a_real_tld_is_refused_before_the_insert() {
    let app = TestApp::spawn().await;

    for bad in ["0@com", "a@b.c"] {
        let resp = subscribe(&app, bad).await;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{bad} must be refused by the handler, not by the column"
        );
    }

    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM newsletter_subscriptions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(stored, 0, "a refused address must leave nothing behind");
}

/// Every bounded column is defended before the statement, not by it.
///
/// A utoipa `max_length` documents and does not reject, so `source` reached
/// its `VARCHAR(40)` column at 41 characters and came back as a 500 carrying
/// a Postgres message. `consent_ip` is the same shape of hole reached from
/// the other side: it is `VARCHAR(45)` filled from `X-Forwarded-For`, which
/// the caller sets, and no contract fuzzer sends headers so nothing would
/// have found it. The IP is truncated rather than refused, because somebody
/// subscribing should not be turned away over what a proxy wrote.
#[tokio::test]
async fn an_oversized_field_is_a_bad_request_and_an_oversized_ip_is_kept_short() {
    let app = TestApp::spawn().await;

    let resp = app
        .post(
            "/api/newsletter/subscriptions",
            &json!({
                "email": "bounded@example.com",
                "locale": "fr",
                "source": "0".repeat(41),
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "a source past its column must be refused here, not by the INSERT"
    );

    let resp = app
        .post_with_header(
            "/api/newsletter/subscriptions",
            &json!({
                "email": "forwarded@example.com",
                "locale": "fr",
                "source": "footer",
            }),
            "X-Forwarded-For",
            &"9".repeat(200),
        )
        .await;
    assert!(
        resp.status().is_success(),
        "a forged forwarding header must not cost the subscriber their subscription: {}",
        resp.status()
    );

    let ip: Option<String> =
        sqlx::query_scalar("SELECT consent_ip FROM newsletter_subscriptions WHERE email = $1")
            .bind("forwarded@example.com")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        ip.map(|v| v.chars().count()).unwrap_or(0) <= 45,
        "the stored IP must fit the column it is stored in"
    );
}
