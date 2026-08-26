//! The ops wizard, the guides it points at, and the toolkit behind them.

mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn six_answers_produce_a_recommendation_and_are_kept() {
    let app = TestApp::spawn().await;
    app.register_user("ops_wizard").await;
    app.login("ops_wizard").await;

    let resp = app
        .post(
            "/api/ops/onboarding",
            &json!({
                "level": "junior",
                "trades": ["sre"],
                "cloud_experience": ["none"],
                "weekly_hours": "3_to_10",
                "objective": "learn",
                "oncall_experience": "never",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let jv: serde_json::Value = resp.json().await.unwrap();
    let rec = &jv["data"]["recommendation"];

    assert_eq!(rec["guides"][0], "ops-onboarding-sre");
    assert_eq!(rec["oncall_ready"], false);
    // Somebody with no cloud account is told where to practise before
    // anything else: without that answer the domain is out of reach for a
    // budget reason rather than a skill one.
    assert!(!rec["practise_at"].as_str().unwrap().is_empty());

    // The answers live in `user_domain_profiles` (migration 0306), not in
    // columns on `users`: seven domains asking for eight columns each is
    // fifty-six columns on the table every query already touches.
    let (answers, skipped): (serde_json::Value, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT p.answers, p.skipped_at
               FROM user_domain_profiles p
               JOIN users u ON u.id = p.user_id
              WHERE u.username = 'ops_wizard' AND p.domain = 'ops'",
        )
        .fetch_one(&app.db)
        .await
        .unwrap();

    assert_eq!(answers["level"], "junior");
    assert_eq!(answers["trades"][0], "sre");
    assert_eq!(answers["oncall_experience"], "never");
    assert!(skipped.is_none(), "answering clears any earlier skip");

    drop(app);
}

#[tokio::test]
async fn three_trades_are_refused() {
    let app = TestApp::spawn().await;
    app.register_user("ops_greedy").await;
    app.login("ops_greedy").await;

    let resp = app
        .post(
            "/api/ops/onboarding",
            &json!({
                "level": "engineer",
                "trades": ["sre", "cloud-architect", "devops-engineer"],
                "cloud_experience": ["aws"],
                "weekly_hours": "over_10",
                "objective": "find_paid_work",
                "oncall_experience": "regular",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400, "picking three means picking none");

    drop(app);
}

#[tokio::test]
async fn a_trade_that_does_not_exist_is_refused_with_the_list() {
    let app = TestApp::spawn().await;
    app.register_user("ops_typo").await;
    app.login("ops_typo").await;

    let resp = app
        .post(
            "/api/ops/onboarding",
            &json!({
                "level": "junior",
                "trades": ["sre-engineer"],
                "cloud_experience": [],
                "weekly_hours": "3_to_10",
                "objective": "learn",
                "oncall_experience": "never",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    // A typo must not silently point somebody at a guide that does not
    // exist, so the refusal names what does.
    let jv: serde_json::Value = resp.json().await.unwrap();
    let message = jv["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("sre"),
        "the list is in the message: {message}"
    );

    drop(app);
}

#[tokio::test]
async fn skipping_is_recorded_so_the_wizard_stops_asking() {
    let app = TestApp::spawn().await;
    app.register_user("ops_skipper").await;
    app.login("ops_skipper").await;

    let resp = app.post("/api/ops/onboarding/skip", &json!({})).await;
    assert_eq!(resp.status(), 200);

    let skipped: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT p.skipped_at FROM user_domain_profiles p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = 'ops_skipper' AND p.domain = 'ops'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // "Stop asking" and "has not answered yet" are different states, and
    // without the distinction the wizard reappears forever for exactly the
    // people who declined it.
    assert!(skipped.is_some());

    drop(app);
}

#[tokio::test]
async fn every_trade_has_a_guide_in_both_languages() {
    let app = TestApp::spawn().await;

    let trades: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations WHERE primary_domain = 'ops' AND NOT is_archived",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(trades.len(), 8);

    for locale in ["fr", "en"] {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM content_guides
              WHERE skill_domain = 'ops' AND kind = 'onboarding' AND locale = $1",
        )
        .bind(locale)
        .fetch_one(&app.db)
        .await
        .unwrap();

        // A trade with no guide is a person arriving at an empty page, which
        // is caught here rather than by them.
        assert_eq!(count, 8, "eight {locale} onboarding guides");
    }

    drop(app);
}

#[tokio::test]
async fn the_ops_guides_do_not_leak_into_the_code_catalogue() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/guides?kind=onboarding&domain=code").await;
    assert_eq!(resp.status(), 200);
    let jv: serde_json::Value = resp.json().await.unwrap();
    let guides = jv["data"].as_array().unwrap();

    assert!(
        guides
            .iter()
            .all(|g| { !g["slug"].as_str().unwrap_or_default().starts_with("ops-") }),
        "a path that says code must not list ops guides"
    );

    drop(app);
}

#[tokio::test]
async fn the_toolkit_says_what_each_thing_costs_to_reach() {
    let app = TestApp::spawn().await;

    let resp = app
        .get("/api/domains/ops/toolkit?category=cloud_free_tier")
        .await;
    assert_eq!(resp.status(), 200);
    let jv: serde_json::Value = resp.json().await.unwrap();
    let resources = jv["data"]["resources"].as_array().unwrap();

    assert!(resources.len() >= 5);

    // The access note is the reason this list exists rather than a page of
    // logos: a free tier whose end nobody described is an invoice waiting.
    for resource in resources {
        let note = resource["access_note"].as_str().unwrap();
        assert!(
            note.len() > 30,
            "{} has no usable access note",
            resource["slug"]
        );
    }

    drop(app);
}

#[tokio::test]
async fn a_guide_falls_back_rather_than_disappearing() {
    let app = TestApp::spawn().await;

    // Arabic has no ops guides. A half-translated catalogue should show the
    // untranslated page rather than a 404 that reads as "does not exist".
    let resp = app
        .get_with_header("/api/guides/ops-onboarding-sre", "accept-language", "ar")
        .await;
    assert_eq!(resp.status(), 200);

    let jv: serde_json::Value = resp.json().await.unwrap();
    // English, because the chain is asked-for, then English, then French —
    // English sits in the middle since it became the locale this content is
    // written in. This asserted French, which was right when French was the
    // fallback and silently wrong afterwards.
    assert_eq!(jv["data"]["locale"], "en");

    drop(app);
}
