//! The wallet, once it stopped keeping its own books (migration 0158).
//!
//! Replaces the P13.1 / P13.4 / P13.5 suites, which asserted the behaviour of
//! `talent_transactions` and the two balance columns. Those are gone; what
//! they were protecting is not, and is re-covered here: balances, withdrawal
//! limits, and the compliance export.

mod common;
use bigdecimal::BigDecimal;
use common::TestApp;
use skilluv_backend::services::ledger::{self, Currency, State};
use skilluv_backend::services::talent_wallet;
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

/// Give someone withdrawable money, the way a real flow would.
async fn fund(app: &TestApp, user: Uuid, amount: &str, currency: Currency) {
    let subject = Uuid::new_v4();
    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        format!("fund:{subject}"),
        user,
        dec(amount),
        dec("0"),
        currency,
        "bounty_slice",
        subject,
    )
    .await
    .unwrap();
    ledger::release(
        &app.db,
        user,
        dec(amount),
        currency,
        "bounty_slice",
        subject,
    )
    .await
    .unwrap();
}

// ─── The old ledger is gone ───────────────────────────────────────

#[tokio::test]
async fn the_legacy_ledger_no_longer_exists() {
    let app = TestApp::spawn().await;

    let table: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables
                        WHERE table_name = 'talent_transactions')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!table, "two ledgers means two answers to the same question");

    let columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns
          WHERE table_name = 'talent_wallets'
            AND column_name IN ('balance_eur', 'balance_xof')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(columns, 0, "a stale balance is worse than no balance");
}

#[tokio::test]
async fn the_wallet_still_holds_the_payout_destinations() {
    // The row was not dropped: where to send money is not a balance.
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_dest").await;

    let wallet = talent_wallet::get_or_init_wallet(&app.db, user)
        .await
        .unwrap();
    assert_eq!(wallet.user_id, user);
    assert!(wallet.momo_phone.is_none());
    assert_eq!(wallet.stripe_kyc_status, "not_started");
}

// ─── Balances come from the ledger ────────────────────────────────

#[tokio::test]
async fn balances_are_derived_and_split_by_state() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_balances").await;
    let subject = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_bal",
        user,
        dec("100"),
        dec("0"),
        Currency::Eur,
        "mentorship_session",
        subject,
    )
    .await
    .unwrap();

    let held = talent_wallet::balances(&app.db, user).await.unwrap();
    assert_eq!(held.pending_eur, dec("100"));
    assert_eq!(held.available_eur, dec("0"));

    ledger::release(
        &app.db,
        user,
        dec("100"),
        Currency::Eur,
        "mentorship_session",
        subject,
    )
    .await
    .unwrap();

    let after = talent_wallet::balances(&app.db, user).await.unwrap();
    assert_eq!(after.pending_eur, dec("0"));
    assert_eq!(after.available_eur, dec("100"));
}

#[tokio::test]
async fn a_new_wallet_reports_zero_everywhere() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_zero").await;

    let balances = talent_wallet::balances(&app.db, user).await.unwrap();
    assert_eq!(balances.available_eur, dec("0"));
    assert_eq!(balances.available_xof, dec("0"));
    assert_eq!(balances.pending_eur, dec("0"));
}

// ─── Withdrawal limits ────────────────────────────────────────────

#[tokio::test]
async fn withdrawals_count_towards_the_rolling_window() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_limit").await;
    fund(&app, user, "500", Currency::Eur).await;

    ledger::withdraw(
        &app.db,
        user,
        dec("200"),
        Currency::Eur,
        "stripe",
        "tr_1",
        "wd:limit:1",
    )
    .await
    .unwrap();

    let within = talent_wallet::withdrawn_within(&app.db, user, Currency::Eur, 24)
        .await
        .unwrap();
    assert_eq!(within, dec("200"));
}

#[tokio::test]
async fn a_refused_payout_does_not_consume_the_limit() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_refused").await;
    fund(&app, user, "500", Currency::Eur).await;

    ledger::withdraw(
        &app.db,
        user,
        dec("200"),
        Currency::Eur,
        "stripe",
        "tr_2",
        "wd:refused:1",
    )
    .await
    .unwrap();
    ledger::reverse_withdrawal(
        &app.db,
        user,
        dec("200"),
        Currency::Eur,
        "stripe",
        "wd:refused:1",
    )
    .await
    .unwrap();

    let within = talent_wallet::withdrawn_within(&app.db, user, Currency::Eur, 24)
        .await
        .unwrap();
    assert_eq!(
        within,
        dec("0"),
        "being told no by a provider must not cost someone their daily allowance"
    );
    assert_eq!(
        ledger::user_balance(&app.db, user, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("500"),
        "and the money is back"
    );
}

