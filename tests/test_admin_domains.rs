//! Running a domain without running the platform.
//!
//! The claim under test is the separation itself: a curator of one domain can
//! read that domain and nothing else, and cannot do any of the things `admin`
//! also grants. Until these endpoints existed, handing somebody the design
//! calendar meant handing them the ban button and the financial dashboard.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn grant(app: &TestApp, user: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test_setup') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_curator_reads_their_domain_and_only_theirs() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_curator").await;
    grant(&app, curator, "domain_curator:design").await;
    app.login("dom_curator").await;

    let mine = app.get("/api/admin/domains/design/overview").await;
    assert_eq!(mine.status(), 200, "{}", mine.text().await.unwrap());

    // The whole point of the scope. A design curator has no business reading
    // the security backlog, and `admin` was the only thing that used to open
    // either.
    let not_mine = app.get("/api/admin/domains/security/overview").await;
    assert_eq!(not_mine.status(), 403);
}

#[tokio::test]
async fn a_curator_of_all_reads_every_domain() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_all").await;
    grant(&app, curator, "domain_curator:all").await;
    app.login("dom_all").await;

    // One grant rather than seven, so the list cannot fall out of sync the
    // day an eighth domain is added.
    for domain in ["design", "code", "ai", "security"] {
        let resp = app.get(&format!("/api/admin/domains/{domain}/overview")).await;
        assert_eq!(resp.status(), 200, "{domain}");
    }
}

#[tokio::test]
async fn somebody_with_no_capability_reads_nothing() {
    let app = TestApp::spawn().await;
    a_user(&app, "dom_nobody").await;
    app.login("dom_nobody").await;

    for path in ["overview", "reviewers", "featured-queue"] {
        let resp = app.get(&format!("/api/admin/domains/design/{path}")).await;
        assert_eq!(resp.status(), 403, "{path}");
    }
}

#[tokio::test]
async fn a_revoked_capability_closes_the_door_again() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_revoked").await;
    grant(&app, curator, "domain_curator:design").await;
    app.login("dom_revoked").await;
    assert_eq!(app.get("/api/admin/domains/design/overview").await.status(), 200);

    sqlx::query(
        "UPDATE user_capabilities SET revoked_at = NOW()
          WHERE user_id = $1 AND capability = 'domain_curator:design'",
    )
    .bind(curator)
    .execute(&app.db)
    .await
    .unwrap();

    // Read live, not from the token. A capability taken back has to shut the
    // door on the next request, not on the next login.
    assert_eq!(app.get("/api/admin/domains/design/overview").await.status(), 403);
}

#[tokio::test]
async fn a_domain_nobody_defined_is_refused() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_unknown").await;
    grant(&app, curator, "domain_curator:all").await;
    app.login("dom_unknown").await;

    // 400 rather than an empty overview: a page of zeroes for a typo reads as
    // a dead domain rather than as a misspelling.
    let resp = app.get("/api/admin/domains/knitting/overview").await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_overview_counts_what_is_in_the_domain_and_not_next_to_it() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_counts").await;
    grant(&app, curator, "domain_curator:all").await;
    app.login("dom_counts").await;

    // The catalogue is seeded, so the figures are read as deltas. An absolute
    // assertion here would pass today and break the day somebody adds a
    // challenge to the seed, which is not a bug in this endpoint.
    let before: serde_json::Value = app
        .get("/api/admin/domains/design/overview")
        .await
        .json()
        .await
        .unwrap();
    let design_before = before["data"]["challenges_published"].as_i64().unwrap();

    // A published challenge must be training or attached to a project — the
    // platform does not publish exercises that lead nowhere.
    sqlx::query(
        "INSERT INTO challenge_templates (title, description, instructions,
                                          skill_domain, difficulty, status, is_training)
         VALUES ('Un défi', 'Description', 'Consignes', 'design', 2, 'published', TRUE),
                ('Un autre', 'Description', 'Consignes', 'code', 2, 'published', TRUE)",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let after: serde_json::Value = app
        .get("/api/admin/domains/design/overview")
        .await
        .json()
        .await
        .unwrap();
    let data = &after["data"];
    assert_eq!(data["skill_domain"], "design");

    // One of the two was code. A page that counted it would be counting the
    // domain next door.
    assert_eq!(
        data["challenges_published"].as_i64().unwrap(),
        design_before + 1,
        "{after}"
    );

    // Never computed is null, not zero. Nobody has been reviewed here, and a
    // mean of zero rounds would say the opposite of what is true.
    assert!(data["mean_rounds_to_approval"].is_null(), "{after}");
    assert!(data["oldest_pending_review_hours"].is_null());
}

#[tokio::test]
async fn the_window_is_bounded() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_window").await;
    grant(&app, curator, "domain_curator:design").await;
    app.login("dom_window").await;

    assert_eq!(
        app.get("/api/admin/domains/design/overview?days=0").await.status(),
        400
    );
    assert_eq!(
        app.get("/api/admin/domains/design/overview?days=4000").await.status(),
        400
    );
}

