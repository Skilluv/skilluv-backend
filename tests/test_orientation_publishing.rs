//! Opening a trade's catalogue, one trade at a time (SKI-349).
//!
//! ## The rule this holds
//!
//! Migration 0239 seeded 130 design challenges as drafts, and said why: a
//! challenge nobody has read must not be handed to somebody who is learning.
//! The ticket adds a second rule that matters as much — **no trade is opened
//! without somebody who can review a submission to it**. A challenge you can
//! submit and nobody can judge is worse than a challenge that is not there:
//! the first wastes a week of somebody's work, the second wastes nothing.
//!
//! Both are enforced here rather than trusted, because both are the kind of
//! rule that holds until the evening somebody is in a hurry.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn a_curator(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'domain_curator:design', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();
    app.login(username).await;
    id
}

/// A design trade that has a review family, so the fixtures are realistic.
async fn a_trade(app: &TestApp) -> (Uuid, String, String) {
    let row: (Uuid, String, String) = sqlx::query_as(
        "SELECT id, slug, reviewer_group FROM orientations
          WHERE primary_domain = 'design' AND reviewer_group IS NOT NULL
          ORDER BY slug LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .expect("a design trade with a review family");
    row
}

/// Bring one trade to the state a curator reaches after an authoring session:
/// every brief written, including the five migration 0606 attached to it.
///
/// Returns how many challenges the trade then holds, because the seeded five
/// are part of the set and publishing takes the set whole.
async fn write_the_briefs(app: &TestApp, orientation: Uuid, author: Uuid) -> i64 {
    // Long enough to pass the stub floor, because the floor is what
    // distinguishes "somebody wrote this" from "the migration did".
    let brief = "## Constraints\n".to_string() + &"Real constraints. ".repeat(60);

    // The trade did not start empty: 0606 gave it the five seeded titles whose
    // briefs are still the shared boilerplate.
    sqlx::query("UPDATE challenge_templates SET instructions = $2 WHERE orientation_id = $1")
        .bind(orientation)
        .bind(&brief)
        .execute(&app.db)
        .await
        .unwrap();

    for i in 0..5 {
        sqlx::query(
            "INSERT INTO challenge_templates
                 (title, description, instructions, skill_domain, difficulty,
                  status, is_training, ai_policy, created_by, orientation_id)
             VALUES ($1, 'd', $2, 'design', 2, 'draft', TRUE,
                     'disclosure_required', $3, $4)",
        )
        .bind(format!("Written brief {i}"))
        .bind(&brief)
        .bind(author)
        .bind(orientation)
        .execute(&app.db)
        .await
        .unwrap();
    }

    sqlx::query_scalar("SELECT count(*) FROM challenge_templates WHERE orientation_id = $1")
        .bind(orientation)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn grant_reviewer(app: &TestApp, user: Uuid, group: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(format!("design_reviewer:{group}"))
    .execute(&app.db)
    .await
    .unwrap();
}

/// The readiness report names what is missing, in words somebody can act on.
#[tokio::test]
async fn the_report_says_what_is_still_in_the_way() {
    let app = TestApp::spawn().await;
    let curator = a_curator(&app, "orient_curator").await;
    let (orientation, slug, _group) = a_trade(&app).await;

    // The seeded state: challenges exist, briefs are stubs, nobody reviews.
    sqlx::query(
        "INSERT INTO challenge_templates
             (title, description, instructions, skill_domain, difficulty,
              status, is_training, ai_policy, created_by, orientation_id)
         VALUES ('A seeded stub', 'd', 'short', 'design', 2, 'draft', TRUE,
                 'disclosure_required', $1, $2)",
    )
    .bind(curator)
    .bind(orientation)
    .execute(&app.db)
    .await
    .unwrap();

    let body: Value = app
        .get(&format!("/api/admin/orientations/{slug}/challenges"))
        .await
        .json()
        .await
        .unwrap();
    let blockers = body["data"]["blockers"].as_array().expect("blockers");
    let joined = blockers
        .iter()
        .filter_map(|b| b.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        joined.contains("stub"),
        "the unwritten brief is not reported: {joined}"
    );
    assert!(
        joined.contains("reviewer") || joined.contains("judged"),
        "the missing reviewer is not reported: {joined}"
    );
    // And the challenge itself comes back, so the screen has something to edit.
    assert!(!body["data"]["challenges"].as_array().unwrap().is_empty());
}

/// The rule worth enforcing: no reviewer, no opening.
#[tokio::test]
async fn a_trade_nobody_can_review_is_not_opened() {
    let app = TestApp::spawn().await;
    let curator = a_curator(&app, "orient_noreviewer").await;
    let (orientation, slug, _group) = a_trade(&app).await;
    let all = write_the_briefs(&app, orientation, curator).await;

    // Briefs written, nobody holds the reviewer capability.
    let response = app
        .post(
            &format!("/api/admin/orientations/{slug}/challenges/publish"),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(response.status().as_u16(), 400);

    let still_draft: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE orientation_id = $1 AND status = 'draft'",
    )
    .bind(orientation)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(still_draft, all, "the refusal did not actually refuse");
}

/// And a stub brief is not opened either, reviewer or no reviewer.
#[tokio::test]
async fn a_stub_brief_is_not_opened() {
    let app = TestApp::spawn().await;
    let curator = a_curator(&app, "orient_stub").await;
    let (orientation, slug, group) = a_trade(&app).await;
    write_the_briefs(&app, orientation, curator).await;
    grant_reviewer(&app, curator, &group).await;

    // One stub among the written ones is enough to hold the whole set.
    sqlx::query(
        "INSERT INTO challenge_templates
             (title, description, instructions, skill_domain, difficulty,
              status, is_training, ai_policy, created_by, orientation_id)
         VALUES ('The one nobody wrote', 'd', 'short', 'design', 2, 'draft',
                 TRUE, 'disclosure_required', $1, $2)",
    )
    .bind(curator)
    .bind(orientation)
    .execute(&app.db)
    .await
    .unwrap();

    let status = app
        .post(
            &format!("/api/admin/orientations/{slug}/challenges/publish"),
            &serde_json::json!({}),
        )
        .await
        .status();
    assert_eq!(status.as_u16(), 400, "a stub brief was opened");
}

/// Written and reviewable: the whole set opens, and it opens together.
///
/// All of it or none of it. Publishing three of five leaves a trade whose
/// catalogue looks thin rather than unopened, and somebody arriving cannot
/// tell the difference.
#[tokio::test]
async fn a_written_and_reviewable_trade_opens_as_a_block() {
    let app = TestApp::spawn().await;
    let curator = a_curator(&app, "orient_ready").await;
    let (orientation, slug, group) = a_trade(&app).await;
    let all = write_the_briefs(&app, orientation, curator).await;
    grant_reviewer(&app, curator, &group).await;

    let response = app
        .post(
            &format!("/api/admin/orientations/{slug}/challenges/publish"),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(
        response.status().as_u16(),
        200,
        "{}",
        response.text().await.unwrap_or_default()
    );

    let (published, drafts): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status = 'published'),
                count(*) FILTER (WHERE status = 'draft')
           FROM challenge_templates WHERE orientation_id = $1",
    )
    .bind(orientation)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(published, all);
    assert_eq!(drafts, 0, "part of the set was left behind");
}

/// Curating one domain is enough. Nobody has to be a global admin to open
/// their own trade — but somebody with no curation at all is refused.
#[tokio::test]
async fn opening_a_trade_needs_the_domains_curation() {
    let app = TestApp::spawn().await;
    app.register_user("orient_nobody").await;
    app.login("orient_nobody").await;
    let (_id, slug, _group) = a_trade(&app).await;

    assert_eq!(
        app.get(&format!("/api/admin/orientations/{slug}/challenges"))
            .await
            .status()
            .as_u16(),
        403
    );
    assert_eq!(
        app.post(
            &format!("/api/admin/orientations/{slug}/challenges/publish"),
            &serde_json::json!({}),
        )
        .await
        .status()
        .as_u16(),
        403
    );
}

/// The 130 seeded challenges know their trade now (migration 0606).
///
/// This is the property the whole surface rests on: before it, the catalogue
/// could not answer "show me the five for this trade", so no screen could list
/// them and no trade could be opened on its own.
#[tokio::test]
async fn the_seeded_design_catalogue_knows_which_trade_each_belongs_to() {
    let app = TestApp::spawn().await;

    let (attached, trades): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(DISTINCT orientation_id)
           FROM challenge_templates
          WHERE skill_domain = 'design' AND orientation_id IS NOT NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(attached, 130, "the backfill of 0606 lost rows");
    assert_eq!(trades, 26, "a trade has no challenges and would look empty");
}
