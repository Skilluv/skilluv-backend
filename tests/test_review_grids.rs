//! Review criteria, and the fallback that stops verification running blind.

mod common;
use common::TestApp;

#[tokio::test]
async fn every_code_reviewer_group_has_a_grid() {
    let app = TestApp::spawn().await;

    // A family with no grid means a reviewer opening that queue has nothing
    // to apply, and two reviewers apply two different standards.
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.reviewer_group
           FROM orientations o
          WHERE o.primary_domain = 'code'
            AND NOT o.is_archived
            AND o.reviewer_group IS NOT NULL
            AND NOT EXISTS (
                SELECT 1 FROM review_grids g
                 WHERE g.domain = o.primary_domain
                   AND g.reviewer_group = o.reviewer_group)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        missing.is_empty(),
        "families with no review grid: {missing:?}"
    );
}

#[tokio::test]
async fn a_domain_has_exactly_one_default_grid() {
    let app = TestApp::spawn().await;

    let defaults: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM review_grids WHERE domain = 'code' AND reviewer_group IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(defaults, 1, "the fallback must be unambiguous");

    // And a second one cannot be added by accident.
    let refused = sqlx::query(
        "INSERT INTO review_grids (domain, display_name, criteria)
         VALUES ('code', 'doublon', '[{\"criterion\": \"x\", \"looks_like\": \"y\"}]')",
    )
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_grid_cannot_be_empty() {
    let app = TestApp::spawn().await;

    // A grid with no criteria is worse than no grid: it looks like a
    // standard has been set.
    for shape in ["[]", "{}", "\"nope\""] {
        let refused = sqlx::query(
            "INSERT INTO review_grids (domain, reviewer_group, display_name, criteria)
             VALUES ('code', 'temp-empty', 'vide', $1::jsonb)",
        )
        .bind(shape)
        .execute(&app.db)
        .await;
        assert!(refused.is_err(), "{shape} must be refused");
    }
}

#[tokio::test]
async fn every_criterion_says_what_it_looks_like() {
    let app = TestApp::spawn().await;

    // "Performance" alone is a word. What a reviewer needs is what counts as
    // meeting it, which is what separates a grid from a checklist.
    let vague: Vec<String> = sqlx::query_scalar(
        "SELECT g.display_name
           FROM review_grids g,
                LATERAL jsonb_array_elements(g.criteria) AS c
          WHERE c->>'criterion' IS NULL
             OR c->>'looks_like' IS NULL
             OR btrim(c->>'looks_like') = ''",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        vague.is_empty(),
        "criteria with nothing to apply: {vague:?}"
    );
}

#[tokio::test]
async fn the_common_grid_states_the_two_non_negotiables() {
    let app = TestApp::spawn().await;

    let criteria: serde_json::Value = sqlx::query_scalar(
        "SELECT criteria FROM review_grids WHERE domain = 'code' AND reviewer_group IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let text = criteria.to_string();
    // Two rules the domain charter makes explicit: undocumented code is
    // refused, and AI assistance is declared rather than hidden.
    assert!(
        text.contains("Documentation"),
        "documentation is not optional"
    );
    assert!(text.contains("IA"), "AI use is declared, not concealed");
}
