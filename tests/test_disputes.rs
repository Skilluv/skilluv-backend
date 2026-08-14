//! The recourse the release window promised.
//!
//! Before this, `release::dispute` had no caller: the seven-day window was
//! seven days during which nothing could be done, and every hold released
//! on schedule whatever had happened. These tests hold the flow to the
//! shape it claims — the payer raises, the recipient answers, and only a
//! real disagreement reaches a human.

mod common;

use bigdecimal::BigDecimal;
use common::TestApp;
use skilluv_backend::services::disputes::{self, Outcome};
use skilluv_backend::services::ledger::{self, Currency, State};
use skilluv_backend::services::release;
use std::str::FromStr;
use uuid::Uuid;

async fn person(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name,
                            first_name, last_name, email_verified)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L', TRUE)
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// A session captured and held, exactly as `mark_completed` does it.
async fn held_session(app: &TestApp, mentor: Uuid, mentee: Uuid, amount: &str) -> Uuid {
    let session = Uuid::new_v4();
    let amount = BigDecimal::from_str(amount).unwrap();

    let posted = ledger::capture_for_recipient(
        &app.db,
        "stripe",
        format!("mentorship_session:{session}"),
        mentor,
        amount.clone(),
        BigDecimal::from(0),
        Currency::Eur,
        "mentorship_session",
        session,
    )
    .await
    .expect("capture");

    let window = release::window_for(&app.db, "mentorship_session")
        .await
        .unwrap();
    let mut tx = app.db.begin().await.unwrap();
    release::hold(
        &mut tx,
        release::Hold {
            ledger_transaction_id: posted.transaction_id(),
            beneficiary_id: mentor,
            subject_type: "mentorship_session",
            subject_id: session,
            amount: &amount,
            currency: Currency::Eur,
            hold_hours: window.hold_hours,
            payer_id: Some(mentee),
            payer_enterprise_id: None,
        },
    )
    .await
    .expect("hold");
    tx.commit().await.unwrap();

    session
}

async fn balance(app: &TestApp, user: Uuid, state: State) -> BigDecimal {
    ledger::user_balance(&app.db, user, state, Currency::Eur)
        .await
        .unwrap()
}

#[tokio::test]
async fn raising_a_dispute_freezes_the_money() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_mentor").await;
    let mentee = person(&app, "dsp_mentee").await;
    let session = held_session(&app, mentor, mentee, "100").await;

    assert_eq!(
        balance(&app, mentor, State::Pending).await,
        BigDecimal::from(100)
    );

    disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "the session never happened, the mentor did not join",
    )
    .await
    .expect("raised");

    // Out of pending and into disputed, so nothing can release it while the
    // argument is running.
    assert_eq!(
        balance(&app, mentor, State::Pending).await,
        BigDecimal::from(0)
    );
    assert_eq!(
        balance(&app, mentor, State::Disputed).await,
        BigDecimal::from(100)
    );
}

#[tokio::test]
async fn only_the_person_who_paid_may_dispute() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_own_mentor").await;
    let mentee = person(&app, "dsp_own_mentee").await;
    let stranger = person(&app, "dsp_stranger").await;
    let session = held_session(&app, mentor, mentee, "50").await;

    // Not the recipient, and not a passer-by. Disputing someone else's
    // payment would freeze money that is none of their business.
    for who in [mentor, stranger] {
        let refused = disputes::raise(
            &app.db,
            "mentorship_session",
            session,
            who,
            "I would like this money back please",
        )
        .await;
        assert!(refused.is_err());
    }
}

#[tokio::test]
async fn a_reason_is_required_because_someone_has_to_answer_it() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_why_mentor").await;
    let mentee = person(&app, "dsp_why_mentee").await;
    let session = held_session(&app, mentor, mentee, "50").await;

    let refused = disputes::raise(&app.db, "mentorship_session", session, mentee, "bad").await;
    assert!(
        refused.is_err(),
        "'disputed' with no reason is unanswerable"
    );
}

#[tokio::test]
async fn conceding_refunds_without_an_operator() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_yield_mentor").await;
    let mentee = person(&app, "dsp_yield_mentee").await;
    let session = held_session(&app, mentor, mentee, "80").await;

    let id = disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "I was charged twice for the same session",
    )
    .await
    .unwrap();

    // The outcome worth designing for: faster for both, and it costs the
    // platform nothing.
    disputes::concede(&app.db, id, mentor)
        .await
        .expect("conceded");

    assert_eq!(
        balance(&app, mentor, State::Disputed).await,
        BigDecimal::from(0),
        "the money left the mentor entirely"
    );
    assert_eq!(
        balance(&app, mentor, State::Available).await,
        BigDecimal::from(0)
    );

    let (status, resolved_by): (String, Option<Uuid>) =
        sqlx::query_as("SELECT status, resolved_by FROM disputes WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "refunded");
    assert!(resolved_by.is_none(), "no human was involved");
}

