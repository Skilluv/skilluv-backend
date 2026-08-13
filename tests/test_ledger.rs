//! Double-entry ledger (migration 0153, `services::ledger`).
//!
//! Two things are being proved here. That the books cannot be made to lie —
//! unbalanced postings, single-legged postings and edits are refused by the
//! database, not by a convention someone has to remember. And that the signs
//! are the right way round, which is the mistake that costs the most and
//! shows the least.

mod common;
use bigdecimal::BigDecimal;
use common::TestApp;
use skilluv_backend::services::ledger::{self, Account, Currency, Leg, Posting, State, owed};
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

// ─── The rules the database enforces ──────────────────────────────

#[tokio::test]
async fn an_unbalanced_posting_is_refused() {
    let app = TestApp::spawn().await;
    let user = person(&app, "led_unbalanced").await;

    let result = ledger::post(
        &app.db,
        Posting::new(
            "bogus",
            vec![
                Leg::debit(
                    Account::Psp {
                        provider: "stripe",
                        currency: Currency::Eur,
                    },
                    dec("100"),
                ),
                // 40 short: money would appear from nowhere.
                Leg::credit(owed(user, State::Pending, Currency::Eur), dec("60")),
            ],
        ),
    )
    .await;

    assert!(
        result.is_err(),
        "a posting that does not sum to zero must fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("balance") || msg.contains("off by"),
        "the error should say what is wrong, got: {msg}"
    );
}

#[tokio::test]
async fn a_single_legged_posting_is_refused() {
    let app = TestApp::spawn().await;
    let user = person(&app, "led_single").await;

    let result = ledger::post(
        &app.db,
        Posting::new(
            "bogus",
            vec![Leg::credit(
                owed(user, State::Available, Currency::Eur),
                dec("100"),
            )],
        ),
    )
    .await;
    assert!(result.is_err(), "money must move between two accounts");
}

#[tokio::test]
async fn entries_cannot_be_edited_or_deleted() {
    let app = TestApp::spawn().await;
    let user = person(&app, "led_immutable").await;

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_immutable",
        user,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "test",
        Uuid::new_v4(),
    )
    .await
    .unwrap();

    let updated = sqlx::query("UPDATE ledger_entries SET amount = 1 WHERE amount <> 1")
        .execute(&app.db)
        .await;
    assert!(updated.is_err(), "entries are append-only");

    let deleted = sqlx::query("DELETE FROM ledger_entries")
        .execute(&app.db)
        .await;
    assert!(deleted.is_err(), "history cannot be erased");
}

#[tokio::test]
async fn an_entry_cannot_sit_in_an_account_of_another_currency() {
    let app = TestApp::spawn().await;

    // Built by hand, since the typed helpers make this unrepresentable.
    let account: Uuid = sqlx::query_scalar(
        "INSERT INTO ledger_accounts (code, kind, currency)
         VALUES ('psp:stripe:settlement:EUR', 'psp', 'EUR') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let other: Uuid = sqlx::query_scalar(
        "INSERT INTO ledger_accounts (code, kind, currency)
         VALUES ('external:world:EUR', 'external', 'EUR') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let txn: Uuid =
        sqlx::query_scalar("INSERT INTO ledger_transactions (reason) VALUES ('mix') RETURNING id")
            .fetch_one(&app.db)
            .await
            .unwrap();

    let mut tx = app.db.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO ledger_entries (transaction_id, account_id, amount, currency)
         VALUES ($1, $2, 100, 'XOF'), ($1, $3, -100, 'XOF')",
    )
    .bind(txn)
    .bind(account)
    .bind(other)
    .execute(&mut *tx)
    .await
    .unwrap();
    let committed = tx.commit().await;

    assert!(
        committed.is_err(),
        "XOF entries in EUR accounts would balance across unrelated money"
    );
}

// ─── Signs ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_capture_splits_between_recipient_and_platform() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_mentor").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_capture",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    // The asset: we now hold 100 at Stripe.
    let held = ledger::balance(
        &app.db,
        &Account::Psp {
            provider: "stripe",
            currency: Currency::Eur,
        },
    )
    .await
    .unwrap();
    assert_eq!(held, dec("100"), "the money is at the provider");

    // The claim: 85 owed to the mentor, and not yet withdrawable.
    let pending = ledger::user_balance(&app.db, mentor, State::Pending, Currency::Eur)
        .await
        .unwrap();
    assert_eq!(pending, dec("85"));

    let available = ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
        .await
        .unwrap();
    assert_eq!(
        available,
        dec("0"),
        "nothing is withdrawable before the payer's window closes"
    );
}

