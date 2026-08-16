//! The two public doors into code work: what to build, and where the people
//! who build it talk.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

/// A curated project with one open, unclaimed, ingested issue in `orientation`.
async fn an_open_issue(
    app: &TestApp,
    project_slug: &str,
    orientation_slug: Option<&str>,
    difficulty: i16,
    languages: &[&str],
) -> Uuid {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'feed_owner'")
            .fetch_optional(&app.db)
            .await
            .unwrap();
    let owner = match existing {
        Some(id) => id,
        None => {
            app.register_user("feed_owner").await;
            sqlx::query_scalar("SELECT id FROM users WHERE username = 'feed_owner'")
                .fetch_one(&app.db)
                .await
                .unwrap()
        }
    };

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id, tech_stack)
         VALUES ($1, $1, 'user', $2, ARRAY['rust']::TEXT[]) RETURNING id",
    )
    .bind(project_slug)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let orientation: Option<Uuid> = match orientation_slug {
        Some(slug) => sqlx::query_scalar("SELECT resolve_orientation($1)")
            .bind(slug)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        None => None,
    };

    let langs: Vec<String> = languages.iter().map(|s| s.to_string()).collect();
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, difficulty,
             status, orientation_id, code_languages, external_metadata)
         VALUES ($1, $2, 'x', 'code', 'github_issue', $3,
                 'open', $4, $5, '{\"issue_url\": \"https://github.com/x/y/issues/1\"}'::JSONB)
         RETURNING id",
    )
    .bind(project)
    .bind(format!("issue on {project_slug}"))
    .bind(difficulty)
    .bind(orientation)
    .bind(&langs)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn feed(app: &TestApp, query: &str) -> Value {
    let resp = app.get(&format!("/api/code/first-issues{query}")).await;
    assert_eq!(resp.status(), 200, "feed must answer");
    resp.json().await.unwrap()
}

#[tokio::test]
async fn the_feed_filters_by_trade() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "feed-front", Some("web-frontend-developer"), 2, &[]).await;
    an_open_issue(&app, "feed-kernel", Some("kernel-driver-developer"), 2, &[]).await;

    let body = feed(&app, "?orientation=web-frontend-developer").await;
    let issues = body["data"]["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "one trade, one issue");
    assert_eq!(issues[0]["project_slug"], "feed-front");
}

#[tokio::test]
async fn an_old_trade_slug_still_reaches_its_issues() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "feed-renamed", Some("web-frontend-developer"), 2, &[]).await;

    // Somebody bookmarked the URL before migration 0173 renamed the trade.
    // Answering nothing would look like the feed had emptied.
    let body = feed(&app, "?orientation=dev-frontend").await;
    assert_eq!(body["data"]["issues"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_trade_nobody_knows_is_not_silently_everything() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "feed-typo", Some("web-frontend-developer"), 2, &[]).await;

    // A typo answering "here is the whole catalogue" is how somebody claims
    // kernel work believing it is frontend.
    let resp = app
        .get("/api/code/first-issues?orientation=metier-invente")
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn a_claimed_issue_leaves_the_feed() {
    let app = TestApp::spawn().await;
    let slice = an_open_issue(&app, "feed-claimed", Some("web-frontend-developer"), 2, &[]).await;

    let before = feed(&app, "?orientation=web-frontend-developer").await;
    assert_eq!(before["data"]["issues"].as_array().unwrap().len(), 1);

    app.register_user("feed_claimer").await;
    let claimer: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'feed_claimer'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE project_slices SET claimed_by_user_id = $2, claimed_at = NOW() WHERE id = $1",
    )
    .bind(slice)
    .bind(claimer)
    .execute(&app.db)
    .await
    .unwrap();

    // Different query string, so the previous answer's cache entry does not
    // decide this one.
    let after = feed(&app, "?orientation=web-frontend-developer&limit=29").await;
    assert!(
        after["data"]["issues"].as_array().unwrap().is_empty(),
        "listing work somebody already took wastes the reader's time"
    );
}

#[tokio::test]
async fn the_feed_stops_at_a_first_issue_difficulty() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "feed-easy", Some("web-frontend-developer"), 2, &[]).await;
    an_open_issue(&app, "feed-hard", Some("web-frontend-developer"), 5, &[]).await;

    let body = feed(&app, "?orientation=web-frontend-developer").await;
    let issues = body["data"]["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "a first-issue feed is not the backlog");
    assert_eq!(issues[0]["project_slug"], "feed-easy");

    let wider = feed(&app, "?orientation=web-frontend-developer&max_difficulty=5").await;
    assert_eq!(wider["data"]["issues"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_language_filter_reads_the_slice_then_the_repository() {
    let app = TestApp::spawn().await;
    // The slice says nothing, so the repository's stack answers for it.
    an_open_issue(
        &app,
        "feed-langfallback",
        Some("systems-programmer"),
        2,
        &[],
    )
    .await;
    an_open_issue(
        &app,
        "feed-langslice",
        Some("systems-programmer"),
        2,
        &["zig"],
    )
    .await;

    let rust = feed(&app, "?language=rust").await;
    let slugs: Vec<&str> = rust["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["project_slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["feed-langfallback"]);

    let zig = feed(&app, "?language=zig").await;
    let slugs: Vec<&str> = zig["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["project_slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["feed-langslice"]);
}

#[tokio::test]
async fn the_ecosystems_listing_names_where_to_go_and_when() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/code/ecosystems").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let all = body["data"]["ecosystems"].as_array().unwrap();
    assert!(all.len() >= 10, "a listing of two is not a curation");

    // Every row must earn its place: a link with no events is a bookmark,
    // and a summary is what makes this different from an awesome-list.
    for eco in all {
        assert!(
            !eco["notable_events"].as_array().unwrap().is_empty(),
            "{} lists no event",
            eco["language"]
        );
        assert!(
            !eco["summary"].as_str().unwrap().is_empty(),
            "{} says nothing about itself",
            eco["language"]
        );
    }

    let one = app.get("/api/code/ecosystems?language=rust").await;
    let body: Value = one.json().await.unwrap();
    let rows = body["data"]["ecosystems"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["language"], "rust");
}
