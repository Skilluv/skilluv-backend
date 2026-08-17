//! The public feed: what gets on it, who decides, and what it says when
//! there is nothing to show.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A deliverable that reaches `verified`, which is what puts it on the feed.
async fn a_verified_artifact(app: &TestApp, user: Uuid, artifact_type: &str) -> Uuid {
    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, status, is_training)
         VALUES ($1, 'x', 'x', 'code', 2, 'published', TRUE) RETURNING id",
    )
    .bind(format!("chal {}", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .unwrap();

    let deliverable: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (user_id, challenge_id, artifact_type, artifact_url, verifiable_by,
             ai_assistance_level)
         VALUES ($1, $2, $3, 'https://github.com/acme/widgets/pull/7', 'github_webhook', 'none')
         RETURNING id",
    )
    .bind(user)
    .bind(challenge)
    .bind(artifact_type)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE deliverables SET verification_status = 'verified', verified_at = NOW()
          WHERE id = $1",
    )
    .bind(deliverable)
    .execute(&app.db)
    .await
    .unwrap();

    deliverable
}

async fn feed(app: &TestApp, query: &str) -> Value {
    let resp = app.get(&format!("/api/feed/public{query}")).await;
    assert_eq!(resp.status(), 200, "the feed must answer");
    resp.json().await.unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Getting on the feed
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_merged_contribution_reaches_the_feed_with_somewhere_to_go() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_contributor").await;
    a_verified_artifact(&app, user, "pr_merged").await;

    let body = feed(&app, "").await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);

    let item = &items[0];
    assert_eq!(item["kind"], "pr_merged_upstream");
    assert_eq!(item["subject_label"], "feed_contributor");
    // The whole argument: every line leads somewhere a stranger can open.
    assert!(
        item["artifact_url"]
            .as_str()
            .unwrap()
            .starts_with("https://"),
        "a line with nowhere to go is a claim"
    );
}

#[tokio::test]
async fn an_issued_attestation_points_at_its_verification_page() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_attested").await;
    let deliverable = a_verified_artifact(&app, user, "pr_merged").await;

    skilluv_backend::services::code_attestations::pr_merged_upstream(&app.db, user, deliverable)
        .await
        .unwrap();

    let body = feed(&app, "?kind=attestation_issued").await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        items[0]["artifact_url"]
            .as_str()
            .unwrap()
            .contains("/verify/"),
        "an attestation must lead to the page that proves it"
    );
}

#[tokio::test]
async fn nothing_reaches_the_feed_twice() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_once").await;
    let deliverable = a_verified_artifact(&app, user, "pr_merged").await;

    // Verified again by another code path — a webhook and a poller arriving
    // together is the normal case, not the rare one.
    sqlx::query("UPDATE deliverables SET verification_status = 'pending' WHERE id = $1")
        .bind(deliverable)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE deliverables SET verification_status = 'verified', verified_at = NOW()
          WHERE id = $1",
    )
    .bind(deliverable)
    .execute(&app.db)
    .await
    .unwrap();

    let body = feed(&app, "").await;
    assert_eq!(
        body["data"]["items"].as_array().unwrap().len(),
        1,
        "one thing happened"
    );
}

#[tokio::test]
async fn a_revoked_artefact_leaves_the_feed_without_being_erased() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_revoked").await;
    let deliverable = a_verified_artifact(&app, user, "pr_merged").await;

    assert_eq!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    sqlx::query("UPDATE deliverables SET revoked_at = NOW() WHERE id = $1")
        .bind(deliverable)
        .execute(&app.db)
        .await
        .unwrap();

    assert!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // The row stays: somebody investigating a complaint needs to see that it
    // was shown, and when it stopped being.
    let retracted: (bool, Option<String>) = sqlx::query_as(
        "SELECT retracted_at IS NOT NULL, retraction_reason
           FROM public_artifact_events WHERE source_id = $1",
    )
    .bind(deliverable)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(retracted.0);
    assert!(retracted.1.is_some());
}