#[tokio::test]
async fn releasing_moves_the_claim_without_moving_the_money() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_release").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_release",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    ledger::release(
        &app.db,
        mentor,
        dec("85"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        dec("0")
    );
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("85")
    );

    // The point: the money never left the provider. Only who it belongs to
    // changed.
    assert_eq!(
        ledger::balance(
            &app.db,
            &Account::Psp {
                provider: "stripe",
                currency: Currency::Eur
            }
        )
        .await
        .unwrap(),
        dec("100")
    );
}

#[tokio::test]
async fn a_withdrawal_drops_both_the_claim_and_the_float() {
    let app = TestApp::spawn().await;
    let talent = person(&app, "led_withdraw").await;
    let subject = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_wd",
        talent,
        dec("100"),
        dec("0"),
        Currency::Eur,
        "bounty",
        subject,
    )
    .await
    .unwrap();
    ledger::release(
        &app.db,
        talent,
        dec("100"),
        Currency::Eur,
        "bounty",
        subject,
    )
    .await
    .unwrap();
    ledger::withdraw(
        &app.db,
        talent,
        dec("100"),
        Currency::Eur,
        "stripe",
        "tr_123",
        "withdraw:led_withdraw:1",
    )
    .await
    .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, talent, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("0"),
        "we no longer owe them anything"
    );
    assert_eq!(
        ledger::balance(
            &app.db,
            &Account::Psp {
                provider: "stripe",
                currency: Currency::Eur
            }
        )
        .await
        .unwrap(),
        dec("0"),
        "and the float went with it"
    );
}

#[tokio::test]
async fn money_can_arrive_on_one_rail_and_leave_on_another() {
    // The case the whole design exists for: a student pays by card in EUR,
    // a mentor in Benin is paid in XOF over Mobile Money. Two providers, two
    // currencies, one ledger tying them together.
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_corridor").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_corridor",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    // Paid out in XOF: a separate claim, funded separately. The ledger does
    // not convert — exchange is a regulated activity and belongs to the
    // provider, not to us.
    ledger::post(
        &app.db,
        Posting::new(
            "fx_settlement",
            vec![
                Leg::debit(owed(mentor, State::Pending, Currency::Eur), dec("85")),
                Leg::credit(
                    Account::Psp {
                        provider: "stripe",
                        currency: Currency::Eur,
                    },
                    dec("85"),
                ),
                Leg::debit(
                    Account::Psp {
                        provider: "mtn",
                        currency: Currency::Xof,
                    },
                    dec("55000"),
                ),
                Leg::credit(owed(mentor, State::Available, Currency::Xof), dec("55000")),
            ],
        )
        .about("mentorship_session", session),
    )
    .await
    .expect("a posting balancing per currency is valid across currencies");

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Xof)
            .await
            .unwrap(),
        dec("55000")
    );
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        dec("0")
    );
}

// ─── Idempotency ──────────────────────────────────────────────────

#[tokio::test]
async fn replaying_a_capture_does_not_double_it() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_replay").await;
    let session = Uuid::new_v4();

    let first = ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_1",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    // A provider webhook is delivered more than once by design.
    let second = ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_1",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    assert!(!first.was_replay());
    assert!(second.was_replay(), "the second delivery must be a no-op");
    assert_eq!(first.transaction_id(), second.transaction_id());
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        dec("85"),
        "credited once, not twice"
    );
}

