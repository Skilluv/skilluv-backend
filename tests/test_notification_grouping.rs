//! Ten mentions in one thread are one notification. In ten threads, ten.
//!
//! The rule this proves is the one that makes grouping useful rather than
//! destructive: the unit is the **context**, not the kind. Folding by kind
//! alone would turn ten conversations into "10 mentions" and destroy the
//! only thing that mattered — where.

mod common;

use common::TestApp;
use serde_json::json;
use skilluv_backend::services::notify::{self, Ctx, Recipient};
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

/// One mention of `user`, by `author`, in the thread `post`.
async fn mention(app: &TestApp, user: Uuid, author: &str, post: Uuid) -> notify::Delivery {
    notify::send(
        Ctx::db_only(&app.db),
        Recipient::User(user),
        "social.mention",
    )
    .arg("author", author)
    .arg("excerpt", "regarde ça")
    .payload(json!({ "post_id": post, "comment_id": Uuid::new_v4() }))
    .execute()
    .await
    .expect("delivery")
}

async fn rows(app: &TestApp, user: Uuid) -> Vec<(String, i32, serde_json::Value)> {
    sqlx::query_as(
        "SELECT title, group_count, group_actors FROM notifications
          WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn several_mentions_in_one_thread_are_one_line() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_one").await;
    let post = Uuid::new_v4();

    mention(&app, user, "Awa", post).await;
    mention(&app, user, "Kofi", post).await;
    let third = mention(&app, user, "Fatou", post).await;

    let rows = rows(&app, user).await;
    assert_eq!(rows.len(), 1, "one thread, one line");
    assert_eq!(rows[0].1, 3, "standing for three events");

    // The line names who, newest first, so "Fatou and 2 others" is true
    // rather than alphabetical.
    let actors: Vec<String> = serde_json::from_value(rows[0].2.clone()).unwrap();
    assert_eq!(actors, vec!["Fatou", "Kofi", "Awa"]);
    assert!(rows[0].0.contains('3'), "the count is in the title");

    // The third event was folded, not delivered again — that is the whole
    // point, and the report says so rather than pretending it was new.
    assert_eq!(third.grouped, 1);
    assert_eq!(third.in_app, 0);
}

#[tokio::test]
async fn mentions_in_different_threads_stay_separate() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_many").await;

    // Ten people naming you in ten discussions is ten things to know
    // about. Collapsing them would destroy the only useful information.
    for _ in 0..10 {
        mention(&app, user, "Awa", Uuid::new_v4()).await;
    }

    assert_eq!(rows(&app, user).await.len(), 10, "ten threads, ten lines");
}

#[tokio::test]
async fn the_same_person_twice_is_named_once() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_dupe").await;
    let post = Uuid::new_v4();

    mention(&app, user, "Awa", post).await;
    mention(&app, user, "Awa", post).await;

    let rows = rows(&app, user).await;
    let actors: Vec<String> = serde_json::from_value(rows[0].2.clone()).unwrap();
    assert_eq!(actors, vec!["Awa"], "one name, however many messages");
    assert_eq!(rows[0].1, 2, "but the count is still two");
}

#[tokio::test]
async fn a_read_notification_does_not_absorb_anything() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_read").await;
    let post = Uuid::new_v4();

    mention(&app, user, "Awa", post).await;
    sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    // Merging into a line someone has already seen would make it change
    // under them, and they would never learn the second thing happened.
    mention(&app, user, "Kofi", post).await;

    assert_eq!(
        rows(&app, user).await.len(),
        2,
        "read is a boundary, not a bucket"
    );
}

#[tokio::test]
async fn an_expired_window_starts_a_new_line() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_window").await;
    let post = Uuid::new_v4();

    mention(&app, user, "Awa", post).await;
    // `social.mention` folds over an hour. Two hours later it is a new
    // conversation, not a continuation of the old one.
    sqlx::query(
        "UPDATE notifications SET created_at = NOW() - INTERVAL '2 hours' WHERE user_id = $1",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    mention(&app, user, "Kofi", post).await;
    assert_eq!(rows(&app, user).await.len(), 2);
}

#[tokio::test]
async fn money_never_groups() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_money").await;
    let subject = Uuid::new_v4();

    // Two payouts are two payouts, whatever they are about. A kind with no
    // window folds nothing, and every kind carrying money has none.
    for _ in 0..3 {
        notify::send(Ctx::db_only(&app.db), Recipient::User(user), "payout.sent")
            .arg("amount", "5000 XOF")
            .arg("destination", "MTN")
            .payload(json!({ "target_id": subject }))
            .execute()
            .await
            .unwrap();
    }

    assert_eq!(
        rows(&app, user).await.len(),
        3,
        "every payout is its own line"
    );

    let windowed: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM notification_kinds
          WHERE group_window_seconds IS NOT NULL
            AND category IN ('payments', 'account', 'mentorship')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        windowed.is_empty(),
        "these carry money or a decision and must never fold: {windowed:?}"
    );
}

#[tokio::test]
async fn a_notification_with_no_context_never_groups() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_nocontext").await;

    // `social.mention` has a window, but a payload naming no subject gives
    // no context to group by — so it must not fold with an unrelated one.
    for _ in 0..3 {
        notify::send(
            Ctx::db_only(&app.db),
            Recipient::User(user),
            "social.mention",
        )
        .arg("author", "Awa")
        .arg("excerpt", "…")
        .execute()
        .await
        .unwrap();
    }

    assert_eq!(rows(&app, user).await.len(), 3);
}

#[tokio::test]
async fn every_grouping_kind_has_grouped_copy_in_every_locale() {
    use skilluv_backend::services::i18n;

    let app = TestApp::spawn().await;
    let grouping: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM notification_kinds WHERE group_window_seconds IS NOT NULL ORDER BY kind",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(!grouping.is_empty());

    // Without this a kind given a window renders its own key as a title the
    // first time two events arrive together — in production, on a Sunday.
    let mut missing = Vec::new();
    for locale in i18n::available() {
        for kind in &grouping {
            for part in ["title", "body"] {
                let key = format!("notification.{kind}.grouped.{part}");
                if i18n::t(locale, &key) == key {
                    missing.push(format!("{locale}: {key}"));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "untranslated grouped copy: {missing:#?}"
    );
}

#[tokio::test]
async fn the_unread_count_matches_the_number_of_lines() {
    let app = TestApp::spawn().await;
    let user = person(&app, "grp_count").await;
    let post = Uuid::new_v4();

    for author in ["Awa", "Kofi", "Fatou"] {
        mention(&app, user, author, post).await;
    }

    // Three events, one line, one unread. A badge showing 3 over a list
    // with one entry is the bug grouping introduces if the counter is
    // bumped on absorption.
    let unread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = FALSE",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(unread, 1);
}
