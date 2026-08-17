//! The service's list of contest kinds and the database's must agree.
//!
//! ## Why this test exists
//!
//! `tournaments_kind_check` cannot be extended, only replaced. Migration 0189
//! added the code kinds; 0223 added the AI kinds and, being a rewrite,
//! silently dropped the code ones. Nothing failed: `VALID_KINDS` in the
//! service already listed all of them, so a code golf passed validation in
//! Rust and was refused by the database with an error naming a constraint.
//!
//! Two lists that must agree and no check that they do is a bug waiting for
//! the next domain. This is that check.

mod common;
use common::TestApp;
use uuid::Uuid;

#[tokio::test]
async fn every_kind_the_service_allows_is_a_kind_the_database_accepts() {
    let app = TestApp::spawn().await;

    for kind in skilluv_backend::services::tournament::VALID_KINDS {
        let inserted = sqlx::query(
            "INSERT INTO tournaments (slug, name, kind, starts_at, ends_at)
             VALUES ($1, $1, $2, NOW(), NOW() + INTERVAL '1 day')",
        )
        .bind(format!("kind-check-{kind}"))
        .bind(kind)
        .execute(&app.db)
        .await;

        assert!(
            inserted.is_ok(),
            "the service allows '{kind}' and the database refuses it — a migration \
             restated tournaments_kind_check without it: {inserted:?}"
        );
    }
}

#[tokio::test]
async fn every_kind_the_database_accepts_is_one_the_service_knows() {
    let app = TestApp::spawn().await;

    // The other direction. A kind in the constraint that the service does not
    // know is a kind nothing can create through the API, and a row nothing
    // knows how to score.
    let definition: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint
          WHERE conrelid = 'tournaments'::regclass AND conname = 'tournaments_kind_check'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    for kind in skilluv_backend::services::tournament::VALID_KINDS {
        assert!(
            definition.contains(kind),
            "the constraint does not mention '{kind}': {definition}"
        );
    }

    // Count the quoted literals in the constraint. More of them than the
    // service knows about means somebody added a kind to the database and
    // forgot the other half.
    let quoted = definition.matches("::character varying").count();
    assert_eq!(
        quoted,
        skilluv_backend::services::tournament::VALID_KINDS.len(),
        "the constraint and VALID_KINDS list different numbers of kinds: {definition}"
    );
}

#[tokio::test]
async fn a_code_golf_can_actually_be_created() {
    let app = TestApp::spawn().await;

    // The concrete case the regression broke, through the service rather than
    // straight SQL — so the scoring direction is exercised too.
    app.register_user("kinds_admin").await;
    let admin: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kinds_admin'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let created = skilluv_backend::services::tournament::create_tournament(
        &app.db,
        admin,
        skilluv_backend::services::tournament::CreateTournamentInput {
            season_id: None,
            slug: "golf-after-the-merge".into(),
            name: "Code golf".into(),
            description: None,
            kind: "code_golf".into(),
            format: Some("ladder".into()),
            prize_pool_fragments: None,
            prize_pool_gp: None,
            sponsor_enterprise_id: None,
            sponsor_logo_url: None,
            sponsor_blurb: None,
            registration_opens_at: None,
            starts_at: chrono::Utc::now(),
            ends_at: chrono::Utc::now() + chrono::Duration::days(7),
            skill_domain: Some("code".into()),
            rules: Some(serde_json::json!({
                "language": "python",
                "problem_url": "https://example.test/problem",
            })),
        },
    )
    .await;

    let tournament = created.expect("a code golf must be creatable after every migration");
    assert_eq!(tournament.scoring_direction, "lower_is_better");
}
