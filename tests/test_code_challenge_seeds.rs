//! The seeded catalogue: present, unreviewed, and never offered as if it
//! were finished.

mod common;
use common::TestApp;

#[tokio::test]
async fn every_code_trade_has_challenges() {
    let app = TestApp::spawn().await;

    // A trade with an empty catalogue is a trade the platform claims to
    // support and cannot. Counted through the seeded titles, which carry the
    // orientation they were written for.
    let seeded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'code' AND is_training AND status = 'draft'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(seeded, 138);
}

#[tokio::test]
async fn a_seed_is_never_published_without_review() {
    let app = TestApp::spawn().await;

    // The title and the intent came from a backlog; the constraints, the
    // numbers and the out-of-scope need an author. Offering that to somebody
    // learning would be worse than an empty catalogue.
    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'code' AND is_training AND status = 'published'
            AND instructions LIKE '%Ce qui sera regardé%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(published, 0, "a seed must be reviewed before it is offered");
}

#[tokio::test]
async fn every_seed_carries_criteria() {
    let app = TestApp::spawn().await;

    // Without a rubric the verifier evaluates against the instructions alone.
    // Inheriting the family grid means criteria exist from day one rather
    // than from whenever somebody remembers to write some.
    let blind: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'code' AND is_training AND status = 'draft'
            AND (evaluation_rubric IS NULL
                 OR jsonb_array_length(evaluation_rubric) = 0)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(blind, 0);
}

#[tokio::test]
async fn a_seed_states_what_has_to_come_out_of_it() {
    let app = TestApp::spawn().await;

    // Every one names an artefact — a merged contribution, a published
    // package, something in service. A challenge whose instructions do not
    // say what to deliver produces submissions nobody can compare.
    let vague: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'code' AND is_training AND status = 'draft'
            AND instructions NOT LIKE '%Ce qui est attendu%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(vague, 0);
}

#[tokio::test]
async fn harder_families_are_marked_harder() {
    let app = TestApp::spawn().await;

    // Not a judgement about the people: it says how much has to be true at
    // once before the work is verifiable at all.
    let compiler_seeds: Vec<i16> = sqlx::query_scalar(
        "SELECT DISTINCT difficulty FROM challenge_templates
          WHERE skill_domain = 'code' AND is_training AND status = 'draft'
            AND title ILIKE '%compiler%'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        compiler_seeds.iter().all(|d| *d >= 4),
        "compiler work cannot be half-done: {compiler_seeds:?}"
    );
}

#[tokio::test]
async fn no_seed_claims_a_language_it_does_not_have() {
    let app = TestApp::spawn().await;

    // The polyglot badge counts distinct languages, so a wrong one is worse
    // than none. Trades spanning several leave it unset.
    let empty_language: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'code' AND is_training AND status = 'draft'
            AND language IS NOT NULL AND btrim(language) = ''",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(empty_language, 0, "an empty string is not a language");
}
