//! One wizard, seven domains.
//!
//! Code used to have its own, writing to eight columns on `users`, while every
//! other domain used `user_domain_profiles`. Two places to read "what did this
//! person say" is one place too many, and they already disagreed.
//!
//! What is under test is the folding: that each domain keeps its own words,
//! that the answers land in one table, and that the wizard finishes with
//! advice rather than a shrug.

mod common;
use common::TestApp;
use serde_json::{Value, json};

async fn a_user(app: &TestApp, username: &str) {
    app.register_user(username).await;
    app.login(username).await;
}

async fn answers(app: &TestApp, username: &str, domain: &str) -> Value {
    sqlx::query_scalar(
        "SELECT p.answers FROM user_domain_profiles p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = $1 AND p.domain = $2",
    )
    .bind(username)
    .bind(domain)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn each_domain_keeps_its_own_words() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_words").await;

    // `staff` is a code rank; the design ladder has no word for it. Flattening
    // the two into one list would have meant inventing one.
    let code = app
        .put(
            "/api/users/me/domain-profile/code",
            &json!({"level": "staff", "goal": "find_paid_work"}),
        )
        .await;
    assert_eq!(code.status(), 200, "{}", code.text().await.unwrap());

    // The same word, offered to design, is not a design level.
    let wrong = app
        .put(
            "/api/users/me/domain-profile/design",
            &json!({"level": "staff"}),
        )
        .await;
    assert_eq!(wrong.status(), 400);

    let design = app
        .put(
            "/api/users/me/domain-profile/design",
            &json!({"level": "senior", "goal": "paid_missions"}),
        )
        .await;
    assert_eq!(design.status(), 200);
}

#[tokio::test]
async fn the_wizard_finishes_with_advice_somebody_can_argue_with() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_advice").await;

    let body: Value = app
        .put(
            "/api/users/me/domain-profile/design",
            &json!({
                "level": "debutant",
                "weekly_hours": "lt3",
                "goal": "portfolio",
                "preferred_families": ["design-brand-identity"],
                "challenge_preference": "individual",
                "main_tool": "figma",
            }),
        )
        .await
        .json()
        .await
        .unwrap();

    let rec = &body["data"]["recommendation"];
    assert!(!rec["headline"].as_str().unwrap().is_empty(), "{body}");
    assert!(
        !rec["because"].as_str().unwrap().is_empty(),
        "advice nobody can argue with is advice nobody follows"
    );
    // The trade narrows the feed: a design trade is what the catalogue is
    // organised by.
    assert!(
        rec["feed_query"]
            .as_str()
            .unwrap()
            .contains("design-brand-identity"),
        "{body}"
    );
    // Three hours a week earns its own sentence.
    assert!(
        rec["next_steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("une seule chose")),
        "{body}"
    );
}

#[tokio::test]
async fn a_read_carries_no_recommendation() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_read").await;
    app.put(
        "/api/users/me/domain-profile/design",
        &json!({"level": "debutant"}),
    )
    .await;

    // The recommendation answers the wizard; it is not a property of the
    // profile. Recomputing it on every read would let it drift from the words
    // the person actually saw.
    let body: Value = app
        .get("/api/users/me/domain-profile/design")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"].get("recommendation").is_none(), "{body}");
    assert_eq!(body["data"]["answers"]["level"], "debutant");
}

#[tokio::test]
async fn skipping_is_recorded_and_answering_afterwards_undoes_it() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_skip").await;

    assert_eq!(
        app.post("/api/users/me/domain-profile/design/skip", &json!({}))
            .await
            .status(),
        204
    );

    let (completed, skipped): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT p.completed_at, p.skipped_at FROM user_domain_profiles p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = 'wiz_skip' AND p.domain = 'design'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    // Skipped is not unanswered: without the distinction the wizard reappears
    // forever for the people who least wanted it.
    assert!(completed.is_none());
    assert!(skipped.is_some());

    app.put(
        "/api/users/me/domain-profile/design",
        &json!({"level": "debutant"}),
    )
    .await;

    let (completed, skipped): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT p.completed_at, p.skipped_at FROM user_domain_profiles p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = 'wiz_skip' AND p.domain = 'design'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    // Answering after skipping is no longer skipping.
    assert!(completed.is_some());
    assert!(skipped.is_none());
}

#[tokio::test]
async fn a_declared_portfolio_is_recorded_unconfirmed() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_portfolio").await;

    app.put(
        "/api/users/me/domain-profile/design",
        &json!({
            "level": "practitioner",
            "portfolio_url": "https://behance.net/quelquun",
        }),
    )
    .await;

    let row: Option<(String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT es.provider, es.verified_at FROM external_signals es
           JOIN users u ON u.id = es.user_id
          WHERE u.username = 'wiz_portfolio'",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap();

    let (provider, verified) = row.expect("the portfolio should have been recorded");
    assert_eq!(provider, "behance", "read off the host, not asked for");
    // Pasting a link proves nothing. A moderator confirms it before it counts
    // as evidence, and until then a recruiter search must not see it.
    assert!(verified.is_none());
}

#[tokio::test]
async fn a_field_belonging_to_another_domain_is_refused() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_wrong_field").await;

    // Storing it is worse than it looks: nobody's design recommender reads
    // `compute`, so the answer would sit there looking saved.
    let resp = app
        .put(
            "/api/users/me/domain-profile/design",
            &json!({"compute": "cloud_large"}),
        )
        .await;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"].as_str().unwrap().contains("ai"),
        "the message has to say which domain: {body}"
    );
}

#[tokio::test]
async fn a_family_nobody_has_is_refused_in_both_domains() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_typo").await;

    // Design names trades by slug, code names families by reviewer group.
    // Both are checked against the catalogue, because a typo recommends
    // nothing and does it silently.
    for (domain, family) in [("design", "design-quantique"), ("code", "quantique")] {
        let resp = app
            .put(
                &format!("/api/users/me/domain-profile/{domain}"),
                &json!({"preferred_families": [family]}),
            )
            .await;
        assert_eq!(resp.status(), 400, "{domain}");
    }

    // And the real ones are accepted, each in its own shape.
    for (domain, family) in [("design", "design-brand-identity"), ("code", "systems")] {
        let resp = app
            .put(
                &format!("/api/users/me/domain-profile/{domain}"),
                &json!({"preferred_families": [family]}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{domain}");
    }
}

#[tokio::test]
async fn an_answer_that_lists_everything_is_refused() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_greedy").await;

    // A wizard answer that names everything sorts nothing, which is the
    // failure mode of asking a question whose answer can be "all of them".
    let resp = app
        .put(
            "/api/users/me/domain-profile/code",
            &json!({"main_tools": ["rust", "go", "python", "elixir"]}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_answers_land_in_one_table_for_every_domain() {
    let app = TestApp::spawn().await;
    a_user(&app, "wiz_one_table").await;

    app.put(
        "/api/users/me/domain-profile/code",
        &json!({"level": "mid", "main_tools": ["rust"]}),
    )
    .await;
    app.put(
        "/api/users/me/domain-profile/design",
        &json!({"level": "practitioner", "main_tool": "figma"}),
    )
    .await;

    // Two domains, two rows, one table. The code answers used to live on
    // `users` — eight columns, and fifty-six if every domain had followed.
    assert_eq!(answers(&app, "wiz_one_table", "code").await["level"], "mid");
    assert_eq!(
        answers(&app, "wiz_one_table", "design").await["level"],
        "practitioner"
    );

    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'users' AND column_name LIKE 'code\\_%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(columns, 0, "the old shape is gone, not deprecated");
}
