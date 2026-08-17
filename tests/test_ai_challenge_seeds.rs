//! Forty-one seeded AI challenges: drafts, with criteria from the first day.

mod common;
use common::TestApp;

#[tokio::test]
async fn the_ai_catalogue_is_not_empty() {
    let app = TestApp::spawn().await;

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'ai' AND is_training = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Ten trades with an empty catalogue are ten trades the platform claims
    // to support and cannot.
    assert!(count >= 41, "expected at least 41 seeds, got {count}");
}

#[tokio::test]
async fn every_seed_is_a_draft() {
    let app = TestApp::spawn().await;

    // The intent comes from the backlog; the full brief needs an author who
    // knows the trade. A challenge nobody reviewed must not be offered to
    // somebody learning.
    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'ai' AND is_training = TRUE AND status <> 'draft'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(published, 0);
}

#[tokio::test]
async fn every_seed_carries_a_rubric() {
    let app = TestApp::spawn().await;

    // Verification with no rubric asks a model whether work is good with no
    // statement of what good means, and it answers anyway.
    let rubricless: Vec<String> = sqlx::query_scalar(
        "SELECT title FROM challenge_templates
          WHERE skill_domain = 'ai' AND is_training = TRUE
            AND (evaluation_rubric IS NULL
                 OR jsonb_array_length(evaluation_rubric) = 0)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(rubricless.is_empty(), "no criteria for: {rubricless:?}");
}

#[tokio::test]
async fn every_seed_says_what_is_expected_of_it() {
    let app = TestApp::spawn().await;

    let thin: Vec<String> = sqlx::query_scalar(
        "SELECT title FROM challenge_templates
          WHERE skill_domain = 'ai' AND is_training = TRUE
            AND (instructions NOT LIKE '%Ce qui est attendu%'
                 OR length(instructions) < 300)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(thin.is_empty(), "briefs missing their shape: {thin:?}");
}

#[tokio::test]
async fn the_two_non_negotiables_are_in_every_brief() {
    let app = TestApp::spawn().await;

    // Measured on unseen data, and obtainable again. The two most common
    // reasons an AI submission comes back.
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT title FROM challenge_templates
          WHERE skill_domain = 'ai' AND is_training = TRUE
            AND (instructions NOT LIKE '%que le modèle n''a pas vues%'
                 OR instructions NOT LIKE '%graines, versions et données figées%')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(missing.is_empty(), "briefs missing the rule: {missing:?}");
}

#[tokio::test]
async fn the_families_get_their_own_criteria_not_the_default() {
    let app = TestApp::spawn().await;

    // A safety challenge judged on the generic grid loses the criteria that
    // make it a safety challenge — disclosure, dual use.
    let has_disclosure: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'ai' AND is_training = TRUE
            AND evaluation_rubric::TEXT LIKE '%ivulgation%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(
        has_disclosure >= 3,
        "the safety seeds should carry the safety grid, got {has_disclosure}"
    );
}