#[tokio::test]
async fn reviewers_are_listed_by_family_with_their_open_work() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_rev_admin").await;
    grant(&app, curator, "domain_curator:design").await;

    let reviewer = a_user(&app, "dom_reviewer").await;
    grant(&app, reviewer, "design_reviewer:brand").await;
    grant(&app, reviewer, "design_reviewer:motion").await;
    // A code capability on the same person: this endpoint must not report it
    // on the design page.
    grant(&app, reviewer, "code_reviewer:web").await;

    app.login("dom_rev_admin").await;
    let body: serde_json::Value = app
        .get("/api/admin/domains/design/reviewers")
        .await
        .json()
        .await
        .unwrap();

    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{body}");
    let families: Vec<&str> = rows[0]["families"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert!(families.contains(&"brand"), "{body}");
    assert!(families.contains(&"motion"));
    assert!(!families.contains(&"web"), "a code family on a design page: {body}");

    // Nothing decided yet — null rather than zero, for the same reason as
    // above.
    assert_eq!(rows[0]["decisions_total"], 0);
    assert!(rows[0]["mean_hours_to_decide"].is_null(), "{body}");
}

#[tokio::test]
async fn an_expired_capability_is_not_a_reviewer() {
    let app = TestApp::spawn().await;
    let curator = a_user(&app, "dom_exp_admin").await;
    grant(&app, curator, "domain_curator:design").await;

    let lapsed = a_user(&app, "dom_lapsed").await;
    grant(&app, lapsed, "design_reviewer:brand").await;
    sqlx::query(
        "UPDATE user_capabilities SET expires_at = NOW() - INTERVAL '1 day'
          WHERE user_id = $1",
    )
    .bind(lapsed)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("dom_exp_admin").await;
    let body: serde_json::Value = app
        .get("/api/admin/domains/design/reviewers")
        .await
        .json()
        .await
        .unwrap();
    // Listing somebody whose capability has lapsed would have a curator
    // chasing a reviewer the queue will refuse.
    assert!(body["data"].as_array().unwrap().is_empty(), "{body}");
}

#[tokio::test]
async fn the_challenge_list_filters_and_counts_honestly() {
    let app = TestApp::spawn().await;
    app.register_admin("dom_chal_admin").await;
    app.login("dom_chal_admin").await;

    async fn total(app: &TestApp, query: &str) -> i64 {
        let body: serde_json::Value = app
            .get(&format!("/api/admin/challenges{query}"))
            .await
            .json()
            .await
            .unwrap();
        body["pagination"]["total"].as_i64().unwrap()
    }

    let design_before = total(&app, "?skill_domain=design").await;
    let all_before = total(&app, "").await;

    sqlx::query(
        "INSERT INTO challenge_templates (title, description, instructions,
                                          skill_domain, difficulty, status, is_training)
         SELECT 'Défi ' || i, 'Description', 'Consignes',
                CASE WHEN i % 2 = 0 THEN 'design' ELSE 'code' END,
                2, 'published', TRUE
           FROM generate_series(1, 10) AS i",
    )
    .execute(&app.db)
    .await
    .unwrap();

    // Five of the ten were design. The filter is what the endpoint gained;
    // before it, a curator opening a domain was served the whole platform.
    assert_eq!(total(&app, "?skill_domain=design").await, design_before + 5);
    assert_eq!(total(&app, "").await, all_before + 10);

    let body: serde_json::Value = app
        .get("/api/admin/challenges?skill_domain=design&per_page=2")
        .await
        .json()
        .await
        .unwrap();

    // The page holds two and the total says otherwise. It used to report the
    // length of the page it had just built — a number that agreed with itself
    // and with nothing else.
    assert_eq!(body["data"].as_array().unwrap().len(), 2, "{body}");
    assert!(body["pagination"]["total"].as_i64().unwrap() > 2);
    assert_eq!(body["pagination"]["page"], 1);

    let page_two: serde_json::Value = app
        .get("/api/admin/challenges?skill_domain=design&per_page=2&page=2")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(page_two["data"].as_array().unwrap().len(), 2);
    assert_ne!(page_two["data"][0]["id"], body["data"][0]["id"]);

    // A status nobody defined is refused rather than silently ignored.
    assert_eq!(
        app.get("/api/admin/challenges?status=whenever").await.status(),
        400
    );
}

#[tokio::test]
async fn the_financial_dashboard_reads_the_capability_and_not_the_column() {
    let app = TestApp::spawn().await;
    // A complete admin: the legacy column, a second factor, and the
    // capability. Then the capability alone is taken away — the column still
    // says `admin` and the token still carries it.
    app.register_admin("dom_fin_admin").await;
    app.login("dom_fin_admin").await;
    assert_eq!(app.get("/api/admin/dashboard/financial").await.status(), 200);

    sqlx::query(
        "UPDATE user_capabilities SET revoked_at = NOW()
          WHERE capability = 'admin'
            AND user_id = (SELECT id FROM users WHERE username = 'dom_fin_admin')",
    )
    .execute(&app.db)
    .await
    .unwrap();

    // This file used to read `auth.role` out of the JWT, which stopped being
    // the answer at P21. Somebody whose capability had been revoked kept
    // their financial page for as long as their token lived.
    let after = app.get("/api/admin/dashboard/financial").await;
    assert_eq!(
        after.status(),
        403,
        "a revoked capability has to close every door: {}",
        after.text().await.unwrap()
    );
}
