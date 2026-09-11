//! The open pool: one listing of unclaimed work, for every trade.

use crate::common::TestApp;
use serde_json::Value;
use uuid::Uuid;

/// A user to own the fixture projects. Created once per database.
async fn owner(app: &TestApp) -> Uuid {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'pool_owner'")
            .fetch_optional(&app.db)
            .await
            .unwrap();
    match existing {
        Some(id) => id,
        None => {
            app.register_user("pool_owner").await;
            sqlx::query_scalar("SELECT id FROM users WHERE username = 'pool_owner'")
                .fetch_one(&app.db)
                .await
                .unwrap()
        }
    }
}

async fn a_project(app: &TestApp, slug: &str) -> Uuid {
    let owner = owner(app).await;
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id, tech_stack)
         VALUES ($1, $1, 'user', $2, ARRAY['rust']::TEXT[]) RETURNING id",
    )
    .bind(slug)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// An open, unclaimed upstream ticket - the shape code has always had.
async fn an_open_issue(
    app: &TestApp,
    project_slug: &str,
    orientation_slug: Option<&str>,
    difficulty: i16,
    languages: &[&str],
) -> Uuid {
    let project = a_project(app, project_slug).await;
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

/// An open design artefact - the shape that had no listing at all before.
async fn an_open_design_artifact(
    app: &TestApp,
    project_slug: &str,
    orientation_slug: &str,
    tools: &[&str],
) -> Uuid {
    let project = a_project(app, project_slug).await;
    let orientation: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
        .bind(orientation_slug)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let tools: Vec<String> = tools.iter().map(|s| s.to_string()).collect();
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, difficulty,
             status, orientation_id, design_subtype, design_tools)
         VALUES ($1, $2, 'x', 'design', 'design_artifact', 2,
                 'open', $3, 'interface', $4)
         RETURNING id",
    )
    .bind(project)
    .bind(format!("screen on {project_slug}"))
    .bind(orientation)
    .bind(&tools)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn pool(app: &TestApp, query: &str) -> Value {
    let resp = app.get(&format!("/api/open-slices{query}")).await;
    assert_eq!(resp.status(), 200, "the pool must answer");
    resp.json().await.unwrap()
}

fn slugs(body: &Value) -> Vec<String> {
    body["data"]["slices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["project_slug"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_pool_answers_for_a_trade_that_is_not_code() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "pool-code", Some("web-frontend-developer"), 2, &[]).await;
    an_open_design_artifact(&app, "pool-design", "design-product", &["figma"]).await;

    // The point of the generalisation: design work was in the table all along
    // and no listing showed it.
    let body = pool(&app, "?domain=design").await;
    assert_eq!(slugs(&body), vec!["pool-design"]);

    let slice = &body["data"]["slices"][0];
    assert_eq!(slice["domain"], "design");
    assert_eq!(slice["slice_type"], "design_artifact");
    assert_eq!(
        slice["subtype"], "interface",
        "the subtype comes from the domain's own column"
    );
}

#[tokio::test]
async fn a_domain_holds_every_surface_it_owns() {
    let app = TestApp::spawn().await;
    an_open_issue(
        &app,
        "pool-mixed-issue",
        Some("web-frontend-developer"),
        2,
        &[],
    )
    .await;
    an_open_design_artifact(&app, "pool-mixed-design", "design-product", &["figma"]).await;

    let all = pool(&app, "?limit=50").await;
    let found = slugs(&all);
    assert!(found.contains(&"pool-mixed-issue".to_string()));
    assert!(found.contains(&"pool-mixed-design".to_string()));
}

#[tokio::test]
async fn an_upstream_ticket_takes_its_trade_from_the_slice() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "pool-ticket", Some("web-frontend-developer"), 2, &[]).await;

    // `github_issue` belongs to no single trade in `slice_types`, so the
    // slice's own `primary_domain` has to answer for it.
    let body = pool(&app, "?domain=code").await;
    assert_eq!(slugs(&body), vec!["pool-ticket"]);
    assert_eq!(body["data"]["slices"][0]["domain"], "code");
}

#[tokio::test]
async fn a_surface_narrows_further_than_a_trade() {
    let app = TestApp::spawn().await;
    an_open_issue(
        &app,
        "pool-narrow-issue",
        Some("web-frontend-developer"),
        2,
        &[],
    )
    .await;
    an_open_design_artifact(&app, "pool-narrow-design", "design-product", &["figma"]).await;

    let body = pool(&app, "?slice_type=design_artifact").await;
    assert_eq!(slugs(&body), vec!["pool-narrow-design"]);
}