#[tokio::test]
async fn releasing_twice_releases_once() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_release2").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_2",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    for _ in 0..3 {
        ledger::release(
            &app.db,
            mentor,
            dec("85"),
            Currency::Eur,
            "mentorship_session",
            session,
        )
        .await
        .unwrap();
    }

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("85"),
        "two reviewers acting at once must not release twice"
    );
}

// ─── Disputes ─────────────────────────────────────────────────────

#[tokio::test]
async fn a_dispute_freezes_pending_money() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_dispute").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_3",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    ledger::hold_dispute(
        &app.db,
        mentor,
        dec("85"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Pending, Currency::Eur)
            .await
            .unwrap(),
        dec("0")
    );
    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Disputed, Currency::Eur)
            .await
            .unwrap(),
        dec("85"),
        "frozen: neither party can move it while a human decides"
    );
}

#[tokio::test]
async fn refunding_a_dispute_returns_our_commission_too() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_refund").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_4",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    ledger::hold_dispute(
        &app.db,
        mentor,
        dec("85"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    ledger::refund_from_dispute(
        &app.db,
        "stripe",
        mentor,
        dec("85"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Disputed, Currency::Eur)
            .await
            .unwrap(),
        dec("0")
    );
    // Keeping a fee on a refunded service would be indefensible, and would
    // also leave the books unbalanced.
    let revenue = ledger::balance(
        &app.db,
        &Account::Platform {
            bucket: "revenue",
            currency: Currency::Eur,
        },
    )
    .await
    .unwrap();
    assert_eq!(revenue, dec("0"), "our commission went back as well");

    assert_eq!(
        ledger::balance(
            &app.db,
            &Account::Psp {
                provider: "stripe",
                currency: Currency::Eur
            }
        )
        .await
        .unwrap(),
        dec("0"),
        "the full 100 left the provider"
    );
}

#[tokio::test]
async fn a_dispute_resolved_for_the_recipient_makes_it_withdrawable() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "led_resolved").await;
    let session = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_5",
        mentor,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    ledger::hold_dispute(
        &app.db,
        mentor,
        dec("85"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();
    ledger::resolve_dispute_for_recipient(
        &app.db,
        mentor,
        dec("85"),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, mentor, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("85")
    );
}

// ─── Reversal ─────────────────────────────────────────────────────

#[tokio::test]
async fn a_reversed_withdrawal_gives_the_money_back_and_keeps_the_trace() {
    let app = TestApp::spawn().await;
    let talent = person(&app, "led_reverse").await;
    let subject = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_6",
        talent,
        dec("50"),
        dec("0"),
        Currency::Eur,
        "bounty",
        subject,
    )
    .await
    .unwrap();
    ledger::release(&app.db, talent, dec("50"), Currency::Eur, "bounty", subject)
        .await
        .unwrap();
    ledger::withdraw(
        &app.db,
        talent,
        dec("50"),
        Currency::Eur,
        "stripe",
        "tr_x",
        "wd:1",
    )
    .await
    .unwrap();
    ledger::reverse_withdrawal(&app.db, talent, dec("50"), Currency::Eur, "stripe", "wd:1")
        .await
        .unwrap();

    assert_eq!(
        ledger::user_balance(&app.db, talent, State::Available, Currency::Eur)
            .await
            .unwrap(),
        dec("50"),
        "the recipient can try again"
    );

    // Both the attempt and its reversal remain, because that is how anyone
    // answers "why was this paid twice" six months later.
    let movements: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ledger_transactions WHERE reason IN ('withdrawal', 'withdrawal_reversed')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(movements, 2, "history is appended to, never rewritten");
}

// ─── Reconciliation ───────────────────────────────────────────────

