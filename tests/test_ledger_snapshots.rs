//! A running total is only safe if nothing can invalidate it.
//!
//! The snapshot replaces a `SUM` over every entry an account ever had. That
//! is normally a bug waiting to happen — a cached total whose source can
//! change underneath it — and it is safe here for two reasons, both of
//! which are enforced by the database rather than by convention: entries
//! are never updated, and never deleted. These tests hold both, and check
//! the arithmetic agrees.

mod common;

use bigdecimal::BigDecimal;
use common::TestApp;
use skilluv_backend::services::ledger::{self, Currency, State};
use std::str::FromStr;
use uuid::Uuid;

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

async fn earn(app: &TestApp, user: Uuid, amount: &str) {
    let subject = Uuid::new_v4();
    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        format!("test:{subject}"),
        user,
        BigDecimal::from_str(amount).unwrap(),
        BigDecimal::from(0),
        Currency::Eur,
        "bounty_slice",
        subject,
    )
    .await
    .unwrap();
}

/// Accounts where the snapshot and the entries disagree. Always empty.
async fn drifted(app: &TestApp) -> Vec<String> {
    sqlx::query_scalar("SELECT account_code FROM ledger_verify_balances()")
        .fetch_all(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_snapshot_agrees_with_the_entries_it_stands_for() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_agree").await;

    for amount in ["100", "250.50", "0.01"] {
        earn(&app, user, amount).await;
    }

    assert_eq!(
        ledger::user_balance(&app.db, user, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        BigDecimal::from_str("350.51").unwrap()
    );
    assert!(
        drifted(&app).await.is_empty(),
        "the running total and the entries must be the same arithmetic"
    );
}

#[tokio::test]
async fn a_balance_no_longer_sums_the_whole_history() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_history").await;

    for _ in 0..25 {
        earn(&app, user, "10").await;
    }

    // Reading a balance touches one row per account now, not one per entry.
    // The assertion that matters is the plan, so this checks the shape of
    // the answer and leaves the timing to the query planner.
    let entries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ledger_entries e
           JOIN ledger_accounts a ON a.id = e.account_id
          WHERE a.owner_user_id = $1",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(entries >= 25, "the history is there");

    let snapshots: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ledger_account_balances b
           JOIN ledger_accounts a ON a.id = b.account_id
          WHERE a.owner_user_id = $1",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        snapshots < entries,
        "and the read no longer walks it: {snapshots} rows for {entries} entries"
    );

    assert_eq!(
        ledger::user_balance(&app.db, user, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        BigDecimal::from(250)
    );
}

#[tokio::test]
async fn an_entry_cannot_be_deleted() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_delete").await;
    earn(&app, user, "100").await;

    // The invariant the snapshot rests on. Without this a DELETE would
    // silently invalidate every total it touched, and nothing would say so
    // until a nightly check ran.
    let deleted = sqlx::query("DELETE FROM ledger_entries")
        .execute(&app.db)
        .await;
    assert!(
        deleted.is_err(),
        "a mistake is corrected with a compensating entry, not by erasing one"
    );
}

#[tokio::test]
async fn an_entry_cannot_be_edited() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_edit").await;
    earn(&app, user, "100").await;

    let edited = sqlx::query("UPDATE ledger_entries SET amount = amount * 2")
        .execute(&app.db)
        .await;
    assert!(edited.is_err(), "entries are evidence, not working notes");
}

#[tokio::test]
async fn a_reversal_moves_the_snapshot_back() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_reverse").await;

    earn(&app, user, "500").await;
    ledger::release(
        &app.db,
        user,
        BigDecimal::from(500),
        Currency::Eur,
        "bounty_slice",
        Uuid::new_v4(),
    )
    .await
    .unwrap();

    let key = format!("withdraw:{user}");
    ledger::withdraw(
        &app.db,
        user,
        BigDecimal::from(500),
        Currency::Eur,
        "mtn",
        "ref-1".to_string(),
        key.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        ledger::user_balance(&app.db, user, State::Available, Currency::Eur)
            .await
            .unwrap(),
        BigDecimal::from(0)
    );

    // The correction is another entry, and the snapshot follows it the same
    // way it followed the original.
    ledger::reverse_withdrawal(
        &app.db,
        user,
        BigDecimal::from(500),
        Currency::Eur,
        "mtn",
        &key,
    )
    .await
    .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, user, State::Available, Currency::Eur)
            .await
            .unwrap(),
        BigDecimal::from(500)
    );
    assert!(drifted(&app).await.is_empty());
}

#[tokio::test]
async fn the_verification_catches_a_snapshot_that_has_been_tampered_with() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_tamper").await;
    earn(&app, user, "100").await;

    assert!(drifted(&app).await.is_empty(), "clean to start with");

    // Trusting the invariant is right. Trusting it without ever checking is
    // how a subtle trigger bug becomes a year of wrong balances — so the
    // check has to actually be able to fail.
    sqlx::query(
        "UPDATE ledger_account_balances
            SET balance = balance + 1
          WHERE account_id = (SELECT id FROM ledger_accounts
                               WHERE owner_user_id = $1 LIMIT 1)",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    assert!(
        !drifted(&app).await.is_empty(),
        "a snapshot that disagrees with its entries must be found"
    );
}

#[tokio::test]
async fn the_books_still_balance_across_every_account() {
    let app = TestApp::spawn().await;
    let user = person(&app, "snap_zero").await;

    for amount in ["100", "37.25"] {
        earn(&app, user, amount).await;
    }

    // The property the whole design exists for: money cannot appear or
    // vanish, only move. Every entry sums to zero per currency, so every
    // snapshot must too.
    let total: BigDecimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(b.balance), 0)
           FROM ledger_account_balances b
           JOIN ledger_accounts a ON a.id = b.account_id
          WHERE a.currency = 'EUR'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(total, BigDecimal::from(0), "the snapshots sum to zero too");
}
