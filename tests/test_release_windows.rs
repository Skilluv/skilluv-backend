//! Holding money and letting it go (migrations 0156/0157, `services::release`).
//!
//! The behaviour this proves is a change of product, not only of code:
//! completing a mentorship session no longer pays the mentor on the spot. It
//! records what is owed and holds it until the student confirms or the window
//! closes. A bounty, whose artefact is public and verifiable, is not held at
//! all.

mod common;
use bigdecimal::BigDecimal;
use common::TestApp;
use skilluv_backend::services::ledger::{self, Currency, State};
use skilluv_backend::services::release;
use std::str::FromStr;
use uuid::Uuid;

fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

async fn person(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L')
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// Capture an amount and register the hold, as a real flow would.
async fn capture_and_hold(
    app: &TestApp,
    beneficiary: Uuid,
    amount: &str,
    subject_type: &str,
    subject_id: Uuid,
) {
    let posted = ledger::capture_for_recipient(
        &app.db,
        "stripe",
        format!("test:{subject_id}"),
        beneficiary,
        dec(amount),
        dec("0"),
        Currency::Eur,
        subject_type,
        subject_id,
    )
    .await
    .unwrap();

    let window = release::window_for(&app.db, subject_type).await.unwrap();
    let mut tx = app.db.begin().await.unwrap();
    release::hold(
        &mut tx,
        release::Hold {
            ledger_transaction_id: posted.transaction_id(),
            beneficiary_id: beneficiary,
            subject_type,
            subject_id,
            amount: &dec(amount),
            currency: Currency::Eur,
            hold_hours: window.hold_hours,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

// ─── The windows themselves ───────────────────────────────────────

#[tokio::test]
async fn a_mentorship_session_is_held_for_a_week() {
    let app = TestApp::spawn().await;
    let window = release::window_for(&app.db, "mentorship_session")
        .await
        .unwrap();
    assert_eq!(window.hold_hours, 168);
    assert!(
        window.payer_can_release_early,
        "a student who is happy should be able to pay their mentor now"
    );
}

#[tokio::test]
async fn a_merged_bounty_is_not_held_at_all() {
    let app = TestApp::spawn().await;
    let window = release::window_for(&app.db, "bounty_slice").await.unwrap();
    assert_eq!(
        window.hold_hours, 0,
        "the contribution is merged upstream and public — there is nothing \
         to contest, so holding it would be a delay with no purpose"
    );
}

#[tokio::test]
async fn an_unknown_subject_type_is_refused_rather_than_defaulted() {
    let app = TestApp::spawn().await;
    let result = release::window_for(&app.db, "something_new").await;
    let msg = match result {
        Ok(_) => panic!("guessing a window would either pay too early or hold forever"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("release_windows"),
        "the error should say where to add it: {msg}"
    );
}

// ─── Holding ──────────────────────────────────────────────────────

#[tokio::test]
async fn held_money_is_not_available() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_held").await;
    let session = Uuid::new_v4();
    capture_and_hold(&app, mentor, "85", "mentorship_session", session).await;

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        dec("85")
    );
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("0")
    );
}

#[tokio::test]
async fn the_sweep_leaves_a_hold_that_is_not_due() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_notdue").await;
    capture_and_hold(&app, mentor, "85", "mentorship_session", Uuid::new_v4()).await;

    let report = release::sweep(&app.db).await.unwrap();
    assert_eq!(report.released, 0, "a week has not passed");
}

#[tokio::test]
async fn the_sweep_releases_a_hold_that_is_due() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_due").await;
    let session = Uuid::new_v4();
    capture_and_hold(&app, mentor, "85", "mentorship_session", session).await;

    // Reach back in time rather than wait a week.
    sqlx::query("UPDATE pending_releases SET release_at = NOW() - INTERVAL '1 hour'")
        .execute(&app.db)
        .await
        .unwrap();

    let report = release::sweep(&app.db).await.unwrap();
    assert_eq!(report.released, 1);
    assert!(report.failed.is_empty());
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("85")
    );
}