#[tokio::test]
async fn provider_positions_report_what_we_hold_where() {
    let app = TestApp::spawn().await;
    let a = person(&app, "led_pos_a").await;
    let b = person(&app, "led_pos_b").await;

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_a",
        a,
        dec("100"),
        dec("10"),
        Currency::Eur,
        "bounty",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    ledger::capture_for_recipient(
        &app.db,
        "mtn",
        "mm_b",
        b,
        dec("50000"),
        dec("5000"),
        Currency::Xof,
        "bounty",
        Uuid::new_v4(),
    )
    .await
    .unwrap();

    let positions = ledger::provider_positions(&app.db).await.unwrap();
    let stripe = positions
        .iter()
        .find(|p| p.account_code.contains("stripe"))
        .expect("stripe position");
    let mtn = positions
        .iter()
        .find(|p| p.account_code.contains("mtn"))
        .expect("mtn position");

    assert_eq!(stripe.balance, dec("100"));
    assert_eq!(mtn.balance, dec("50000"));
}

#[tokio::test]
async fn the_books_balance_across_every_transaction() {
    // The invariant the whole table exists for: whatever happened, the sum
    // of every entry in a currency is zero. If this ever fails, money was
    // created or destroyed.
    let app = TestApp::spawn().await;
    let user = person(&app, "led_invariant").await;
    let subject = Uuid::new_v4();

    ledger::capture_for_recipient(
        &app.db,
        "stripe",
        "pi_inv",
        user,
        dec("100"),
        dec("15"),
        Currency::Eur,
        "bounty",
        subject,
    )
    .await
    .unwrap();
    ledger::release(&app.db, user, dec("85"), Currency::Eur, "bounty", subject)
        .await
        .unwrap();
    ledger::withdraw(
        &app.db,
        user,
        dec("85"),
        Currency::Eur,
        "stripe",
        "tr_inv",
        "wd:inv",
    )
    .await
    .unwrap();

    let total: BigDecimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM ledger_entries WHERE currency = 'EUR'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(total, dec("0"), "money was created or destroyed");
}

// ─── Routing ──────────────────────────────────────────────────────

#[tokio::test]
async fn benin_routes_to_mobile_money_not_stripe() {
    use skilluv_backend::services::payout::{Rail, routes};
    let app = TestApp::spawn().await;

    let found = routes(&app.db, Some("BJ"), Currency::Xof, Rail::MobileMoney)
        .await
        .unwrap();
    assert!(
        !found.is_empty(),
        "Benin must be reachable — Stripe does not serve it"
    );
    assert_eq!(
        found[0].provider, "mtn",
        "country rule wins over the catch-all"
    );
}

#[tokio::test]
async fn an_unknown_country_falls_back_to_the_catch_all() {
    use skilluv_backend::services::payout::{Rail, routes};
    let app = TestApp::spawn().await;

    let found = routes(&app.db, Some("ZZ"), Currency::Eur, Rail::BankAccount)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].country.is_none());
    assert_eq!(found[0].provider, "stripe");
}

#[tokio::test]
async fn a_disabled_route_is_not_offered() {
    use skilluv_backend::services::payout::{Rail, routes};
    let app = TestApp::spawn().await;

    sqlx::query("UPDATE payout_routes SET enabled = FALSE WHERE country = 'BJ'")
        .execute(&app.db)
        .await
        .unwrap();

    let found = routes(&app.db, Some("BJ"), Currency::Xof, Rail::MobileMoney)
        .await
        .unwrap();
    assert!(
        found.iter().all(|r| r.country.is_none()),
        "disabling an outage must be one column, and reversible"
    );
}

#[tokio::test]
async fn the_registry_skips_providers_this_deployment_cannot_use() {
    use skilluv_backend::services::payout::{PayoutRegistry, Rail};
    let app = TestApp::spawn().await;

    // Empty registry: every route names a provider with no credentials here.
    let registry = PayoutRegistry::new();
    let result = registry
        .resolve(&app.db, Some("BJ"), Currency::Xof, Rail::MobileMoney)
        .await;

    // `Arc<dyn PayoutProvider>` is not Debug, so unwrap the error by hand.
    let msg = match result {
        Ok(_) => panic!("nothing is configured, so nothing can pay"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("mtn") || msg.contains("no payout provider"),
        "the message should name what was tried, got: {msg}"
    );
}
