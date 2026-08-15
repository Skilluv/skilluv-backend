//! What an operator can see, and what they deliberately cannot do.

mod common;

use common::TestApp;
use uuid::Uuid;

#[tokio::test]
async fn the_money_screens_are_operator_only() {
    let app = TestApp::spawn().await;
    app.register_user("money_nobody").await;
    app.login("money_nobody").await;

    // Every one of these lists amounts, provider references and masked
    // destinations. Not a screen for a logged-in stranger.
    for path in [
        "/api/admin/money/overview",
        "/api/admin/money/payments",
        "/api/admin/money/payouts",
        "/api/admin/money/routes",
        "/api/admin/money/methods",
    ] {
        let resp = app.get(path).await;
        assert_eq!(resp.status().as_u16(), 403, "{path} must be admin-only");
    }
}

#[tokio::test]
async fn the_overview_answers_the_questions_asked_at_nine_in_the_morning() {
    let app = TestApp::spawn().await;
    app.register_admin("money_admin").await;
    app.login("money_admin").await;

    let body: serde_json::Value = app
        .get("/api/admin/money/overview")
        .await
        .json()
        .await
        .unwrap();
    let data = &body["data"];

    // Each of these was previously unanswerable without a psql prompt.
    for field in [
        "paid_but_undelivered",
        "payments_pending",
        "payouts_pending",
        "payouts_failed_today",
        "disputes_awaiting_decision",
        "notifications_abandoned",
        "ledger_snapshot_drift",
    ] {
        assert!(data[field].is_number(), "{field} is missing");
    }

    // The one that must be zero on a healthy system: it means the ledger's
    // own arithmetic disagrees with itself.
    assert_eq!(data["ledger_snapshot_drift"], 0);
}

#[tokio::test]
async fn money_taken_and_nothing_given_is_one_query_away() {
    let app = TestApp::spawn().await;
    app.register_admin("money_owed").await;
    app.login("money_owed").await;

    let subject = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO payments
            (subject_type, subject_id, provider, method, amount, currency, status,
             succeeded_at, idempotency_key)
         VALUES ('mentorship_session', $1, 'fedapay', 'mobile_money', 5000, 'XOF',
                 'succeeded', NOW(), 'admin-test-1')",
    )
    .bind(subject)
    .execute(&app.db)
    .await
    .unwrap();

    let body: serde_json::Value = app
        .get("/api/admin/money/payments?undelivered=true")
        .await
        .json()
        .await
        .unwrap();
    let found = body["data"]["payments"].as_array().unwrap();
    assert_eq!(found.len(), 1);
    // Text, not a float: money on a screen must be the number in the
    // database rather than a rounding of it.
    assert_eq!(found[0]["amount"], "5000.0000");
    assert!(found[0]["fulfilled_at"].is_null());
}

#[tokio::test]
async fn both_directions_appear_in_one_list() {
    let app = TestApp::spawn().await;
    app.register_admin("money_routes").await;
    app.login("money_routes").await;

    let body: serde_json::Value = app
        .get("/api/admin/money/routes")
        .await
        .json()
        .await
        .unwrap();
    let routes = body["data"]["routes"].as_array().unwrap();

    // An operator closing a corridor during an outage wants both
    // directions in front of them, not two screens.
    assert!(routes.iter().any(|r| r["direction"] == "in"));
    assert!(routes.iter().any(|r| r["direction"] == "out"));
}

#[tokio::test]
async fn closing_a_corridor_is_one_call_and_it_is_reversible() {
    let app = TestApp::spawn().await;
    app.register_admin("money_toggle").await;
    app.login("money_toggle").await;

    let route: Uuid = sqlx::query_scalar(
        "SELECT id FROM collection_routes WHERE currency = 'XOF' AND method = 'mobile_money' LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let off = app
        .post(
            &format!("/api/admin/money/routes/{route}/toggle"),
            &serde_json::json!({ "enabled": false, "direction": "in" }),
        )
        .await;
    assert_eq!(off.status().as_u16(), 200);

    let enabled: bool = sqlx::query_scalar("SELECT enabled FROM collection_routes WHERE id = $1")
        .bind(route)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(!enabled);

    // The whole point of `enabled` being a column: an outage is a
    // one-column change and putting it back is another.
    app.post(
        &format!("/api/admin/money/routes/{route}/toggle"),
        &serde_json::json!({ "enabled": true, "direction": "in" }),
    )
    .await;
    let enabled: bool = sqlx::query_scalar("SELECT enabled FROM collection_routes WHERE id = $1")
        .bind(route)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(enabled);
}

#[tokio::test]
async fn a_toggle_must_say_which_table_it_means() {
    let app = TestApp::spawn().await;
    app.register_admin("money_direction").await;
    app.login("money_direction").await;

    // The two tables have separate id spaces. Guessing would eventually
    // disable the wrong corridor, and the report would say it worked.
    let resp = app
        .post(
            &format!("/api/admin/money/routes/{}/toggle", Uuid::new_v4()),
            &serde_json::json!({ "enabled": false, "direction": "sideways" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn nothing_here_moves_money() {
    let app = TestApp::spawn().await;
    app.register_admin("money_readonly").await;
    app.login("money_readonly").await;

    // An operator panel that can move money is one that will, at three in
    // the morning, on the wrong row. The only writes are toggles.
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&app.db)
        .await
        .unwrap();

    for path in [
        "/api/admin/money/overview",
        "/api/admin/money/payments",
        "/api/admin/money/payouts",
        "/api/admin/money/routes",
        "/api/admin/money/methods",
    ] {
        app.get(path).await;
    }

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(before, after);
}