// ═══════════════════════════════════════════════════════════════════
// Consent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_hidden_profile_is_never_on_the_feed() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_hidden").await;
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    a_verified_artifact(&app, user, "pr_merged").await;

    // Somebody who hid their profile is not somebody who wants a ticker
    // announcing them, whatever the per-kind default says.
    assert!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn turning_a_kind_off_takes_down_what_is_already_up() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_optout").await;
    a_verified_artifact(&app, user, "pr_merged").await;
    assert_eq!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    app.login("feed_optout").await;
    let resp = app
        .post(
            "/api/users/me/public-feed-preferences",
            &json!({"kind": "pr_merged_upstream", "visible": false}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["existing_items_changed"], 1,
        "somebody asking to be off the page is not asking to be off it from now on"
    );

    assert!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // And turning it back on restores it — the row was never deleted.
    app.post(
        "/api/users/me/public-feed-preferences",
        &json!({"kind": "pr_merged_upstream", "visible": true}),
    )
    .await;
    assert_eq!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn withdrawing_clears_everything_at_once() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_withdraw").await;
    a_verified_artifact(&app, user, "pr_merged").await;
    let deliverable = a_verified_artifact(&app, user, "documentation").await;
    let _ = deliverable;

    assert_eq!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    app.login("feed_withdraw").await;
    let resp = app
        .post("/api/users/me/public-feed-preferences/withdraw", &json!({}))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["items_removed"], 2);

    assert!(
        feed(&app, "").await["data"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn money_is_hidden_unless_somebody_asks_for_it() {
    let app = TestApp::spawn().await;
    a_user(&app, "feed_defaults").await;
    app.login("feed_defaults").await;

    let body: Value = app
        .get("/api/users/me/public-feed-preferences")
        .await
        .json()
        .await
        .unwrap();
    let preferences = body["data"]["preferences"].as_array().unwrap();
    assert_eq!(
        preferences.len(),
        6,
        "every kind, including the unchosen ones"
    );

    for p in preferences {
        let kind = p["kind"].as_str().unwrap();
        let visible = p["visible"].as_bool().unwrap();
        let already_public = p["already_public_elsewhere"].as_bool().unwrap();

        // Repeating something already public is fair. Publishing what
        // somebody earns because they took a bounty is not.
        assert_eq!(
            visible, already_public,
            "{kind}: the default must follow whether the artefact is already public"
        );
        assert!(p["is_default"].as_bool().unwrap());
    }
}

#[tokio::test]
async fn a_kind_the_feed_does_not_show_is_refused() {
    let app = TestApp::spawn().await;
    a_user(&app, "feed_unknown_kind").await;
    app.login("feed_unknown_kind").await;

    let resp = app
        .post(
            "/api/users/me/public-feed-preferences",
            &json!({"kind": "points_earned", "visible": true}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "a feed of points proves nothing to anybody"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Reading it
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_feed_is_public_and_paginates_by_keyset() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_many").await;
    for _ in 0..5 {
        a_verified_artifact(&app, user, "pr_merged").await;
    }

    // Unauthenticated: this is what a landing page polls.
    let first = feed(&app, "?limit=2").await;
    let items = first["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let cursor = first["data"]["next_cursor"].as_str().expect("a full page");

    let second = feed(&app, &format!("?limit=2&after={cursor}")).await;
    let next = second["data"]["items"].as_array().unwrap();
    assert_eq!(next.len(), 2);

    // No overlap: an offset would skip or repeat exactly when the feed is
    // busy, which is the only time anybody paginates it.
    let first_ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    for item in next {
        assert!(!first_ids.contains(&item["id"].as_str().unwrap()));
    }

    // The last page carries no cursor: handing one back means a caller polls
    // forever for a page that will always be empty.
    let last = feed(&app, "?limit=20").await;
    assert!(last["data"]["next_cursor"].is_null());
}

#[tokio::test]
async fn an_unusable_cursor_is_refused_rather_than_restarted() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/feed/public?after=garbage").await;
    assert_eq!(
        resp.status(),
        400,
        "silently restarting makes a client re-read the whole feed and never find out why"
    );
}

#[tokio::test]
async fn an_empty_forge_does_not_claim_to_be_live() {
    let app = TestApp::spawn().await;

    let body = feed(&app, "").await;
    assert!(body["data"]["items"].as_array().unwrap().is_empty());
    // The one thing on the landing page a careful visitor can check. A
    // pulsing dot over an empty feed is the fabricated social proof this
    // whole table replaced.
    assert_eq!(body["data"]["live"], false);
    assert_eq!(body["data"]["artifacts_per_day"], 0.0);
}

#[tokio::test]
async fn density_decides_whether_the_dot_pulses() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "feed_busy").await;

    // Below the threshold, the honest presentation is a "latest work" list.
    for _ in 0..3 {
        a_verified_artifact(&app, user, "pr_merged").await;
    }
    assert_eq!(feed(&app, "").await["data"]["live"], false);

    // Thirty-five artefacts over seven days is five a day, which is the
    // threshold: enough that the first line is never two days old.
    for _ in 0..32 {
        a_verified_artifact(&app, user, "pr_merged").await;
    }
    assert_eq!(feed(&app, "").await["data"]["live"], true);
}