#[tokio::test]
async fn sweeping_twice_releases_once() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_twice").await;
    capture_and_hold(&app, mentor, "85", "mentorship_session", Uuid::new_v4()).await;
    sqlx::query("UPDATE pending_releases SET release_at = NOW() - INTERVAL '1 hour'")
        .execute(&app.db)
        .await
        .unwrap();

    release::sweep(&app.db).await.unwrap();
    let second = release::sweep(&app.db).await.unwrap();

    assert_eq!(
        second.released, 0,
        "the sweep runs on a schedule and overlaps"
    );
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("85"),
        "released once, not twice"
    );
}

// ─── Early release ────────────────────────────────────────────────

#[tokio::test]
async fn the_payer_can_release_early() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_early").await;
    let session = Uuid::new_v4();
    capture_and_hold(&app, mentor, "85", "mentorship_session", session).await;

    let released = release::release_early(&app.db, "mentorship_session", session)
        .await
        .unwrap();
    assert!(released);
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("85"),
        "both parties agree, so the mentor is paid today"
    );
}

#[tokio::test]
async fn early_release_is_refused_where_the_window_forbids_it() {
    let app = TestApp::spawn().await;
    let buyer = person(&app, "rel_cert").await;
    let purchase = Uuid::new_v4();
    capture_and_hold(&app, buyer, "50", "certification_purchase", purchase).await;

    let result = release::release_early(&app.db, "certification_purchase", purchase).await;
    assert!(
        result.is_err(),
        "certification_purchase sets payer_can_release_early = FALSE"
    );
}

// ─── Disputes ─────────────────────────────────────────────────────

#[tokio::test]
async fn a_dispute_stops_the_sweep_from_releasing() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_disputed").await;
    let session = Uuid::new_v4();
    capture_and_hold(&app, mentor, "85", "mentorship_session", session).await;

    release::dispute(&app.db, "mentorship_session", session)
        .await
        .unwrap();
    sqlx::query("UPDATE pending_releases SET release_at = NOW() - INTERVAL '1 hour'")
        .execute(&app.db)
        .await
        .unwrap();

    let report = release::sweep(&app.db).await.unwrap();
    assert_eq!(
        report.released, 0,
        "money being argued over must not be handed over by a timer"
    );
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Disputed, Currency::Eur)
            .await
            .unwrap(),
        dec("85")
    );
}

#[tokio::test]
async fn released_money_cannot_be_disputed_afterwards() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_late").await;
    let session = Uuid::new_v4();
    capture_and_hold(&app, mentor, "85", "mentorship_session", session).await;
    release::release_early(&app.db, "mentorship_session", session)
        .await
        .unwrap();

    let result = release::dispute(&app.db, "mentorship_session", session).await;
    assert!(
        result.is_err(),
        "once released the money is the recipient's — clawing it back is a \
         refund, and a harder problem"
    );
}

// ─── The overdue queue ────────────────────────────────────────────

#[tokio::test]
async fn overdue_lists_what_the_sweep_has_not_managed_to_release() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "rel_overdue").await;
    capture_and_hold(&app, mentor, "85", "mentorship_session", Uuid::new_v4()).await;
    sqlx::query("UPDATE pending_releases SET release_at = NOW() - INTERVAL '2 hours'")
        .execute(&app.db)
        .await
        .unwrap();

    let late = release::overdue(&app.db).await.unwrap();
    assert_eq!(late.len(), 1, "someone is owed money and cannot reach it");

    release::sweep(&app.db).await.unwrap();
    assert!(
        release::overdue(&app.db).await.unwrap().is_empty(),
        "empty is the only acceptable state"
    );
}

// ─── Credit conversion ────────────────────────────────────────────

#[tokio::test]
async fn credit_conversion_never_silently_yields_nothing() {
    use skilluv_backend::services::credit_value;

    // The regression: an unset environment variable parsed to 0.0, the
    // payout was skipped, and the slice was stamped paid anyway.
    for currency in [Currency::Eur, Currency::Xof] {
        let out = credit_value::to_currency(&dec("10"), currency);
        assert!(
            out > dec("0"),
            "10 credits produced nothing in {}",
            currency.as_str()
        );
    }
}