#[tokio::test]
async fn contesting_sends_it_to_a_human_and_nothing_moves() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_fight_mentor").await;
    let mentee = person(&app, "dsp_fight_mentee").await;
    let session = held_session(&app, mentor, mentee, "60").await;

    let id = disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "the mentor never showed up for the call",
    )
    .await
    .unwrap();

    disputes::contest(
        &app.db,
        id,
        mentor,
        "the call happened and lasted the full hour, here is the recording",
    )
    .await
    .expect("contested");

    // Still frozen: nobody is paid while two people disagree.
    assert_eq!(
        balance(&app, mentor, State::Disputed).await,
        BigDecimal::from(60)
    );

    let status: String = sqlx::query_scalar("SELECT status FROM disputes WHERE id = $1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "contested");
}

#[tokio::test]
async fn an_operator_decision_must_say_why() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_note_mentor").await;
    let mentee = person(&app, "dsp_note_mentee").await;
    let admin = person(&app, "dsp_note_admin").await;
    let session = held_session(&app, mentor, mentee, "40").await;

    let id = disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "the session was cut short after five minutes",
    )
    .await
    .unwrap();
    disputes::contest(&app.db, id, mentor, "the student left the call themselves")
        .await
        .unwrap();

    // Both parties read the decision. "Resolved" with no reason is how a
    // marketplace loses the trust of whichever side lost.
    let refused = disputes::decide(&app.db, id, admin, Outcome::Recipient, "ok").await;
    assert!(refused.is_err());

    disputes::decide(
        &app.db,
        id,
        admin,
        Outcome::Recipient,
        "the recording shows a full session; the student disconnected at minute five",
    )
    .await
    .expect("decided");

    assert_eq!(
        balance(&app, mentor, State::Available).await,
        BigDecimal::from(40),
        "the mentor was paid"
    );
}

#[tokio::test]
async fn a_disputed_hold_is_never_released_by_the_sweep() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_sweep_mentor").await;
    let mentee = person(&app, "dsp_sweep_mentee").await;
    let session = held_session(&app, mentor, mentee, "70").await;

    disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "this session was never delivered to me",
    )
    .await
    .unwrap();

    // Make the window long past. The sweep must still not touch it —
    // releasing money that is being argued over is the failure this whole
    // flow exists to prevent.
    sqlx::query("UPDATE pending_releases SET release_at = NOW() - INTERVAL '30 days'")
        .execute(&app.db)
        .await
        .unwrap();

    let report = release::sweep(&app.db).await.unwrap();
    assert_eq!(report.released, 0);
    assert_eq!(
        balance(&app, mentor, State::Disputed).await,
        BigDecimal::from(70)
    );
}

#[tokio::test]
async fn a_dispute_cannot_be_raised_twice() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_twice_mentor").await;
    let mentee = person(&app, "dsp_twice_mentee").await;
    let session = held_session(&app, mentor, mentee, "30").await;

    disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "the first complaint about this session",
    )
    .await
    .unwrap();

    let again = disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "the second complaint about this session",
    )
    .await;
    assert!(again.is_err(), "the money can only be in one place");
}

#[tokio::test]
async fn withdrawing_puts_the_money_back_on_its_normal_course() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "dsp_undo_mentor").await;
    let mentee = person(&app, "dsp_undo_mentee").await;
    let session = held_session(&app, mentor, mentee, "25").await;

    let id = disputes::raise(
        &app.db,
        "mentorship_session",
        session,
        mentee,
        "I think I was charged for a session I cancelled",
    )
    .await
    .unwrap();

    disputes::withdraw(&app.db, id, mentee)
        .await
        .expect("withdrawn");

    assert_eq!(
        balance(&app, mentor, State::Available).await,
        BigDecimal::from(25)
    );
    let status: String = sqlx::query_scalar("SELECT status FROM disputes WHERE id = $1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "withdrawn");
}

#[tokio::test]
async fn every_dispute_kind_has_copy_in_every_locale() {
    use skilluv_backend::services::i18n;

    let app = TestApp::spawn().await;
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM notification_kinds WHERE kind LIKE 'dispute.%'")
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(kinds.len(), 5);

    let mut missing = Vec::new();
    for locale in i18n::available() {
        for kind in &kinds {
            for part in ["title", "body", "cta"] {
                let key = format!("notification.{kind}.{part}");
                if i18n::t(locale, &key) == key {
                    missing.push(format!("{locale}: {key}"));
                }
            }
        }
    }
    assert!(missing.is_empty(), "untranslated: {missing:#?}");
}