#[tokio::test]
async fn the_window_only_counts_recent_withdrawals() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_window").await;
    fund(&app, user, "500", Currency::Eur).await;

    ledger::withdraw(
        &app.db,
        user,
        dec("100"),
        Currency::Eur,
        "stripe",
        "tr_3",
        "wd:window:1",
    )
    .await
    .unwrap();
    // Age it past the window. Entries are immutable by trigger — that is the
    // point of them — so the guard is lifted for exactly this statement.
    // Travelling in time is the one thing a test legitimately needs to do
    // that production must never be able to.
    sqlx::query("ALTER TABLE ledger_entries DISABLE TRIGGER trg_ledger_entries_no_update")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE ledger_entries SET created_at = NOW() - INTERVAL '48 hours'
          WHERE transaction_id IN (SELECT id FROM ledger_transactions WHERE reason = 'withdrawal')",
    )
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE ledger_entries ENABLE TRIGGER trg_ledger_entries_no_update")
        .execute(&app.db)
        .await
        .unwrap();

    let within = talent_wallet::withdrawn_within(&app.db, user, Currency::Eur, 24)
        .await
        .unwrap();
    assert_eq!(within, dec("0"), "yesterday's limit is not today's");
}

#[tokio::test]
async fn limits_are_counted_per_currency() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_percur").await;
    fund(&app, user, "500", Currency::Eur).await;
    fund(&app, user, "50000", Currency::Xof).await;

    ledger::withdraw(
        &app.db,
        user,
        dec("100"),
        Currency::Eur,
        "stripe",
        "tr_4",
        "wd:cur:eur",
    )
    .await
    .unwrap();

    assert_eq!(
        talent_wallet::withdrawn_within(&app.db, user, Currency::Eur, 24)
            .await
            .unwrap(),
        dec("100")
    );
    assert_eq!(
        talent_wallet::withdrawn_within(&app.db, user, Currency::Xof, 24)
            .await
            .unwrap(),
        dec("0"),
        "a euro withdrawal must not eat into a franc limit"
    );
}

// ─── Compliance export ────────────────────────────────────────────

#[tokio::test]
async fn the_statement_reports_what_the_books_say() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_csv").await;
    fund(&app, user, "300", Currency::Eur).await;
    ledger::withdraw(
        &app.db,
        user,
        dec("100"),
        Currency::Eur,
        "stripe",
        "tr_csv",
        "wd:csv:1",
    )
    .await
    .unwrap();

    let csv = talent_wallet::statement_csv(&app.db, user).await.unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "date,reason,state,amount,currency,provider,reference"
    );
    assert!(csv.contains("capture"));
    assert!(csv.contains("withdrawal"));
    assert!(
        csv.contains("tr_csv"),
        "the provider reference is what reconciles"
    );
}

#[tokio::test]
async fn the_statement_quotes_fields_it_did_not_choose() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_csvquote").await;
    fund(&app, user, "100", Currency::Eur).await;

    // A provider reference is data from someone else's system.
    ledger::withdraw(
        &app.db,
        user,
        dec("50"),
        Currency::Eur,
        "stripe",
        "ref,with\"quotes",
        "wd:quote:1",
    )
    .await
    .unwrap();

    let csv = talent_wallet::statement_csv(&app.db, user).await.unwrap();
    assert!(
        csv.contains("\"ref,with\"\"quotes\""),
        "a comma in a reference must not shift every following column: {csv}"
    );
}

#[tokio::test]
async fn a_talent_sees_only_their_own_movements() {
    let app = TestApp::spawn().await;
    let mine = person(&app, "wal_mine").await;
    let theirs = person(&app, "wal_theirs").await;
    fund(&app, mine, "100", Currency::Eur).await;
    fund(&app, theirs, "999", Currency::Eur).await;

    let movements = talent_wallet::list_movements(&app.db, mine, 50)
        .await
        .unwrap();
    assert!(!movements.is_empty());
    let csv = talent_wallet::statement_csv(&app.db, mine).await.unwrap();
    // The amount column, not the whole file. Searching the text for "999"
    // also searched `created_at.to_rfc3339()`, whose fractional seconds carry
    // those three digits about once in three hundred rows — a test that failed
    // on the clock rather than on a leak.
    let amounts: Vec<&str> = csv
        .lines()
        .skip(1)
        .filter_map(|line| line.split(',').nth(3))
        .collect();
    assert!(
        !amounts.iter().any(|a| a.starts_with("999")),
        "the platform's float and other people's money are not theirs to see: {amounts:?}"
    );
}

#[tokio::test]
async fn movements_read_positive_for_the_talent() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wal_sign").await;
    fund(&app, user, "100", Currency::Eur).await;

    let movements = talent_wallet::list_movements(&app.db, user, 50)
        .await
        .unwrap();
    let capture = movements
        .iter()
        .find(|m| m.reason == "capture")
        .expect("capture present");
    assert!(
        capture.amount > dec("0"),
        "claims are stored negative; nobody outside the ledger should see that"
    );
}