#[tokio::test]
async fn a_surface_nobody_knows_is_not_silently_everything() {
    let app = TestApp::spawn().await;
    an_open_design_artifact(&app, "pool-typo", "design-product", &["figma"]).await;

    // "nothing is open in design" and "that is not how the surface is spelled"
    // are different answers, and only one tells the caller to fix the request.
    let resp = app.get("/api/open-slices?slice_type=design_artefact").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn a_trade_nobody_knows_is_refused_too() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/open-slices?domain=charpente").await;
    assert_eq!(resp.status(), 400, "a domain is checked against the list");

    let resp = app.get("/api/open-slices?orientation=metier-invente").await;
    assert_eq!(resp.status(), 404, "an orientation is resolved or refused");
}

#[tokio::test]
async fn the_tag_filter_reads_each_trade_in_its_own_words() {
    let app = TestApp::spawn().await;
    an_open_design_artifact(&app, "pool-figma", "design-product", &["figma"]).await;
    an_open_design_artifact(&app, "pool-penpot", "design-product", &["penpot"]).await;

    let body = pool(&app, "?domain=design&tag=figma").await;
    assert_eq!(slugs(&body), vec!["pool-figma"]);

    // And the repository's stack must not leak in as a tag on work it says
    // nothing about: these projects are `rust`, the design artefacts are not.
    let body = pool(&app, "?domain=design&tag=rust").await;
    assert!(
        body["data"]["slices"].as_array().unwrap().is_empty(),
        "a design artefact on a Rust repository is not tagged rust"
    );
}

#[tokio::test]
async fn a_ticket_still_falls_back_to_the_repository_stack() {
    let app = TestApp::spawn().await;
    // The slice says nothing, so the repository's stack answers for it - the
    // rule the code feed has always had, kept.
    an_open_issue(&app, "pool-fallback", Some("systems-programmer"), 2, &[]).await;
    an_open_issue(&app, "pool-own", Some("systems-programmer"), 2, &["zig"]).await;

    assert_eq!(
        pool(&app, "?tag=rust").await["data"]["slices"][0]["project_slug"],
        "pool-fallback"
    );
    assert_eq!(
        pool(&app, "?tag=zig").await["data"]["slices"][0]["project_slug"],
        "pool-own"
    );
}

#[tokio::test]
async fn claimed_work_leaves_the_pool() {
    let app = TestApp::spawn().await;
    let slice = an_open_design_artifact(&app, "pool-claimed", "design-product", &["figma"]).await;

    let before = pool(&app, "?domain=design").await;
    assert_eq!(before["data"]["slices"].as_array().unwrap().len(), 1);

    app.register_user("pool_claimer").await;
    let claimer: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'pool_claimer'")
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

    // Different limit, so the previous answer's cache entry does not decide
    // this one.
    let after = pool(&app, "?domain=design&limit=29").await;
    assert!(
        after["data"]["slices"].as_array().unwrap().is_empty(),
        "listing work somebody already took wastes the reader's time"
    );
}

#[tokio::test]
async fn the_pool_stops_at_an_entry_difficulty() {
    let app = TestApp::spawn().await;
    an_open_issue(&app, "pool-easy", Some("web-frontend-developer"), 2, &[]).await;
    an_open_issue(&app, "pool-hard", Some("web-frontend-developer"), 5, &[]).await;

    let body = pool(&app, "?orientation=web-frontend-developer").await;
    assert_eq!(
        slugs(&body),
        vec!["pool-easy"],
        "an entry pool is not the backlog"
    );

    let wider = pool(&app, "?orientation=web-frontend-developer&max_difficulty=5").await;
    assert_eq!(wider["data"]["slices"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn the_deprecated_code_feed_answers_from_the_same_pool() {
    let app = TestApp::spawn().await;
    an_open_issue(
        &app,
        "pool-alias-code",
        Some("web-frontend-developer"),
        2,
        &["rust"],
    )
    .await;
    an_open_design_artifact(&app, "pool-alias-design", "design-product", &["figma"]).await;

    let resp = app
        .get("/api/code/first-issues?orientation=web-frontend-developer")
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    // Same rows, same names as before: nothing that calls the old route has
    // to change until it moves.
    let issues = body["data"]["issues"].as_array().unwrap();
    assert_eq!(
        issues.len(),
        1,
        "design work must not appear in the code feed"
    );
    assert_eq!(issues[0]["project_slug"], "pool-alias-code");
    assert_eq!(issues[0]["issue_url"], "https://github.com/x/y/issues/1");
    assert_eq!(issues[0]["languages"][0], "rust");
    assert!(issues[0]["ingested_at"].is_string());
}
