//! The design domain's guides, and the invariant that caught their absence.
//!
//! Design was the only opened domain with **zero** rows in `content_guides`,
//! while `docs/design/ONBOARDING.md`, `TOOLKIT.md`, `BRIEF-TEMPLATES.md` and
//! `WRITEUP-TEMPLATES.md` all existed and were written. Nothing had ported
//! them, so `/design/toolkit` and `/design/onboarding` on the front read an
//! endpoint that answered an empty list (SKI-186).
//!
//! Nothing failed. A guide catalogue with a hole in it looks exactly like a
//! guide catalogue, from the backend's side — which is why the last test here
//! is written against **every** domain rather than against design, and is the
//! one that matters going forward.

mod common;
use common::TestApp;
use serde_json::Value;

async fn count(app: &TestApp, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn every_design_review_family_has_an_onboarding_guide() {
    let app = TestApp::spawn().await;

    // The families come from the reviewer groups, so a trade added later
    // without a guide is caught here rather than by the designer who arrives
    // and finds nothing.
    let families: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE reviewer_group IS NOT NULL AND primary_domain = 'design'
            AND is_archived = FALSE",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        families.len() >= 13,
        "the design catalogue lost its families: {families:?}"
    );

    let mut missing = Vec::new();
    for family in &families {
        let has: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM content_guides
                             WHERE kind = 'onboarding' AND skill_domain = 'design'
                               AND reviewer_group = $1 AND is_published)",
        )
        .bind(family)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if !has {
            missing.push(family.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "families with no onboarding: {missing:?}"
    );
}

#[tokio::test]
async fn every_design_review_family_has_a_brief_template() {
    let app = TestApp::spawn().await;

    // The thirteen SKI-186 asked for. A brief template is what an editor loads
    // when writing a challenge, so a family without one is a family whose
    // challenges get written from scratch each time and drift.
    let templates = count(
        &app,
        "SELECT count(*) FROM content_guides
          WHERE kind = 'brief_template' AND skill_domain = 'design'
            AND reviewer_group IS NOT NULL",
    )
    .await;
    assert!(templates >= 13, "only {templates} design brief templates");

    // Plus the one that states the eight common sections, which the thirteen
    // build on rather than repeat.
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM content_guides
              WHERE slug = 'design-brief-common' AND is_published"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn the_design_toolkit_and_writeup_templates_are_served() {
    let app = TestApp::spawn().await;

    let body: Value = app
        .get("/api/guides?domain=design&kind=toolkit")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"].as_array().expect("a list");
    assert_eq!(items.len(), 1, "{body}");

    // The licence trap is the most expensive mistake in this domain, and the
    // toolkit is where somebody reads about it before delivering a font they
    // do not own the right to hand over.
    let slug = items[0]["slug"].as_str().unwrap();
    let one: Value = app
        .get(&format!("/api/guides/{slug}"))
        .await
        .json()
        .await
        .unwrap();
    let body_md = one["data"]["body_md"].as_str().unwrap();
    assert!(body_md.contains("licence"), "the toolkit omits licensing");

    // The three a designer actually writes: the version note, the critique,
    // and the case study.
    let body: Value = app
        .get("/api/guides?domain=design&kind=writeup_template")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 3, "{body}");
}

#[tokio::test]
async fn a_designer_arriving_is_served_something_in_their_language() {
    let app = TestApp::spawn().await;

    // These rows are English. The route falls back "asked for, then English,
    // then French", so a French reader gets the page rather than nothing —
    // which is the whole reason seeding one locale is acceptable.
    let resp = app
        .client
        .get(format!("{}/api/guides?domain=design", app.addr))
        .header("origin", "http://localhost:5173")
        .header("Accept-Language", "fr")
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let items = body["data"].as_array().expect("a list");
    assert!(
        !items.is_empty(),
        "a French reader gets nothing for design: {body}"
    );
}

/// The invariant that would have caught this, and will catch the next one.
///
/// Written against every open domain rather than against design, because the
/// failure was not that design was special — it was that nothing asked the
/// question. A domain can be opened, seeded with challenges, given reviewer
/// capabilities and a review grid, and still have no guide telling anybody how
/// to start. Nothing errors; the pages are just empty.
#[tokio::test]
async fn no_open_domain_is_left_without_an_onboarding_guide() {
    let app = TestApp::spawn().await;

    // A domain counts as open once it has live orientations — that is what
    // lets somebody declare the trade and hand work in.
    let domains: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT primary_domain FROM orientations
          WHERE is_archived = FALSE AND primary_domain IS NOT NULL
          ORDER BY primary_domain",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    let mut bare = Vec::new();
    for domain in &domains {
        let guides: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM content_guides
              WHERE kind = 'onboarding' AND skill_domain = $1 AND is_published",
        )
        .bind(domain)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if guides == 0 {
            bare.push(domain.clone());
        }
    }

    assert!(
        bare.is_empty(),
        "these domains are open and have no onboarding guide, so somebody \
         declaring the trade is served an empty page: {bare:?}"
    );
}

/// SKI-239 — design award categories exist, so `/design/awards` is not an
/// empty page rendered against a working endpoint.
///
/// Migration 0590 gave `award_categories` its `skill_domain` and said the
/// per-family seeding was a separate ticket. It stayed separate: every other
/// domain seeded its categories with its practice data, and design got the
/// column without the rows.
#[tokio::test]
async fn design_has_award_categories_and_they_are_scoped_to_design() {
    let app = TestApp::spawn().await;

    let scoped: i64 =
        sqlx::query_scalar("SELECT count(*) FROM award_categories WHERE skill_domain = 'design'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(scoped >= 6, "only {scoped} design award categories");

    // Work-shaped and person-shaped both present: nominating a deliverable and
    // nominating somebody for a year of reviewing are different awards, and a
    // catalogue with only one kind cannot express the other.
    for subject in ["deliverable", "user"] {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM award_categories
              WHERE skill_domain = 'design' AND subject_type = $1",
        )
        .bind(subject)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(n > 0, "no design award nominates a {subject}");
    }

    // The endpoint the front reads, filtered the way `/design/awards` filters
    // it — the `domain` parameter migration 0590 added the column for.
    let body: Value = app
        .get("/api/awards/categories?domain=design")
        .await
        .json()
        .await
        .unwrap();
    let listed = body["data"]["categories"].as_array().expect("a list");
    assert!(
        !listed.is_empty(),
        "the categories endpoint serves nothing for design: {body}"
    );
    // The filter returns design's own and the cross-cutting ones, never
    // another family's — an awards page showing code categories is worse than
    // an empty one.
    assert!(
        listed
            .iter()
            .all(|c| c["skill_domain"] == "design" || c["skill_domain"].is_null()),
        "another domain's categories leaked into design: {body}"
    );
}
