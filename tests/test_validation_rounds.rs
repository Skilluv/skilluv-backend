//! Rejection as a round, with a cap and a reason that says what to do.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn a_slice(app: &TestApp) -> Uuid {
    app.register_user("round_owner").await;
    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'round_owner'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet', 'user', $2) RETURNING id",
    )
    .bind(format!("round-{}", Uuid::new_v4().simple()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, difficulty)
         VALUES ($1, 'Tranche', 'x', 'code', 'github_issue', 3)
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn a_validator(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn record_rejection(app: &TestApp, slice: Uuid, validator: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO slice_validation_decisions
            (slice_id, validator_id, decision, reason, blocking_reason)
         VALUES ($1, $2, 'reject', 'il manque les tests', 'tests_missing')",
    )
    .bind(slice)
    .bind(validator)
    .execute(&app.db)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn rounds_count_themselves() {
    let app = TestApp::spawn().await;
    let slice = a_slice(&app).await;
    let validator = a_validator(&app, "round_val").await;

    for _ in 0..3 {
        record_rejection(&app, slice, validator).await.unwrap();
    }

    let rounds: Vec<i16> = sqlx::query_scalar(
        "SELECT round FROM slice_validation_decisions WHERE slice_id = $1 ORDER BY round",
    )
    .bind(slice)
    .fetch_all(&app.db)
    .await
    .unwrap();

    // Derived, never supplied: a caller cannot restart the count and hide
    // how many passes a slice has taken.
    assert_eq!(rounds, vec![1, 2, 3]);
}

#[tokio::test]
async fn a_sixth_pass_is_refused_and_says_why() {
    let app = TestApp::spawn().await;
    let slice = a_slice(&app).await;
    let validator = a_validator(&app, "round_cap").await;

    for _ in 0..5 {
        record_rejection(&app, slice, validator).await.unwrap();
    }

    let refused = record_rejection(&app, slice, validator).await;
    let message = format!("{:?}", refused.unwrap_err());

    // The cap is not about the sixth attempt being worthless. By then the
    // problem is scope or assignment, and the message has to say so rather
    // than read as another rejection.
    assert!(
        message.contains("five validation rounds"),
        "the refusal must name the real problem: {message}"
    );
    assert!(message.contains("human decision"), "{message}");
}

#[tokio::test]
async fn a_rejection_must_name_the_kind_of_blocker() {
    let app = TestApp::spawn().await;
    let slice = a_slice(&app).await;
    let validator = a_validator(&app, "round_noreason").await;

    // "Rejected" alone leaves a contributor guessing between fixing CI,
    // renaming a variable, and having taken the wrong slice.
    let refused = sqlx::query(
        "INSERT INTO slice_validation_decisions (slice_id, validator_id, decision, reason)
         VALUES ($1, $2, 'reject', 'non')",
    )
    .bind(slice)
    .bind(validator)
    .execute(&app.db)
    .await;

    assert!(refused.is_err());
}

#[tokio::test]
async fn an_approval_carries_no_blocker() {
    let app = TestApp::spawn().await;
    let slice = a_slice(&app).await;
    let validator = a_validator(&app, "round_approve").await;

    let contradictory = sqlx::query(
        "INSERT INTO slice_validation_decisions
            (slice_id, validator_id, decision, blocking_reason)
         VALUES ($1, $2, 'approve', 'ci_failing')",
    )
    .bind(slice)
    .bind(validator)
    .execute(&app.db)
    .await;
    assert!(
        contradictory.is_err(),
        "approved and blocked is not a state"
    );

    let accepted = sqlx::query(
        "INSERT INTO slice_validation_decisions (slice_id, validator_id, decision)
         VALUES ($1, $2, 'approve')",
    )
    .bind(slice)
    .bind(validator)
    .execute(&app.db)
    .await;
    assert!(accepted.is_ok(), "{accepted:?}");
}

#[tokio::test]
async fn an_unknown_blocker_is_refused_before_it_reaches_the_database() {
    let app = TestApp::spawn().await;
    let slice = a_slice(&app).await;
    let validator = a_validator(&app, "round_badreason").await;

    let refused = skilluv_backend::services::slice_validation::reject(
        &app.db, slice, validator, "un motif", "inventé",
    )
    .await;

    let message = format!("{:?}", refused.expect_err("must refuse"));
    assert!(
        message.contains("unknown blocking reason"),
        "the answer must list what is expected: {message}"
    );
}
